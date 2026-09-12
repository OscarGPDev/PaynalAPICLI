use paynal::commands::{execute_exec, run_paynal_file};
use paynal::evaluator::VariableContext;
use paynal::manifest::PaynalManifest;
use paynal::runner::HttpRunner;
use std::collections::HashMap;
use std::io::Write;
use tempfile::NamedTempFile;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn create_test_runner() -> HttpRunner {
    HttpRunner::new(true, None)
}

#[tokio::test]
async fn test_single_request_execution_with_wiremock() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/ping"))
        .respond_with(ResponseTemplate::new(200).set_body_string("pong"))
        .mount(&mock_server)
        .await;

    let yaml_content = format!(
        r#"
version: "1.0"
name: "Ping Test"
vars:
  baseUrl: "{}"
request:
  method: "GET"
  url: "${{baseUrl}}/api/ping"
assert:
  status: 200
  contains: "pong"
"#,
        mock_server.uri()
    );

    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(yaml_content.as_bytes()).unwrap();

    let runner = create_test_runner();
    let manifest = PaynalManifest::default();

    let result = run_paynal_file(
        &runner,
        temp_file.path(),
        &manifest,
        None,
        None,
        false,
    )
    .await;

    assert_eq!(result.unwrap(), true, "Executing valid single request should pass all assertions");
}

#[tokio::test]
async fn test_routine_execution_with_captures_and_chaining() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/auth/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "token_abc_123"
        })))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/secure/data"))
        .and(header("Authorization", "Bearer token_abc_123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "secret": "classified"
        })))
        .mount(&mock_server)
        .await;

    let yaml_content = format!(
        r#"
version: "1.0"
name: "Auth and Fetch Routine"
vars:
  host: "{}"
steps:
  - id: "login"
    request:
      method: "POST"
      url: "${{host}}/auth/token"
    capture:
      tok: "$.access_token"
    assert:
      status: 200

  - id: "fetch"
    request:
      method: "GET"
      url: "${{host}}/secure/data"
      headers:
        Authorization: "Bearer ${{tok}}"
    assert:
      status: 200
      json:
        "$.secret": "classified"
"#,
        mock_server.uri()
    );

    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(yaml_content.as_bytes()).unwrap();

    let runner = create_test_runner();
    let manifest = PaynalManifest::default();

    let result = run_paynal_file(
        &runner,
        temp_file.path(),
        &manifest,
        None,
        None,
        false,
    )
    .await;

    assert_eq!(result.unwrap(), true, "Chained routine should execute and pass all assertions");
}

#[tokio::test]
async fn test_assertion_failure_returns_false_fixed_p0_1() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/broken-endpoint"))
        .respond_with(ResponseTemplate::new(500).set_body_string("Server Error"))
        .mount(&mock_server)
        .await;

    let yaml_content = format!(
        r#"
version: "1.0"
name: "Failing Assertion Request"
request:
  method: "GET"
  url: "{}/broken-endpoint"
assert:
  status: 200
"#,
        mock_server.uri()
    );

    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(yaml_content.as_bytes()).unwrap();

    let runner = create_test_runner();
    let manifest = PaynalManifest::default();

    let result = run_paynal_file(
        &runner,
        temp_file.path(),
        &manifest,
        None,
        None,
        false,
    )
    .await;

    let passed = result.expect("run_paynal_file should return Ok(false) on assertion failure");
    assert!(
        !passed,
        "P0-1 fixed: run_paynal_file reports false when assertions fail!"
    );
}

#[tokio::test]
async fn test_transport_error_returns_err() {
    let yaml_content = r#"
version: "1.0"
name: "Unreachable Endpoint"
request:
  method: "GET"
  url: "http://127.0.0.1:1"
assert:
  status: 200
"#;

    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(yaml_content.as_bytes()).unwrap();

    let runner = create_test_runner();
    let manifest = PaynalManifest::default();

    let result = run_paynal_file(
        &runner,
        temp_file.path(),
        &manifest,
        None,
        None,
        false,
    )
    .await;

    assert!(
        result.is_err(),
        "Transport / connection failure properly returns Err"
    );
}

#[test]
fn test_deny_unknown_fields_rejects_typos() {
    let yaml_with_asserts_typo = r#"
version: "1.0"
name: "Typo Test"
request:
  method: "GET"
  url: "https://example.com"
asserts:
  status: 200
"#;
    let res: Result<paynal::models::PaynalFile, _> = serde_yaml::from_str(yaml_with_asserts_typo);
    assert!(res.is_err(), "Typo 'asserts' must be rejected due to deny_unknown_fields");
    let err_msg = res.unwrap_err().to_string();
    assert!(err_msg.contains("unknown field"), "Error should indicate unknown field: {}", err_msg);

    let yaml_with_inner_typo = r#"
version: "1.0"
name: "Inner Typo Test"
request:
  method: "GET"
  url: "https://example.com"
assert:
  statuses: 200
"#;
    let res2: Result<paynal::models::PaynalFile, _> = serde_yaml::from_str(yaml_with_inner_typo);
    assert!(res2.is_err(), "Typo 'statuses' inside assert must be rejected");
}

