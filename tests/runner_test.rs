use paynal::evaluator::VariableContext;
use paynal::models::{AssertSpec, RequestSpec};
use paynal::runner::HttpRunner;
use std::collections::HashMap;
use std::time::Duration;
use wiremock::matchers::{body_string, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn create_test_runner() -> HttpRunner {
    HttpRunner::new(true, None)
}

#[tokio::test]
async fn test_runner_http_methods() {
    let mock_server = MockServer::start().await;

    for (m, p) in [
        ("GET", "/method-get"),
        ("POST", "/method-post"),
        ("PUT", "/method-put"),
        ("DELETE", "/method-delete"),
        ("PATCH", "/method-patch"),
    ] {
        Mock::given(method(m))
            .and(path(p))
            .respond_with(ResponseTemplate::new(200).set_body_string(m))
            .mount(&mock_server)
            .await;

        let runner = create_test_runner();
        let ctx = VariableContext::new();
        let req = RequestSpec {
            method: m.to_string(),
            url: format!("{}{}", mock_server.uri(), p),
            ..Default::default()
        };

        let res = runner.execute(&req, None, &ctx).await.unwrap();
        assert_eq!(res.status, 200);
        assert_eq!(res.body, m);
    }
}

#[tokio::test]
async fn test_runner_interpolates_url_headers_and_body() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/v1/orders/ORD-999"))
        .and(header("X-Api-Key", "secret-key-456"))
        .and(body_string(r#"{"customer": "Alice", "amount": 100}"#))
        .respond_with(ResponseTemplate::new(201).set_body_string(r#"{"status": "created"}"#))
        .mount(&mock_server)
        .await;

    let runner = create_test_runner();
    let mut ctx = VariableContext::new();
    ctx.set("orderId", "ORD-999");
    ctx.set("apiKey", "secret-key-456");
    ctx.set("customerName", "Alice");

    let mut headers = HashMap::new();
    headers.insert("X-Api-Key".to_string(), "${apiKey}".to_string());

    let req = RequestSpec {
        method: "POST".to_string(),
        url: format!("{}/api/v1/orders/${{orderId}}", mock_server.uri()),
        headers,
        body: Some(r#"{"customer": "${customerName}", "amount": 100}"#.to_string()),
        ..Default::default()
    };

    let res = runner.execute(&req, None, &ctx).await.unwrap();
    assert_eq!(res.status, 201);
}

#[tokio::test]
async fn test_assert_max_duration_exceeded() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/slow-endpoint"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(150))
                .set_body_string("slow response"),
        )
        .mount(&mock_server)
        .await;

    let runner = create_test_runner();
    let ctx = VariableContext::new();

    let req = RequestSpec {
        method: "GET".to_string(),
        url: format!("{}/slow-endpoint", mock_server.uri()),
        ..Default::default()
    };

    let assert_spec = AssertSpec {
        status: Some(200),
        max_duration_ms: Some(50), // 50ms limit, but server takes 150ms
        ..Default::default()
    };

    let res = runner.execute(&req, Some(&assert_spec), &ctx).await.unwrap();
    assert_eq!(res.status, 200);
    assert!(
        !res.asserts_passed,
        "max_duration_ms was exceeded so assertion should fail"
    );
    assert!(
        res.assert_messages
            .iter()
            .any(|m| m.contains("exceeded maximum")),
        "Message should report latency exceeded"
    );
}

#[tokio::test]
async fn test_assert_max_duration_passed() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/fast-endpoint"))
        .respond_with(ResponseTemplate::new(200).set_body_string("fast response"))
        .mount(&mock_server)
        .await;

    let runner = create_test_runner();
    let ctx = VariableContext::new();

    let req = RequestSpec {
        method: "GET".to_string(),
        url: format!("{}/fast-endpoint", mock_server.uri()),
        ..Default::default()
    };

    let assert_spec = AssertSpec {
        status: Some(200),
        max_duration_ms: Some(5000), // 5000ms limit
        ..Default::default()
    };

    let res = runner.execute(&req, Some(&assert_spec), &ctx).await.unwrap();
    assert_eq!(res.status, 200);
    assert!(res.asserts_passed, "Latency within bounds should pass");
}

#[tokio::test]
async fn test_request_timeout_ms_triggers_error() {
    // P1-1: Request-level timeout override causes request to abort when exceeded
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/delayed"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(200))
                .set_body_string("delayed response"),
        )
        .mount(&mock_server)
        .await;

    let runner = create_test_runner();
    let ctx = VariableContext::new();

    let req = RequestSpec {
        method: "GET".to_string(),
        url: format!("{}/delayed", mock_server.uri()),
        timeout_ms: Some(30), // 30ms timeout, server takes 200ms
        ..Default::default()
    };

    let result = runner.execute(&req, None, &ctx).await;
    assert!(
        result.is_err(),
        "Request should have failed with a timeout error"
    );
    let err = result.unwrap_err();
    let full_err_msg = format!("{:#}", err);
    assert!(
        full_err_msg.to_lowercase().contains("timeout") || full_err_msg.to_lowercase().contains("timed out"),
        "Full error chain should indicate timeout: {}",
        full_err_msg
    );
}

#[tokio::test]
async fn test_client_default_timeout_ms_triggers_error() {
    // P1-1: Client-level default timeout applies when request does not specify one
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/delayed-client"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(200))
                .set_body_string("delayed response"),
        )
        .mount(&mock_server)
        .await;

    let runner = HttpRunner::new_with_timeout(true, None, Some(30));
    let ctx = VariableContext::new();

    let req = RequestSpec {
        method: "GET".to_string(),
        url: format!("{}/delayed-client", mock_server.uri()),
        ..Default::default()
    };

    let result = runner.execute(&req, None, &ctx).await;
    assert!(
        result.is_err(),
        "Request should have failed with client default timeout"
    );
}
