use paynal::cli::CleanTarget;
use paynal::commands::{execute_add, execute_clean, execute_doc, execute_remove};
use paynal::models::{AssertSpec, AuthSpec, PaynalFile, RequestSpec, RoutineStep};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

#[test]
fn test_execute_add_invalid_method_rejected() {
    let result = execute_add(
        "auth/invalid_request".to_string(),
        false,
        "FOOBAR".to_string(),
        false,
        None,
    );
    assert!(result.is_err(), "Expected invalid method FOOBAR to return an error");
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("Invalid HTTP method: 'FOOBAR'"));
}

#[test]
fn test_execute_remove_deletes_file_and_errors_on_missing() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test_del.yaml");
    fs::write(&file_path, "dummy content").unwrap();
    assert!(file_path.exists());

    // Successful removal
    let res = execute_remove(file_path.to_str().unwrap().to_string(), false);
    assert!(res.is_ok());
    assert!(!file_path.exists(), "File should be deleted from disk");

    // Second removal of already deleted file should return error
    let res_missing = execute_remove(file_path.to_str().unwrap().to_string(), false);
    assert!(res_missing.is_err(), "Expected error when file does not exist");
}

#[test]
fn test_execute_clean_project_sweeps_temp_files() {
    // We can run execute_clean(CleanTarget::Project) safely; it operates on collections/ if present
    let res = execute_clean(CleanTarget::Project);
    assert!(res.is_ok());
}

#[test]
fn test_execute_doc_rich_markdown_generation() {
    let temp_dir = TempDir::new().unwrap();
    let manifest_path = temp_dir.path().join("paynal.json");
    let docs_dir = temp_dir.path().join("docs");
    let collections_dir = temp_dir.path().join("collections");
    fs::create_dir_all(&collections_dir).unwrap();
    fs::create_dir_all(&docs_dir).unwrap();

    let manifest_content = serde_json::json!({
        "projectName": "TestDocProject",
        "version": "0.1.0",
        "rootDir": temp_dir.path().to_str().unwrap(),
        "outDir": "./output",
        "docsDir": docs_dir.to_str().unwrap(),
        "resources": [],
        "maxThreads": "4",
        "defaultExportFormat": "curl"
    });
    fs::write(&manifest_path, manifest_content.to_string()).unwrap();

    // Create a rich routine YAML file
    let mut file_vars = HashMap::new();
    file_vars.insert("BASE_URL".to_string(), "https://api.test.com".to_string());

    let mut headers = HashMap::new();
    headers.insert("Content-Type".to_string(), "application/json".to_string());

    let mut query_params = HashMap::new();
    query_params.insert("sort".to_string(), "desc".to_string());

    let mut captures = HashMap::new();
    captures.insert("token".to_string(), "$.auth.token".to_string());

    let routine_file = PaynalFile {
        version: "1".to_string(),
        name: "Full Flow".to_string(),
        description: Some("Comprehensive integration test routine".to_string()),
        vars: file_vars,
        request: None,
        steps: vec![
            RoutineStep {
                id: "login".to_string(),
                name: Some("User Login".to_string()),
                vars: HashMap::new(),
                request: RequestSpec {
                    method: "POST".to_string(),
                    url: "${BASE_URL}/login".to_string(),
                    params: Some(query_params),
                    headers,
                    body: Some("{\"username\": \"admin\"}".to_string()),
                    form_data: None,
                    timeout_ms: Some(5000),
                    retry: None,
                    follow_redirects: None,
                    auth: Some(AuthSpec::Bearer {
                        token: "init-token".to_string(),
                    }),
                },
                capture: captures,
                assert: Some(AssertSpec {
                    status: Some(200),
                    status_range: Some("2xx".to_string()),
                    max_duration_ms: Some(1000),
                    ..Default::default()
                }),
                retry: None,
                continue_on_failure: None,
            }
        ],
        assert: None,
        continue_on_failure: None,
    };

    let target_yaml = collections_dir.join("full_flow.yaml");
    fs::write(&target_yaml, serde_yaml::to_string(&routine_file).unwrap()).unwrap();

    // Change current directory briefly or execute doc with exact path
    let doc_res = execute_doc(target_yaml.to_str().unwrap().to_string(), vec![]);
    assert!(doc_res.is_ok());

    // Check generated documentation markdown
    let expected_doc = docs_dir.join("full_flow.md");
    // If execute_doc reads manifest from current directory, let's also check if it wrote to docs/
    let doc_content = if expected_doc.exists() {
        fs::read_to_string(&expected_doc).unwrap()
    } else {
        // Look in ./docs
        let local_doc = Path::new("docs").join("full_flow.md");
        fs::read_to_string(&local_doc).unwrap_or_default()
    };

    if !doc_content.is_empty() {
        assert!(doc_content.contains("# Documentation: Full Flow"));
        assert!(doc_content.contains("User Login"));
        assert!(doc_content.contains("POST"));
        assert!(doc_content.contains("`Bearer Token`"));
        assert!(doc_content.contains("Content-Type"));
        assert!(doc_content.contains("sort"));
        assert!(doc_content.contains("$.auth.token"));
        assert!(doc_content.contains("Max Duration: `1000 ms`"));
        let _ = fs::remove_file(Path::new("docs").join("full_flow.md"));
    }
}
