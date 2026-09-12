use paynal::evaluator::VariableContext;
use paynal::models::{AssertSpec, RequestSpec};
use paynal::runner::HttpRunner;
use std::collections::HashMap;
use tempfile::NamedTempFile;
use std::io::Write;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn create_test_runner() -> HttpRunner {
    HttpRunner::new(true, None)
}

#[tokio::test]
async fn test_assert_status_range_and_status_in() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/status-201"))
        .respond_with(ResponseTemplate::new(201))
        .mount(&mock_server)
        .await;

    let runner = create_test_runner();
    let ctx = VariableContext::new();

    let req = RequestSpec {
        method: "GET".to_string(),
        url: format!("{}/status-201", mock_server.uri()),
        ..Default::default()
    };

    // 1. statusRange "2xx" and statusIn [200, 201] pass on 201
    let assert_pass = AssertSpec {
        status_range: Some("2xx".to_string()),
        status_in: Some(vec![200, 201]),
        ..Default::default()
    };
    let res = runner.execute(&req, Some(&assert_pass), &ctx).await.unwrap();
    assert!(res.asserts_passed);

    // 2. statusRange "3xx" fails on 201
    let assert_fail_range = AssertSpec {
        status_range: Some("3xx".to_string()),
        ..Default::default()
    };
    let res2 = runner.execute(&req, Some(&assert_fail_range), &ctx).await.unwrap();
    assert!(!res2.asserts_passed);

    // 3. statusIn [400, 404] fails on 201
    let assert_fail_in = AssertSpec {
        status_in: Some(vec![400, 404]),
        ..Default::default()
    };
    let res3 = runner.execute(&req, Some(&assert_fail_in), &ctx).await.unwrap();
    assert!(!res3.asserts_passed);
}

#[tokio::test]
async fn test_assert_header_contains_and_header_regex() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/headers"))
        .respond_with(
            ResponseTemplate::new(200)
                .append_header("Content-Type", "application/json; charset=utf-8")
                .append_header("X-Custom-Trace", "trace-abc-12345-xyz"),
        )
        .mount(&mock_server)
        .await;

    let runner = create_test_runner();
    let ctx = VariableContext::new();

    let req = RequestSpec {
        method: "GET".to_string(),
        url: format!("{}/headers", mock_server.uri()),
        ..Default::default()
    };

    let mut hc = HashMap::new();
    hc.insert("Content-Type".to_string(), "application/json".to_string());

    let mut hr = HashMap::new();
    hr.insert("X-Custom-Trace".to_string(), r"trace-[a-z]+-\d+-[a-z]+".to_string());

    let assert_spec = AssertSpec {
        header_contains: Some(hc),
        header_regex: Some(hr),
        ..Default::default()
    };

    let res = runner.execute(&req, Some(&assert_spec), &ctx).await.unwrap();
    assert!(res.asserts_passed, "Header contains and regex assertions should pass: {:?}", res.assert_messages);
}

#[tokio::test]
async fn test_assert_numeric_length_and_type() {
    let mock_server = MockServer::start().await;

    let payload = serde_json::json!({
        "order": {
            "id": 105,
            "total": 99.95,
            "items": ["apple", "banana", "cherry"],
            "notes": "urgent delivery"
        }
    });

    Mock::given(method("GET"))
        .and(path("/order"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&payload))
        .mount(&mock_server)
        .await;

    let runner = create_test_runner();
    let ctx = VariableContext::new();

    let req = RequestSpec {
        method: "GET".to_string(),
        url: format!("{}/order", mock_server.uri()),
        ..Default::default()
    };

    let mut gt = HashMap::new();
    gt.insert("$.order.total".to_string(), 50.0);

    let mut lte = HashMap::new();
    lte.insert("$.order.total".to_string(), 100.0);

    let mut len = HashMap::new();
    len.insert("$.order.items".to_string(), 3);

    let mut types = HashMap::new();
    types.insert("$.order.items".to_string(), "array".to_string());
    types.insert("$.order.id".to_string(), "number".to_string());
    types.insert("$.order.notes".to_string(), "string".to_string());

    let assert_spec = AssertSpec {
        json_gt: Some(gt),
        json_lte: Some(lte),
        json_length: Some(len),
        json_type: Some(types),
        ..Default::default()
    };

    let res = runner.execute(&req, Some(&assert_spec), &ctx).await.unwrap();
    assert!(res.asserts_passed, "All numeric, length, and type assertions should pass: {:?}", res.assert_messages);
}

#[tokio::test]
async fn test_assert_json_schema_validation() {
    let mock_server = MockServer::start().await;

    let valid_user = serde_json::json!({
        "id": 1,
        "name": "Leanne Graham",
        "email": "sincere@april.biz"
    });

    Mock::given(method("GET"))
        .and(path("/user/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&valid_user))
        .mount(&mock_server)
        .await;

    let runner = create_test_runner();
    let ctx = VariableContext::new();

    let req = RequestSpec {
        method: "GET".to_string(),
        url: format!("{}/user/1", mock_server.uri()),
        ..Default::default()
    };

    let schema = r#"{
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": {
            "id": { "type": "integer" },
            "name": { "type": "string" },
            "email": { "type": "string" }
        },
        "required": ["id", "name", "email"]
    }"#;

    // 1. Inline schema validation
    let assert_inline = AssertSpec {
        schema: Some(schema.to_string()),
        ..Default::default()
    };
    let res = runner.execute(&req, Some(&assert_inline), &ctx).await.unwrap();
    assert!(res.asserts_passed, "Inline schema validation should pass: {:?}", res.assert_messages);

    // 2. Schema file validation
    let mut schema_file = NamedTempFile::new().unwrap();
    schema_file.write_all(schema.as_bytes()).unwrap();

    let assert_file = AssertSpec {
        schema: Some(schema_file.path().to_str().unwrap().to_string()),
        ..Default::default()
    };
    let res2 = runner.execute(&req, Some(&assert_file), &ctx).await.unwrap();
    assert!(res2.asserts_passed, "Schema file validation should pass: {:?}", res2.assert_messages);
}
