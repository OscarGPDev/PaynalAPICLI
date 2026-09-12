use paynal::commands::exec::{execute_exec_with_options, ExecOptions};
use std::io::Write;
use tempfile::NamedTempFile;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_dry_run_sends_zero_network_requests() {
    let mock_server = MockServer::start().await;

    // Zero requests should reach the mock server!
    Mock::given(method("POST"))
        .and(path("/api/users"))
        .respond_with(ResponseTemplate::new(201))
        .expect(0)
        .mount(&mock_server)
        .await;

    let yaml_content = format!(
        r#"
version: "1.0"
name: "Dry Run Test"
vars:
  endpoint: "/api/users"
  username: "testuser"
request:
  method: "POST"
  url: "{}${{endpoint}}"
  headers:
    Content-Type: "application/json"
  body: '{{"user": "${{username}}"}}'
assert:
  status: 201
"#,
        mock_server.uri()
    );

    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(yaml_content.as_bytes()).unwrap();

    let options = ExecOptions {
        dry_run: true,
        ..Default::default()
    };

    let exit_code = execute_exec_with_options(
        temp_file.path().to_str().unwrap().to_string(),
        options,
    )
    .await
    .unwrap();

    assert_eq!(exit_code, 0, "Dry run should pass cleanly");
    // MockServer verifies 0 requests received on drop
}
