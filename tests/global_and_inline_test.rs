use paynal::commands::global::{
    execute_add as execute_global_add, execute_discover, execute_list,
    execute_remove as execute_global_remove, execute_show, GlobalRegistry,
};
use paynal::commands::mcp::process_mcp_request;
use paynal::commands::req_cmd::{execute_req, ReqArgs};
use paynal::commands::{execute_exec_with_options, ExecOptions};
use serde_json::json;
use std::fs;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

static SERIAL_TEST: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test]
async fn test_global_registry_crud_and_isolation() {
    let _guard = SERIAL_TEST.lock().await;
    let temp = TempDir::new().unwrap();
    let reg_path = temp.path().join("registry.json");
    std::env::set_var("PAYNAL_GLOBAL_REGISTRY_PATH", reg_path.to_str().unwrap());

    // 1. Initial registry is empty
    let reg = GlobalRegistry::load().unwrap();
    assert!(reg.workspaces.is_empty());

    // 2. Add workspace
    let ws_dir = temp.path().join("my_service");
    fs::create_dir_all(ws_dir.join("collections")).unwrap();
    fs::write(
        ws_dir.join("paynal.json"),
        r#"{"projectName": "my_service", "version": "1.0", "rootDir": ".", "outDir": "out"}"#,
    )
    .unwrap();

    let add_res = execute_global_add(
        Some("my_service".to_string()),
        Some(ws_dir.to_str().unwrap().to_string()),
        "Primary payments service".to_string(),
    );
    assert!(add_res.is_ok());

    // 3. Verify registry content
    let reg2 = GlobalRegistry::load().unwrap();
    assert_eq!(reg2.workspaces.len(), 1);
    let entry = reg2.find_workspace("my_service").unwrap();
    assert_eq!(entry.name, "my_service");
    assert_eq!(entry.description, "Primary payments service");

    // 4. Test list command
    assert!(execute_list(false).is_ok());
    assert!(execute_list(true).is_ok());

    // 5. Remove workspace
    assert!(execute_global_remove("my_service".to_string()).is_ok());
    let reg3 = GlobalRegistry::load().unwrap();
    assert!(reg3.workspaces.is_empty());

    // 6. Removing non-existent workspace returns error
    assert!(execute_global_remove("non_existent".to_string()).is_err());
}

#[tokio::test]
async fn test_global_discover_and_show() {
    let _guard = SERIAL_TEST.lock().await;
    let temp = TempDir::new().unwrap();
    let reg_path = temp.path().join("registry.json");
    std::env::set_var("PAYNAL_GLOBAL_REGISTRY_PATH", reg_path.to_str().unwrap());

    // Create workspace A with a single request
    let ws_a = temp.path().join("auth_ws");
    let coll_a = ws_a.join("collections").join("v1");
    fs::create_dir_all(&coll_a).unwrap();
    fs::write(
        coll_a.join("login.yaml"),
        r#"
version: "1.0"
name: "Login User"
description: "Authenticates user with credentials"
request:
  method: "POST"
  url: "https://api.example.com/v1/login"
"#,
    )
    .unwrap();

    // Create workspace B with a routine
    let ws_b = temp.path().join("billing_ws");
    let coll_b = ws_b.join("collections");
    fs::create_dir_all(&coll_b).unwrap();
    fs::write(
        coll_b.join("checkout.yaml"),
        r#"
version: "1.0"
name: "Checkout Routine"
description: "Processes customer checkout"
steps:
  - id: "step1"
    name: "Validate Cart"
    request:
      method: "GET"
      url: "https://api.example.com/cart"
  - id: "step2"
    name: "Charge Card"
    request:
      method: "POST"
      url: "https://api.example.com/charge"
"#,
    )
    .unwrap();

    // Register both
    execute_global_add(
        Some("auth".to_string()),
        Some(ws_a.to_str().unwrap().to_string()),
        "Authentication module".to_string(),
    )
    .unwrap();

    execute_global_add(
        Some("billing".to_string()),
        Some(ws_b.to_str().unwrap().to_string()),
        "Billing module".to_string(),
    )
    .unwrap();

    // Test discover without query
    assert!(execute_discover(None, false).is_ok());
    assert!(execute_discover(None, true).is_ok());

    // Test discover with query filtering
    assert!(execute_discover(Some("login".to_string()), false).is_ok());
    assert!(execute_discover(Some("billing".to_string()), true).is_ok());

    // Test show command with @workspace/path
    assert!(execute_show("@auth/v1/login".to_string()).is_ok());
    assert!(execute_show("billing/checkout".to_string()).is_ok());

    // Test show with non-existent route
    assert!(execute_show("@auth/v1/not_found".to_string()).is_err());
}

