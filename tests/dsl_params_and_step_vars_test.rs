use paynal::commands::exec::{execute_exec_with_options, ExecOptions};
use std::io::Write;
use tempfile::NamedTempFile;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_structured_query_params_are_url_encoded() {
    let mock_server = MockServer::start().await;

    // Expect query params to be URL encoded: page=2, filter=name eq 'john'
    Mock::given(method("GET"))
        .and(path("/api/users"))
        .and(query_param("page", "2"))
        .and(query_param("filter", "name eq 'john'"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"users": []}"#))
        .expect(1)
        .mount(&mock_server)
        .await;

    let yaml_content = format!(
        r#"
version: "1.0"
name: "Params Test"
vars:
  pageNumber: "2"
request:
  method: "GET"
  url: "{}/api/users"
  params:
    page: "${{pageNumber}}"
    filter: "name eq 'john'"
assert:
  status: 200
"#,
        mock_server.uri()
    );

    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(yaml_content.as_bytes()).unwrap();

    let exit_code = execute_exec_with_options(
        temp_file.path().to_str().unwrap().to_string(),
        ExecOptions::default(),
    )
    .await
    .unwrap();

    assert_eq!(exit_code, 0, "Request with params should match mock and pass");
}

#[tokio::test]
async fn test_step_level_vars_override_parent_vars() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/step1-override"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/step2-parent"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock_server)
        .await;

    let yaml_content = format!(
        r#"
version: "1.0"
name: "Step Vars Test"
vars:
  targetPath: "step-parent"
steps:
  - id: "step-1"
    vars:
      targetPath: "step1-override"
    request:
      method: "GET"
      url: "{uri}/${{targetPath}}"
    assert:
      status: 200
  - id: "step-2"
    request:
      method: "GET"
      url: "{uri}/step2-parent"
    assert:
      status: 200
"#,
        uri = mock_server.uri()
    );

    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(yaml_content.as_bytes()).unwrap();

    let exit_code = execute_exec_with_options(
        temp_file.path().to_str().unwrap().to_string(),
        ExecOptions::default(),
    )
    .await
    .unwrap();

    assert_eq!(exit_code, 0, "Routine with step-level vars should resolve correctly");
}
