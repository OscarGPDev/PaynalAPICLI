use paynal::cli::ExportType;
use paynal::commands::{execute_doc, execute_export};
use std::fs;
use tempfile::tempdir;

#[test]
fn test_doc_and_export_directory_support() {
    let dir = tempdir().unwrap();
    let subfolder = dir.path().join("sub");
    fs::create_dir_all(&subfolder).unwrap();

    let yaml_content = r#"
version: "1.0"
name: "Test Doc"
request:
  method: "POST"
  url: "https://example.com/api"
  headers:
    Content-Type: "application/json"
  body: '{"foo": "bar"}'
"#;

    let file_path = subfolder.join("req1.yaml");
    fs::write(&file_path, yaml_content).unwrap();

    // 1. Test doc on directory
    let doc_res = execute_doc(subfolder.to_str().unwrap().to_string(), vec![]);
    assert!(doc_res.is_ok(), "execute_doc on a directory should succeed without errors");

    // 2. Test export on directory (curl, postman, insomnia)
    let export_curl_res = execute_export(subfolder.to_str().unwrap().to_string(), ExportType::Curl);
    assert!(export_curl_res.is_ok(), "execute_export Curl on a directory should succeed");

    let export_postman_res = execute_export(subfolder.to_str().unwrap().to_string(), ExportType::Postman);
    assert!(export_postman_res.is_ok(), "execute_export Postman on a directory should succeed");

    let export_insomnia_res = execute_export(subfolder.to_str().unwrap().to_string(), ExportType::Insomnia);
    assert!(export_insomnia_res.is_ok(), "execute_export Insomnia on a directory should succeed");

    // Verify generated curl.sh has escaped single quotes if any
    let curl_file = subfolder.join("req1.curl.sh");
    assert!(curl_file.exists(), "curl.sh script should be generated");
    let curl_content = fs::read_to_string(&curl_file).unwrap();
    assert!(curl_content.contains("curl -X POST"));
}
