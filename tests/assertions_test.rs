use paynal::evaluator::VariableContext;
use paynal::models::{AssertSpec, MatchSpec, RequestSpec, StringOrVec};
use paynal::runner::HttpRunner;
use std::collections::HashMap;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn create_test_runner() -> HttpRunner {
    HttpRunner::new(true, None)
}

fn create_get_request(url: &str) -> RequestSpec {
    RequestSpec {
        method: "GET".to_string(),
        url: url.to_string(),
        ..Default::default()
    }
}

#[tokio::test]
async fn test_assert_status_exact_success() {
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/status-200"))
        .respond_with(ResponseTemplate::new(200).set_body_string("OK"))
        .mount(&mock_server)
        .await;

    let runner = create_test_runner();
    let ctx = VariableContext::new();
    let req = create_get_request(&format!("{}/status-200", mock_server.uri()));

    let assert_spec = AssertSpec {
        status: Some(200),
        ..Default::default()
    };

    let result = runner.execute(&req, Some(&assert_spec), &ctx).await.unwrap();
    assert_eq!(result.status, 200);
    assert!(result.asserts_passed, "Assertion for status 200 should pass");
}

#[tokio::test]
async fn test_assert_status_mismatch_failure() {
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/status-500"))
        .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
        .mount(&mock_server)
        .await;

    let runner = create_test_runner();
    let ctx = VariableContext::new();
    let req = create_get_request(&format!("{}/status-500", mock_server.uri()));

    let assert_spec = AssertSpec {
        status: Some(200),
        ..Default::default()
    };

    let result = runner.execute(&req, Some(&assert_spec), &ctx).await.unwrap();
    assert_eq!(result.status, 500);
    assert!(!result.asserts_passed, "Assertion expected 200 but received 500; should fail");
    assert!(result.assert_messages.iter().any(|m| m.contains("Status code mismatch")));
}

#[tokio::test]
async fn test_assert_json_fields_and_normalization() {
    let mock_server = MockServer::start().await;
    let json_body = serde_json::json!({
        "status": "success",
        "data": {
            "userId": 1042,
            "roles": ["admin", "developer"],
            "profile": {
                "active": true
            }
        }
    });

    Mock::given(method("GET"))
        .and(path("/user-data"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&json_body))
        .mount(&mock_server)
        .await;

    let runner = create_test_runner();
    let ctx = VariableContext::new();
    let req = create_get_request(&format!("{}/user-data", mock_server.uri()));

    let mut json_asserts = HashMap::new();
    // With explicit $.
    json_asserts.insert("$.status".to_string(), serde_json::json!("success"));
    json_asserts.insert("$.data.userId".to_string(), serde_json::json!(1042));
    json_asserts.insert("$.data.profile.active".to_string(), serde_json::json!(true));
    // Without $. (testing automatic normalization in assertions)
    json_asserts.insert("data.roles[0]".to_string(), serde_json::json!("admin"));

    let assert_spec = AssertSpec {
        status: Some(200),
        json: Some(json_asserts),
        ..Default::default()
    };

    let result = runner.execute(&req, Some(&assert_spec), &ctx).await.unwrap();
    assert!(result.asserts_passed, "All JSON assertions including normalized paths should pass");
}

#[tokio::test]
async fn test_assert_headers() {
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/headers-test"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("X-Custom-Header", "PaynalSecureValue")
                .insert_header("Content-Type", "application/json; charset=utf-8"),
        )
        .mount(&mock_server)
        .await;

    let runner = create_test_runner();
    let ctx = VariableContext::new();
    let req = create_get_request(&format!("{}/headers-test", mock_server.uri()));

    let mut header_asserts = HashMap::new();
    header_asserts.insert(
        "x-custom-header".to_string(),
        "PaynalSecureValue".to_string(),
    );

    let assert_spec = AssertSpec {
        headers: Some(header_asserts),
        ..Default::default()
    };

    let result = runner.execute(&req, Some(&assert_spec), &ctx).await.unwrap();
    assert!(result.asserts_passed, "Header assertion should pass case-insensitively");
}

#[tokio::test]
async fn test_assert_contains_and_regex() {
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/text-content"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Transaction TX_98765 approved successfully."))
        .mount(&mock_server)
        .await;

    let runner = create_test_runner();
    let ctx = VariableContext::new();
    let req = create_get_request(&format!("{}/text-content", mock_server.uri()));

    let assert_spec = AssertSpec {
        status: Some(200),
        contains: Some(MatchSpec::Single("approved".to_string())),
        icontains: Some(MatchSpec::Single("tx_98765".to_string())),
        not_contains: Some(MatchSpec::Single("error".to_string())),
        not_icontains: Some(MatchSpec::Single("rejected".to_string())),
        regex: Some(MatchSpec::Single(r"TX_\d+".to_string())),
        ..Default::default()
    };

    let result = runner.execute(&req, Some(&assert_spec), &ctx).await.unwrap();
    assert!(result.asserts_passed, "Contains and regex assertions should pass");
}

#[tokio::test]
async fn test_assert_exists_and_not_exists() {
    let mock_server = MockServer::start().await;
    let json_body = serde_json::json!({
        "sessionId": "abc-xyz",
        "account": {
            "balance": 150.0
        }
    });

    Mock::given(method("GET"))
        .and(path("/exists-test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&json_body))
        .mount(&mock_server)
        .await;

    let runner = create_test_runner();
    let ctx = VariableContext::new();
    let req = create_get_request(&format!("{}/exists-test", mock_server.uri()));

    let assert_spec = AssertSpec {
        exists: Some(StringOrVec::List(vec!["sessionId".to_string(), "$.account.balance".to_string()])),
        not_exists: Some(StringOrVec::List(vec!["nonExistentField".to_string(), "$.account.debt".to_string()])),
        ..Default::default()
    };

    let result = runner.execute(&req, Some(&assert_spec), &ctx).await.unwrap();
    assert!(result.asserts_passed, "Exists and not_exists assertions should pass");
}
