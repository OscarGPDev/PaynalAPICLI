use paynal::commands::add::execute_add;
use paynal::commands::exec::{run_paynal_file_ext, RunFileOptions};
use paynal::commands::import_cmd::execute_import;
use paynal::commands::remove::execute_remove;
use paynal::manifest::PaynalManifest;
use paynal::reporters::{FileReport, ReporterType, StepReport, TestReport, write_report};
use paynal::runner::HttpRunner;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[test]
fn test_finding_3_and_10_utf8_secret_masking_and_sensitive_headers() {
    // Finding #10: Header Detection
    assert!(HttpRunner::is_sensitive_header("api-key"));
    assert!(HttpRunner::is_sensitive_header("apikey"));
    assert!(HttpRunner::is_sensitive_header("X-Api-Key"));
    assert!(HttpRunner::is_sensitive_header("Proxy-Authorization"));
    assert!(HttpRunner::is_sensitive_header("Authorization"));
    assert!(HttpRunner::is_sensitive_header("Cookie"));
    assert!(HttpRunner::is_sensitive_header("Set-Cookie"));
    assert!(HttpRunner::is_sensitive_header("x-auth-token"));
    assert!(HttpRunner::is_sensitive_header("my-secret-key"));
    assert!(HttpRunner::is_sensitive_header("User-Password"));

    // Insensitive headers
    assert!(!HttpRunner::is_sensitive_header("Content-Type"));
    assert!(!HttpRunner::is_sensitive_header("Accept"));
    assert!(!HttpRunner::is_sensitive_header("User-Agent"));

    // Finding #3: UTF-8 & Emoji Secret Masking (No panics on character slicing)
    // Multibyte accented characters (9 characters > 8: prefix 4 chars + "...***")
    let accented_long = "ééééééééé";
    assert_eq!(accented_long.chars().count(), 9);
    let masked_accented = HttpRunner::mask_sensitive_value("Authorization", accented_long);
    assert_eq!(masked_accented, "éééé...***");

    // Emoji secret (9 emojis > 8: prefix 4 emojis + "...***")
    let emoji_secret = "🔑🔑🔑🔑🔑🔑🔑🔑🔑";
    assert_eq!(emoji_secret.chars().count(), 9);
    let masked_emoji = HttpRunner::mask_sensitive_value("Authorization", emoji_secret);
    assert_eq!(masked_emoji, "🔑🔑🔑🔑...***");

    // Short multibyte secret (3 characters <= 8: masked completely as "***")
    let short_multibyte = "ñññ";
    assert_eq!(short_multibyte.chars().count(), 3);
    assert_eq!(HttpRunner::mask_sensitive_value("Authorization", short_multibyte), "***");

    // Exact boundary check: 8 characters -> "***", 9 characters -> prefix 4 + "...***"
    let eight_chars = "12345678";
    assert_eq!(HttpRunner::mask_sensitive_value("api-key", eight_chars), "***");
    let nine_chars = "123456789";
    assert_eq!(HttpRunner::mask_sensitive_value("api-key", nine_chars), "1234...***");

    // Non-sensitive header returns original value untouched
    assert_eq!(
        HttpRunner::mask_sensitive_value("Content-Type", "application/json; charset=utf-8"),
        "application/json; charset=utf-8"
    );
}

