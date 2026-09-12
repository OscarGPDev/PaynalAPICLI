use paynal::commands::exec::{execute_exec_with_options, ExecOptions};
use std::io::Write;
use tempfile::NamedTempFile;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_cli_var_override_takes_precedence_over_file_vars() {
    let mock_server = MockServer::start().await;

    // Expect the path overridden by CLI --var (staging instead of default_env)
    Mock::given(method("GET"))
        .and(path("/api/staging"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .expect(1)
        .mount(&mock_server)
        .await;

    let yaml_content = format!(
        r#"
version: "1.0"
name: "Var Override Test"
vars:
  envName: "default_env"
request:
  method: "GET"
  url: "{}/api/${{envName}}"
assert:
  status: 200
"#,
        mock_server.uri()
    );

    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(yaml_content.as_bytes()).unwrap();

    let options = ExecOptions {
        cli_vars: vec!["envName=staging".to_string()],
        ..Default::default()
    };

    let exit_code = execute_exec_with_options(
        temp_file.path().to_str().unwrap().to_string(),
        options,
    )
    .await
    .unwrap();

    assert_eq!(exit_code, 0, "CLI --var override should be reflected in request URL");
}
