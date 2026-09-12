use paynal::commands::exec::{execute_exec_with_options, ExecOptions};
use paynal::reporters::{FileReport, ReporterType, StepReport, TestReport};
use std::fs;
use std::io::Write;
use tempfile::NamedTempFile;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[test]
fn test_reporters_unit_formatting() {
    let mut report = TestReport {
        total_duration_ms: 125,
        files: vec![
            FileReport {
                name: "AuthSuite".to_string(),
                path: "collections/auth/login.yaml".to_string(),
                is_routine: true,
                duration_ms: 100,
                passed: true,
                error: None,
                steps: vec![
                    StepReport {
                        id: "step-1".to_string(),
                        name: "Login Step".to_string(),
                        method: "POST".to_string(),
                        url: "http://api.example.com/login".to_string(),
                        status: 200,
                        duration_ms: 50,
                        passed: true,
                        skipped: false,
                        asserts: vec!["Status == 200".to_string()],
                        failure_messages: vec![],
                        error: None,
                    },
                    StepReport {
                        id: "step-2".to_string(),
                        name: "Profile Step".to_string(),
                        method: "GET".to_string(),
                        url: "http://api.example.com/profile".to_string(),
                        status: 200,
                        duration_ms: 50,
                        passed: true,
                        skipped: false,
                        asserts: vec!["Status == 200".to_string()],
                        failure_messages: vec![],
                        error: None,
                    },
                ],
            },
            FileReport {
                name: "OrdersSuite".to_string(),
                path: "collections/orders/create.yaml".to_string(),
                is_routine: false,
                duration_ms: 25,
                passed: false,
                error: None,
                steps: vec![StepReport {
                    id: "order-step".to_string(),
                    name: "Create Order".to_string(),
                    method: "POST".to_string(),
                    url: "http://api.example.com/orders".to_string(),
                    status: 400,
                    duration_ms: 25,
                    passed: false,
                    skipped: false,
                    asserts: vec!["Status expected 201 but got 400".to_string()],
                    failure_messages: vec!["Status expected 201 but got 400".to_string()],
                    error: None,
                }],
            },
        ],
        ..Default::default()
    };

    report.compute_summary();

    // 1. Verify compute summary
    assert_eq!(report.total_files, 2);
    assert_eq!(report.passed_files, 1);
    assert_eq!(report.failed_files, 1);
    assert_eq!(report.total_steps, 3);
    assert_eq!(report.passed_steps, 2);
    assert_eq!(report.failed_steps, 1);

    // 2. Human output contains summary
    let human = report.to_human_summary();
    assert!(human.contains("Execution Summary (CI Aggregated)"));
    assert!(human.contains("Files: 2 total"));
    assert!(human.contains("Steps: 3 total"));

    // 3. JSON output valid and parseable
    let json_str = report.to_json().expect("JSON serialization must succeed");
    let json_val: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    assert_eq!(json_val["total_files"], 2);
    assert_eq!(json_val["total_steps"], 3);
    assert_eq!(json_val["files"][0]["name"], "AuthSuite");

    // 4. JUnit XML output valid schema
    let junit_xml = report.to_junit_xml();
    assert!(junit_xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
    assert!(junit_xml.contains("<testsuites name=\"Paynal\" tests=\"3\" failures=\"1\""));
    assert!(junit_xml.contains("<testsuite name=\"AuthSuite\" tests=\"2\" failures=\"0\""));
    assert!(junit_xml.contains("<testsuite name=\"OrdersSuite\" tests=\"1\" failures=\"1\""));
    assert!(junit_xml.contains("<failure message=\"Status expected 201 but got 400\""));
}

#[tokio::test]
async fn test_execute_exec_with_junit_reporter_and_out_file() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/test"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&mock_server)
        .await;

    let yaml_content = format!(
        r#"
version: "1.0"
name: "JUnit CI Test"
request:
  method: "GET"
  url: "{}/api/test"
assert:
  status: 200
"#,
        mock_server.uri()
    );

    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(yaml_content.as_bytes()).unwrap();

    let out_report_file = NamedTempFile::new().unwrap();
    let out_report_path = out_report_file.path().to_str().unwrap().to_string();

    let options = ExecOptions {
        reporter: ReporterType::Junit,
        report_out: Some(out_report_path.clone()),
        ..Default::default()
    };

    let exit_code = execute_exec_with_options(
        temp_file.path().to_str().unwrap().to_string(),
        options,
    )
    .await
    .unwrap();

    assert_eq!(exit_code, 0, "Execution should pass");

    let xml_saved = fs::read_to_string(&out_report_path).unwrap();
    assert!(xml_saved.contains("<testsuites name=\"Paynal\" tests=\"1\" failures=\"0\""));
    assert!(xml_saved.contains("<testsuite name=\"JUnit CI Test\""));
}

#[tokio::test]
async fn test_execute_exec_with_json_reporter_and_out_file() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/json-test"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&mock_server)
        .await;

    let yaml_content = format!(
        r#"
version: "1.0"
name: "JSON CI Test"
request:
  method: "GET"
  url: "{}/api/json-test"
assert:
  status: 200
"#,
        mock_server.uri()
    );

    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(yaml_content.as_bytes()).unwrap();

    let out_report_file = NamedTempFile::new().unwrap();
    let out_report_path = out_report_file.path().to_str().unwrap().to_string();

    let options = ExecOptions {
        reporter: ReporterType::Json,
        report_out: Some(out_report_path.clone()),
        ..Default::default()
    };

    let exit_code = execute_exec_with_options(
        temp_file.path().to_str().unwrap().to_string(),
        options,
    )
    .await
    .unwrap();

    assert_eq!(exit_code, 0);

    let json_saved = fs::read_to_string(&out_report_path).unwrap();
    let val: serde_json::Value = serde_json::from_str(&json_saved).unwrap();
    assert_eq!(val["passed_files"], 1);
    assert_eq!(val["failed_files"], 0);
    assert_eq!(val["files"][0]["name"], "JSON CI Test");
}