#[test]
fn test_finding_5_and_6_path_traversal_and_arbitrary_file_deletion_prevention() {
    let temp = TempDir::new().unwrap();
    let outside_file = temp.path().join("canary_outside.txt");
    fs::write(&outside_file, "CANARY_DATA_DO_NOT_DELETE").unwrap();
    assert!(outside_file.exists());

    // 1. commands::remove::execute_remove with path traversal ("../outside") must fail
    let res_traversal = execute_remove("../outside".to_string(), true);
    assert!(res_traversal.is_err(), "execute_remove('../outside') should return Err");
    let err_str = res_traversal.unwrap_err().to_string();
    assert!(err_str.contains("path traversal") || err_str.contains("not allowed"));

    // 2. commands::remove::execute_remove with absolute path ("/etc/passwd") must fail
    let res_absolute = execute_remove("/etc/passwd".to_string(), true);
    assert!(res_absolute.is_err(), "execute_remove('/etc/passwd') should return Err");

    // 3. Verify files outside collections/ were NOT deleted
    assert!(
        outside_file.exists(),
        "Canary file outside collections/ must remain untouched"
    );
    assert_eq!(
        fs::read_to_string(&outside_file).unwrap(),
        "CANARY_DATA_DO_NOT_DELETE"
    );

    // 4. commands::add::execute_add with path traversal must fail
    let res_add_traversal = execute_add("../escape".to_string(), false, "GET".to_string(), false, None);
    assert!(res_add_traversal.is_err(), "execute_add('../escape') should return Err");

    let res_add_abs = execute_add("/etc/shadow".to_string(), false, "GET".to_string(), false, None);
    assert!(res_add_abs.is_err(), "execute_add('/etc/shadow') should return Err");

    // 5. commands::import_cmd::execute_import with --into ../escape must fail
    let res_import = execute_import("dummy_input.json".to_string(), "../escape".to_string());
    assert!(res_import.is_err(), "execute_import with out_dir '../escape' should return Err");
    let import_err = res_import.unwrap_err().to_string();
    assert!(import_err.contains("path traversal") || import_err.contains("not allowed"));
}

#[tokio::test]
async fn test_finding_1_2_7_silent_and_workspace_scoped_execution_without_set_current_dir() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/status"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{\"service\":\"up\"}"))
        .mount(&mock_server)
        .await;

    let temp = TempDir::new().unwrap();
    let ws_dir = temp.path().to_path_buf();

    // 1. Create workspace paynal.env with custom workspace variables
    let mock_port = mock_server.address().port();
    fs::write(
        ws_dir.join("paynal.env"),
        format!("SERVICE_PORT={}\nENV_NAME=qa_automated\n", mock_port),
    )
    .unwrap();

    // 2. Create collection YAML utilizing interpolated workspace variable
    let collections_dir = ws_dir.join("collections");
    fs::create_dir_all(&collections_dir).unwrap();
    let test_yaml = collections_dir.join("status.yaml");
    fs::write(
        &test_yaml,
        r#"
version: "1.0"
name: "Scoped Status Check"
request:
  method: "GET"
  url: "http://127.0.0.1:${SERVICE_PORT}/api/status"
assert:
  status: 200
  contains: "up"
"#,
    )
    .unwrap();

    let runner = HttpRunner::default();
    let manifest = PaynalManifest::default();

    let opts = RunFileOptions {
        export_override: None,
        env_profile: None,
        strict_vars: true,
        dry_run: false,
        verbose: false,
        cli_vars: HashMap::new(),
        silent: true,
        workspace_dir: Some(ws_dir.clone()),
    };

    // Record process current directory before execution
    let cwd_before = std::env::current_dir().unwrap();

    // Execute scoped run
    let report = run_paynal_file_ext(&runner, &test_yaml, &manifest, &opts)
        .await
        .expect("run_paynal_file_ext should succeed");

    // Record process current directory after execution
    let cwd_after = std::env::current_dir().unwrap();

    // Verification 1: Process working directory must not be mutated
    assert_eq!(
        cwd_before, cwd_after,
        "Current working directory must not change during workspace-scoped execution"
    );

    // Verification 2: The execution report passed and resolved workspace variables
    assert!(report.passed, "Execution report should pass");
    assert_eq!(report.steps.len(), 1);
    assert_eq!(report.steps[0].status, 200);
}

