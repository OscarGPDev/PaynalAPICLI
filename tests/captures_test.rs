use paynal::evaluator::VariableContext;
use paynal::models::RequestSpec;
use paynal::runner::HttpRunner;
use std::collections::HashMap;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn create_test_runner() -> HttpRunner {
    HttpRunner::new(true, None)
}

#[tokio::test]
async fn test_capture_header() {
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/header-capture"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("X-Session-Token", "session-token-xyz-123")
                .set_body_string("OK"),
        )
        .mount(&mock_server)
        .await;

    let runner = create_test_runner();
    let mut ctx = VariableContext::new();

    let req = RequestSpec {
        method: "GET".to_string(),
        url: format!("{}/header-capture", mock_server.uri()),
        ..Default::default()
    };

    let result = runner.execute(&req, None, &ctx).await.unwrap();

    let mut captures = HashMap::new();
    captures.insert("mySession".to_string(), "header.X-Session-Token".to_string());

    runner.extract_captures(&result, &captures, &mut ctx);

    assert_eq!(
        ctx.get("mySession").map(|s| s.as_str()),
        Some("session-token-xyz-123"),
        "Extracted header should be present in VariableContext"
    );
}

#[tokio::test]
async fn test_capture_jsonpath_with_prefix() {
    let mock_server = MockServer::start().await;
    let json_body = serde_json::json!({
        "auth": {
            "token": "bearer-token-abc-456",
            "expiresIn": 3600
        }
    });

    Mock::given(method("POST"))
        .and(path("/login"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&json_body))
        .mount(&mock_server)
        .await;

    let runner = create_test_runner();
    let mut ctx = VariableContext::new();

    let req = RequestSpec {
        method: "POST".to_string(),
        url: format!("{}/login", mock_server.uri()),
        ..Default::default()
    };

    let result = runner.execute(&req, None, &ctx).await.unwrap();

    let mut captures = HashMap::new();
    captures.insert("token".to_string(), "$.auth.token".to_string());
    captures.insert("expiry".to_string(), "$.auth.expiresIn".to_string());

    runner.extract_captures(&result, &captures, &mut ctx);

    assert_eq!(ctx.get("token").map(|s| s.as_str()), Some("bearer-token-abc-456"));
    assert_eq!(ctx.get("expiry").map(|s| s.as_str()), Some("3600"));
}

#[tokio::test]
async fn test_capture_jsonpath_without_prefix_demonstrates_p1_3() {
    // This test documents and verifies P1-3:
    // Captures without '$.' prefix do NOT capture with jsonpath_lib 0.3
    let mock_server = MockServer::start().await;
    let json_body = serde_json::json!({
        "userId": 999
    });

    Mock::given(method("GET"))
        .and(path("/info"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&json_body))
        .mount(&mock_server)
        .await;

    let runner = create_test_runner();
    let mut ctx = VariableContext::new();

    let req = RequestSpec {
        method: "GET".to_string(),
        url: format!("{}/info", mock_server.uri()),
        ..Default::default()
    };

    let result = runner.execute(&req, None, &ctx).await.unwrap();

    let mut captures = HashMap::new();
    captures.insert("noPrefixId".to_string(), "userId".to_string());

    runner.extract_captures(&result, &captures, &mut ctx);

    // P1-3 fixed: capture without '$.' is normalized and succeeds!
    assert_eq!(
        ctx.get("noPrefixId").map(|s| s.as_str()),
        Some("999"),
        "P1-3 fixed: capture without '$.' is normalized and extracts successfully"
    );
}

#[tokio::test]
async fn test_chaining_captures_across_steps() {
    let mock_server = MockServer::start().await;

    // Step 1: Login endpoint returns token and correlation header
    Mock::given(method("POST"))
        .and(path("/api/auth/login"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("X-Correlation-Id", "corr-8888")
                .set_body_json(serde_json::json!({
                    "token": "jwt-token-step1"
                })),
        )
        .mount(&mock_server)
        .await;

    // Step 2: Protected endpoint verifies headers interpolated from step 1
    Mock::given(method("GET"))
        .and(path("/api/users/me"))
        .and(header("Authorization", "Bearer jwt-token-step1"))
        .and(header("X-Trace", "corr-8888"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({
                    "username": "tester",
                    "role": "admin"
                })),
        )
        .mount(&mock_server)
        .await;

    let runner = create_test_runner();
    let mut ctx = VariableContext::new();

    // Execute Step 1
    let step1_req = RequestSpec {
        method: "POST".to_string(),
        url: format!("{}/api/auth/login", mock_server.uri()),
        body: Some("{}".to_string()),
        ..Default::default()
    };
    let step1_res = runner.execute(&step1_req, None, &ctx).await.unwrap();

    let mut step1_captures = HashMap::new();
    step1_captures.insert("authToken".to_string(), "$.token".to_string());
    step1_captures.insert("traceId".to_string(), "header.X-Correlation-Id".to_string());

    runner.extract_captures(&step1_res, &step1_captures, &mut ctx);

    assert_eq!(ctx.get("authToken").map(|s| s.as_str()), Some("jwt-token-step1"));
    assert_eq!(ctx.get("traceId").map(|s| s.as_str()), Some("corr-8888"));

    // Execute Step 2 using interpolated variables
    let mut step2_headers = HashMap::new();
    step2_headers.insert("Authorization".to_string(), "Bearer ${authToken}".to_string());
    step2_headers.insert("X-Trace".to_string(), "${traceId}".to_string());

    let step2_req = RequestSpec {
        method: "GET".to_string(),
        url: format!("{}/api/users/me", mock_server.uri()),
        headers: step2_headers,
        ..Default::default()
    };

    let step2_res = runner.execute(&step2_req, None, &ctx).await.unwrap();
    assert_eq!(step2_res.status, 200);
    assert!(step2_res.body.contains("tester"));
}