#[tokio::test]
async fn test_execute_exec_fullmax_threads_does_not_panic() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/ping"))
        .respond_with(ResponseTemplate::new(200).set_body_string("pong"))
        .mount(&mock_server)
        .await;

    let yaml_content = format!(
        r#"
version: "1.0"
name: "Fullmax Test"
request:
  method: "GET"
  url: "{}/ping"
assert:
  status: 200
"#,
        mock_server.uri()
    );

    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(yaml_content.as_bytes()).unwrap();

    let exit_code = execute_exec(
        temp_file.path().to_str().unwrap().to_string(),
        None,
        true, // parallel = true
        Some("FULLMAX".to_string()), // threads = FULLMAX
        None,
        None,
        false,
        false,
    )
    .await
    .unwrap();

    assert_eq!(exit_code, 0, "FULLMAX execution should complete with exit code 0 without panicking");
}

#[tokio::test]
async fn test_execute_exec_returns_exit_code_1_on_assertion_failure() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/failing"))
        .respond_with(ResponseTemplate::new(404).set_body_string("Not Found"))
        .mount(&mock_server)
        .await;

    let yaml_content = format!(
        r#"
version: "1.0"
name: "404 Test"
request:
  method: "GET"
  url: "{}/failing"
assert:
  status: 200
"#,
        mock_server.uri()
    );

    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(yaml_content.as_bytes()).unwrap();

    let exit_code = execute_exec(
        temp_file.path().to_str().unwrap().to_string(),
        None,
        false,
        None,
        None,
        None,
        false,
        false,
    )
    .await
    .unwrap();

    assert_eq!(exit_code, 1, "Failed assertion must produce exit code 1 (P0-1)");
}

#[tokio::test]
async fn test_routine_continue_on_failure_default_false_halts_early() {
    // P1-6: By default, if a step in a routine fails its assertions,
    // subsequent steps must NOT be executed.
    let mock_server = MockServer::start().await;

    // Step 1 fails
    Mock::given(method("GET"))
        .and(path("/step1"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock_server)
        .await;

    // Step 2 should NEVER be called
    Mock::given(method("GET"))
        .and(path("/step2"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&mock_server)
        .await;

    let yaml_content = format!(
        r#"
version: "1.0"
name: "Fail-fast Routine"
steps:
  - id: "step-1"
    request:
      method: "GET"
      url: "{uri}/step1"
    assert:
      status: 200
  - id: "step-2"
    request:
      method: "GET"
      url: "{uri}/step2"
    assert:
      status: 200
"#,
        uri = mock_server.uri()
    );

    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(yaml_content.as_bytes()).unwrap();

    let runner = create_test_runner();
    let manifest = PaynalManifest::default();

    let passed = run_paynal_file(
        &runner,
        temp_file.path(),
        &manifest,
        None,
        None,
        false,
    )
    .await
    .unwrap();

    assert!(!passed, "Routine should fail overall");
    // Mock for /step2 verifies 0 requests received
}

#[tokio::test]
async fn test_routine_continue_on_failure_true_proceeds() {
    // P1-6: When continueOnFailure is true, subsequent steps execute even if previous step fails
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/step1"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/step2"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock_server)
        .await;

    let yaml_content = format!(
        r#"
version: "1.0"
name: "Continue Routine"
continueOnFailure: true
steps:
  - id: "step-1"
    request:
      method: "GET"
      url: "{uri}/step1"
    assert:
      status: 200
  - id: "step-2"
    request:
      method: "GET"
      url: "{uri}/step2"
    assert:
      status: 200
"#,
        uri = mock_server.uri()
    );

    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(yaml_content.as_bytes()).unwrap();

    let runner = create_test_runner();
    let manifest = PaynalManifest::default();

    let passed = run_paynal_file(
        &runner,
        temp_file.path(),
        &manifest,
        None,
        None,
        false,
    )
    .await
    .unwrap();

    assert!(!passed, "Routine should fail overall because step 1 failed");
    // Mock for /step2 verifies 1 request received
}

#[tokio::test]
async fn test_strict_vars_rejects_unresolved_placeholder() {
    // P1-4: With strict_vars = true, any unresolved placeholder produces an error
    let runner = create_test_runner();
    let manifest = PaynalManifest::default();

    let yaml_content = r#"
version: "1.0"
name: "Missing Var Request"
request:
  method: "GET"
  url: "https://example.com/api/${UNDEFINED_VAR_XYZ}"
assert:
  status: 200
"#;

    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(yaml_content.as_bytes()).unwrap();

    let result = run_paynal_file(
        &runner,
        temp_file.path(),
        &manifest,
        None,
        None,
        true, // strict_vars = true
    )
    .await;

    assert!(result.is_err(), "Strict vars should return Err on unresolved variable");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("UNDEFINED_VAR_XYZ"),
        "Error message should mention the missing variable"
    );
}

#[test]
fn test_vars_intra_block_dependencies_resolve_deterministically() {
    // P1-7: Variables referencing other variables defined in the same block
    // must resolve deterministically via multi-pass resolution regardless of insertion order.
    let mut ctx = VariableContext::new();

    let mut vars = HashMap::new();
    vars.insert("endpoint".to_string(), "${baseUrl}/v1/users".to_string());
    vars.insert("baseUrl".to_string(), "${protocol}://${domain}".to_string());
    vars.insert("protocol".to_string(), "https".to_string());
    vars.insert("domain".to_string(), "api.example.com".to_string());

    ctx.extend(&vars);

    assert_eq!(
        ctx.get("endpoint").map(|s| s.as_str()),
        Some("https://api.example.com/v1/users"),
        "Multi-pass fixed-point resolution should resolve transitive dependencies"
    );
}