#[test]
fn test_finding_12_junit_xml_ansi_strip_and_control_character_handling() {
    let mut report = TestReport::default();

    let step = StepReport {
        id: "step_1".to_string(),
        name: "Login Attempt \x1b[33mStep\x1b[0m".to_string(),
        method: "POST".to_string(),
        url: "https://api.test.com/login".to_string(),
        status: 500,
        duration_ms: 120,
        passed: false,
        skipped: false,
        asserts: vec!["status == 200".to_string()],
        failure_messages: vec![
            "\x1b[31mFAIL\x1b[0m: status was 500 & body contained <error>\x07 (bell)".to_string(),
        ],
        error: Some("Internal \x1b[1;31mFatal\x1b[0m \x0b error & crash".to_string()),
    };

    let file = FileReport {
        name: "Auth \x1b[36mSuite\x1b[0m".to_string(),
        path: "collections/auth.yaml".to_string(),
        is_routine: false,
        duration_ms: 120,
        passed: false,
        error: Some("Init failure: \x1b[31mCRITICAL\x1b[0m \x00 NUL & <invalid>".to_string()),
        steps: vec![step],
    };

    report.files.push(file);
    report.compute_summary();

    // Generate JUnit report string via write_report
    let xml_output = write_report(&report, ReporterType::Junit, None::<&Path>)
        .expect("write_report should produce valid XML string");

    // 1. Verify ANSI escape codes are stripped
    assert!(
        !xml_output.contains("\x1b[31m"),
        "ANSI escape sequences must be stripped from JUnit XML"
    );
    assert!(
        !xml_output.contains("\x1b[0m"),
        "ANSI reset escape codes must be stripped from JUnit XML"
    );
    assert!(
        !xml_output.contains("[31m"),
        "ANSI bracket codes must be stripped from JUnit XML"
    );
    assert!(
        !xml_output.contains("[0m"),
        "ANSI bracket codes must be stripped from JUnit XML"
    );

    // 2. Verify control characters are stripped
    assert!(
        !xml_output.contains('\x07'),
        "Control character \x07 (bell) must be stripped"
    );
    assert!(
        !xml_output.contains('\x00'),
        "Control character \x00 (NUL) must be stripped"
    );
    assert!(
        !xml_output.contains('\x0b'),
        "Control character \x0b (vertical tab) must be stripped"
    );

    // 3. Verify clean text and valid XML escaping are preserved
    assert!(xml_output.contains("FAIL: status was 500 &amp; body contained &lt;error&gt; (bell)"));
    assert!(xml_output.contains("Init failure: CRITICAL  NUL &amp; &lt;invalid&gt;"));
    assert!(xml_output.contains("<testsuites name=\"Paynal\""));
    assert!(xml_output.contains("</testsuites>"));
}

#[tokio::test]
async fn test_pass2_global_execute_show_and_resolve_cross_workspace_path_traversal() {
    use paynal::commands::global::{execute_add as execute_global_add, execute_show, resolve_cross_workspace_path};

    let temp = TempDir::new().unwrap();
    let reg_path = temp.path().join("registry.json");
    std::env::set_var("PAYNAL_GLOBAL_REGISTRY_PATH", reg_path.to_str().unwrap());

    // 1. Setup workspace with a secret file at the workspace root
    let ws_dir = temp.path().join("secure_ws");
    let coll_dir = ws_dir.join("collections");
    fs::create_dir_all(&coll_dir).unwrap();
    fs::write(
        ws_dir.join("paynal.json"),
        r#"{"projectName": "secure_ws", "version": "1.0", "rootDir": ".", "outDir": "out"}"#,
    ).unwrap();
    fs::write(
        ws_dir.join("paynal.env"),
        "SUPER_SECRET_TOKEN=xyz_secret_999\n",
    ).unwrap();

    // Create a legitimate collection file
    fs::write(
        coll_dir.join("ping.yaml"),
        "version: \"1.0\"\nname: Ping\nrequest:\n  method: GET\n  url: https://example.com/ping\n",
    ).unwrap();

    // Register workspace
    execute_global_add(
        Some("secure_ws".to_string()),
        Some(ws_dir.to_str().unwrap().to_string()),
        "Secure Workspace".to_string(),
    ).unwrap();

    // 2. Test resolve_cross_workspace_path traversal protection
    // Target attempting parent directory traversal (@workspace/../../paynal.env)
    let res_resolve_traversal = resolve_cross_workspace_path("@secure_ws/../../paynal.env");
    assert!(res_resolve_traversal.is_err(), "resolve_cross_workspace_path should reject ../ traversal");
    let err_msg = res_resolve_traversal.unwrap_err().to_string();
    assert!(err_msg.contains("path traversal") || err_msg.contains("not allowed"));

    // Target attempting absolute subpath (@workspace//etc/passwd)
    let res_resolve_abs = resolve_cross_workspace_path("@secure_ws//etc/passwd");
    assert!(res_resolve_abs.is_err(), "resolve_cross_workspace_path should reject absolute subpaths");

    // Valid cross-workspace target should succeed
    let res_resolve_valid = resolve_cross_workspace_path("@secure_ws/ping");
    assert!(res_resolve_valid.is_ok(), "resolve_cross_workspace_path should accept valid route");
    let (_entry, sub) = res_resolve_valid.unwrap();
    assert_eq!(sub, "ping");

    // 3. Test execute_show traversal protection
    // Traversing to paynal.env outside collections/
    let res_show_traversal = execute_show("@secure_ws/../../paynal.env".to_string());
    assert!(res_show_traversal.is_err(), "execute_show should reject traversal");
    let show_err = res_show_traversal.unwrap_err().to_string();
    assert!(show_err.contains("path traversal") || show_err.contains("not allowed"));

    // Traversing to absolute path
    let res_show_abs = execute_show("@secure_ws//etc/passwd".to_string());
    assert!(res_show_abs.is_err(), "execute_show should reject absolute path");

    // Valid show should succeed
    let res_show_valid = execute_show("@secure_ws/ping".to_string());
    assert!(res_show_valid.is_ok(), "execute_show should succeed for legitimate route");
}