#[tokio::test]
async fn test_inline_request_execution_with_wiremock() {
    let _guard = SERIAL_TEST.lock().await;
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/status"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "status": "healthy",
                    "version": "1.0.4",
                    "uptime": 3600
                }))
                .insert_header("x-custom-test", "paynal-test-val"),
        )
        .mount(&mock_server)
        .await;

    // 1. Successful inline execution with assertions
    let target_url = format!("{}/api/status", mock_server.uri());
    let args = ReqArgs {
        method_or_url: target_url.clone(),
        url: None,
        headers: vec!["Accept: application/json".to_string()],
        body: None,
        params: vec!["foo=bar".to_string()],
        bearer: None,
        assert_status: Some(200),
        assert_json: vec![
            "$.status=healthy".to_string(),
            "$.version=1.0.4".to_string(),
        ],
        sections: None,
        env: None,
        vars: Vec::new(),
        timeout: Some(5000),
        insecure: false,
        dry_run: false,
        verbose: true,
        save: None,
    };

    let exit_code = execute_req(args).await.unwrap();
    assert_eq!(exit_code, 0, "Expected inline request to pass assertions");

    // 2. Failed status assertion returns exit code 1
    let fail_args = ReqArgs {
        method_or_url: "GET".to_string(),
        url: Some(target_url.clone()),
        headers: Vec::new(),
        body: None,
        params: Vec::new(),
        bearer: None,
        assert_status: Some(404), // will fail, server returns 200
        assert_json: Vec::new(),
        sections: None,
        env: None,
        vars: Vec::new(),
        timeout: Some(5000),
        insecure: false,
        dry_run: false,
        verbose: false,
        save: None,
    };

    let fail_exit = execute_req(fail_args).await.unwrap();
    assert_eq!(fail_exit, 1, "Expected exit code 1 on failed assertion");

    // 3. Dry run mode sends zero network requests and exits 0
    let dry_run_args = ReqArgs {
        method_or_url: "POST".to_string(),
        url: Some("http://127.0.0.1:9999/unreachable".to_string()),
        headers: vec!["Content-Type: application/json".to_string()],
        body: Some(r#"{"msg":"hello"}"#.to_string()),
        params: Vec::new(),
        bearer: Some("secret-token".to_string()),
        assert_status: None,
        assert_json: Vec::new(),
        sections: None,
        env: None,
        vars: Vec::new(),
        timeout: None,
        insecure: false,
        dry_run: true,
        verbose: false,
        save: None,
    };
    let dry_exit = execute_req(dry_run_args).await.unwrap();
    assert_eq!(dry_exit, 0, "Dry run must succeed without network request");
}

