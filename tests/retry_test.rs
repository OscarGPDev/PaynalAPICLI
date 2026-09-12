use paynal::commands::exec::{execute_exec_with_options, ExecOptions};
use std::io::Write;
use tempfile::NamedTempFile;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_retry_on_503_recovers_on_second_attempt() {
    let mock_server = MockServer::start().await;

    // First attempt gets 503
    Mock::given(method("GET"))
        .and(path("/retry-endpoint"))
        .respond_with(ResponseTemplate::new(503).set_body_string("Service Unavailable"))
        .up_to_n_times(1)
        .expect(1)
        .mount(&mock_server)
        .await;

    // Second attempt gets 200
    Mock::given(method("GET"))
        .and(path("/retry-endpoint"))
        .respond_with(ResponseTemplate::new(200).set_body_string("All good now"))
        .expect(1)
        .mount(&mock_server)
        .await;

    let yaml_content = format!(
        r#"
version: "1.0"
name: "Retry Test"
request:
  method: "GET"
  url: "{}/retry-endpoint"
  retry:
    attempts: 3
    backoffMs: 20
    on: [503]
assert:
  status: 200
"#,
        mock_server.uri()
    );

    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(yaml_content.as_bytes()).unwrap();

    let options = ExecOptions::default();

    let exit_code = execute_exec_with_options(
        temp_file.path().to_str().unwrap().to_string(),
        options,
    )
    .await
    .unwrap();

    assert_eq!(exit_code, 0, "Execution should pass after retry");
    // Wiremock will verify both expectations (1 failure + 1 success = 2 requests) on mock_server drop
}

#[tokio::test]
async fn test_retry_exhausted_returns_failure() {
    let mock_server = MockServer::start().await;

    // Always returns 502 Bad Gateway
    Mock::given(method("GET"))
        .and(path("/always-fails"))
        .respond_with(ResponseTemplate::new(502).set_body_string("Bad Gateway"))
        .expect(3) // 3 attempts made
        .mount(&mock_server)
        .await;

    let yaml_content = format!(
        r#"
version: "1.0"
name: "Exhausted Retry Test"
request:
  method: "GET"
  url: "{}/always-fails"
  retry:
    attempts: 3
    backoffMs: 10
    on: [502]
assert:
  status: 200
"#,
        mock_server.uri()
    );

    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(yaml_content.as_bytes()).unwrap();

    let options = ExecOptions::default();

    let exit_code = execute_exec_with_options(
        temp_file.path().to_str().unwrap().to_string(),
        options,
    )
    .await
    .unwrap();

    assert_eq!(exit_code, 1, "Execution should fail when retries are exhausted and assertions fail");
}