#[test]
fn test_pass2_importer_malicious_item_and_folder_containment() {
    let temp = TempDir::new().unwrap();
    let malicious_json = temp.path().join("malicious_postman.json");

    let postman_content = serde_json::json!({
        "info": {
            "name": "Malicious Collection",
            "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
        },
        "item": [
            {
                "name": "../../escape_payload",
                "request": {
                    "method": "GET",
                    "url": {
                        "raw": "https://api.example.com/exploit"
                    }
                }
            },
            {
                "name": "../../../evil_folder",
                "item": [
                    {
                        "name": "../../nested_escape_item",
                        "request": {
                            "method": "POST",
                            "url": {
                                "raw": "https://api.example.com/nested"
                            }
                        }
                    }
                ]
            }
        ]
    });

    fs::write(&malicious_json, postman_content.to_string()).unwrap();

    let out_dir = temp.path().join("collections");
    let canary_file = temp.path().join("escape_payload.yaml");
    assert!(!canary_file.exists());

    let res = execute_import(
        malicious_json.to_str().unwrap().to_string(),
        out_dir.to_str().unwrap().to_string(),
    );
    assert!(res.is_ok(), "execute_import should succeed by sanitizing malicious item names");

    // 1. Verify canary file outside out_dir was NOT created
    assert!(
        !canary_file.exists(),
        "Malicious item must NOT escape out_dir to create file in parent directory"
    );

    // 2. Verify all files written are strictly within out_dir
    fn assert_all_within(dir: &Path, expected_root: &Path) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let p = entry.path();
                assert!(
                    p.starts_with(expected_root),
                    "Found file {:?} outside of expected target {:?}",
                    p,
                    expected_root
                );
                if p.is_dir() {
                    assert_all_within(&p, expected_root);
                }
            }
        }
    }
    assert_all_within(&out_dir, &out_dir);

    // 3. Verify that the files were created with sanitized names inside out_dir
    assert!(out_dir.exists(), "Target out_dir should exist");
    let mut imported_files = Vec::new();
    fn collect_files(dir: &Path, acc: &mut Vec<String>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let p = entry.path();
                if p.is_file() {
                    acc.push(p.file_name().unwrap().to_string_lossy().to_string());
                } else if p.is_dir() {
                    collect_files(&p, acc);
                }
            }
        }
    }
    collect_files(&out_dir, &mut imported_files);
    assert!(!imported_files.is_empty(), "Should have imported sanitized files");
    for fname in &imported_files {
        assert!(
            !fname.contains(".."),
            "Sanitized filename must not contain '..': {}",
            fname
        );
    }
}