#[tokio::test]
async fn test_inline_request_save_to_yaml() {
    let _guard = SERIAL_TEST.lock().await;
    let temp = TempDir::new().unwrap();
    let prev = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    let save_args = ReqArgs {
        method_or_url: "POST".to_string(),
        url: Some("https://api.example.com/items".to_string()),
        headers: vec!["Content-Type: application/json".to_string()],
        body: Some(r#"{"name": "Widget"}"#.to_string()),
        params: vec!["category=tools".to_string()],
        bearer: Some("my_token".to_string()),
        assert_status: Some(201),
        assert_json: vec!["$.success=true".to_string()],
        sections: None,
        env: None,
        vars: Vec::new(),
        timeout: Some(3000),
        insecure: false,
        dry_run: true,
        verbose: false,
        save: Some("items/create".to_string()),
    };

    let exit_code = execute_req(save_args).await.unwrap();
    assert_eq!(exit_code, 0);

    let saved_file = temp.path().join("collections").join("items").join("create.yaml");
    assert!(saved_file.exists(), "Expected collections/items/create.yaml to be created");

    let content = fs::read_to_string(&saved_file).unwrap();
    assert!(content.contains("https://api.example.com/items"));
    assert!(content.contains("category: tools"));

    std::env::set_current_dir(prev).unwrap();
}

#[tokio::test]
async fn test_cross_workspace_exec_and_inline_url_detection() {
    let _guard = SERIAL_TEST.lock().await;
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/ping"))
        .respond_with(ResponseTemplate::new(200).set_body_string("pong"))
        .mount(&mock_server)
        .await;

    let temp = TempDir::new().unwrap();
    let reg_path = temp.path().join("registry.json");
    std::env::set_var("PAYNAL_GLOBAL_REGISTRY_PATH", reg_path.to_str().unwrap());

    // 1. Test inline URL delegation via execute_exec_with_options
    let ping_url = format!("{}/ping", mock_server.uri());
    let opts = ExecOptions::default();
    let inline_exit = execute_exec_with_options(ping_url.clone(), opts).await.unwrap();
    assert_eq!(inline_exit, 0, "Inline URL via exec should succeed");

    // 2. Setup a workspace with a YAML request pointing to wiremock
    let ws_dir = temp.path().join("remote_service");
    let coll_dir = ws_dir.join("collections");
    fs::create_dir_all(&coll_dir).unwrap();
    fs::write(
        ws_dir.join("paynal.json"),
        r#"{"projectName": "remote_service", "version": "1.0", "rootDir": ".", "outDir": "out"}"#,
    )
    .unwrap();

    let yaml_content = format!(
        r#"
version: "1.0"
name: "Ping WireMock"
request:
  method: "GET"
  url: "{}"
assert:
  status: 200
  contains: "pong"
"#,
        ping_url
    );
    fs::write(coll_dir.join("ping.yaml"), yaml_content).unwrap();

    // Register workspace as @remote
    execute_global_add(
        Some("remote".to_string()),
        Some(ws_dir.to_str().unwrap().to_string()),
        "Remote test workspace".to_string(),
    )
    .unwrap();

    // 3. Execute via @remote/ping from an arbitrary directory
    let outside_dir = temp.path().join("outside");
    fs::create_dir_all(&outside_dir).unwrap();
    let prev = std::env::current_dir().unwrap();
    std::env::set_current_dir(&outside_dir).unwrap();

    let exec_opts = ExecOptions::default();
    let exit_code = execute_exec_with_options("@remote/ping".to_string(), exec_opts)
        .await
        .unwrap();
    assert_eq!(exit_code, 0, "Expected cross-workspace execution of @remote/ping to succeed");

    std::env::set_current_dir(prev).unwrap();
}

#[tokio::test]
async fn test_mcp_global_tools_and_cross_workspace_execution() {
    let _guard = SERIAL_TEST.lock().await;
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/mcp-echo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "result": "ok" })))
        .mount(&mock_server)
        .await;

    let temp = TempDir::new().unwrap();
    let reg_path = temp.path().join("registry.json");
    std::env::set_var("PAYNAL_GLOBAL_REGISTRY_PATH", reg_path.to_str().unwrap());

    // Register a sample workspace
    let ws_dir = temp.path().join("mcp_ws");
    let coll_dir = ws_dir.join("collections");
    fs::create_dir_all(&coll_dir).unwrap();
    fs::write(
        ws_dir.join("paynal.json"),
        r#"{"projectName": "mcp_ws", "version": "1.0", "rootDir": ".", "outDir": "out"}"#,
    )
    .unwrap();
    fs::write(
        coll_dir.join("test_route.yaml"),
        format!(
            r#"
version: "1.0"
name: "MCP Test Route"
request:
  method: "POST"
  url: "{}/mcp-echo"
assert:
  status: 200
"#,
            mock_server.uri()
        ),
    )
    .unwrap();

    execute_global_add(
        Some("mcp_ws".to_string()),
        Some(ws_dir.to_str().unwrap().to_string()),
        "MCP Workspace".to_string(),
    )
    .unwrap();

    // 1. MCP list_global_workspaces
    let list_req = json!({
        "jsonrpc": "2.0",
        "id": 10,
        "method": "tools/call",
        "params": {
            "name": "list_global_workspaces",
            "arguments": {}
        }
    });
    let list_resp = process_mcp_request(&list_req).await.unwrap();
    let text = list_resp["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("mcp_ws"));

    // 2. MCP discover_global_routines
    let disc_req = json!({
        "jsonrpc": "2.0",
        "id": 11,
        "method": "tools/call",
        "params": {
            "name": "discover_global_routines",
            "arguments": {
                "query": "mcp_ws"
            }
        }
    });
    let disc_resp = process_mcp_request(&disc_req).await.unwrap();
    let disc_text = disc_resp["result"]["content"][0]["text"].as_str().unwrap();
    assert!(disc_text.contains("@mcp_ws/test_route"));

    // 3. MCP execute_inline_request
    let inline_req = json!({
        "jsonrpc": "2.0",
        "id": 12,
        "method": "tools/call",
        "params": {
            "name": "execute_inline_request",
            "arguments": {
                "method": "POST",
                "url": format!("{}/mcp-echo", mock_server.uri()),
                "body": "{\"hello\":\"world\"}"
            }
        }
    });
    let inline_resp = process_mcp_request(&inline_req).await.unwrap();
    assert_eq!(inline_resp["result"]["isError"], false);
    let inline_text = inline_resp["result"]["content"][0]["text"].as_str().unwrap();
    assert!(inline_text.contains("\"status\": 200"));

    // 4. MCP execute_routine with @workspace/path
    let ws_routine_req = json!({
        "jsonrpc": "2.0",
        "id": 13,
        "method": "tools/call",
        "params": {
            "name": "execute_routine",
            "arguments": {
                "path": "@mcp_ws/test_route"
            }
        }
    });
    let ws_routine_resp = process_mcp_request(&ws_routine_req).await.unwrap();
    assert_eq!(ws_routine_resp["result"]["isError"], false);
}
