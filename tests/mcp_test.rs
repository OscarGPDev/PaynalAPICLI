use paynal::commands::mcp::process_mcp_request;
use serde_json::json;

#[tokio::test]
async fn test_mcp_ignores_notifications() {
    // Notification has no "id" member
    let notification = json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });

    let resp = process_mcp_request(&notification).await;
    assert!(resp.is_none(), "MCP server must not reply to notifications per JSON-RPC 2.0");

    let notification_with_null_id = json!({
        "jsonrpc": "2.0",
        "method": "notifications/cancelled",
        "id": null
    });

    let resp_null = process_mcp_request(&notification_with_null_id).await;
    assert!(resp_null.is_none(), "MCP server must not reply to notifications with null id");
}

#[tokio::test]
async fn test_mcp_tools_list_contains_execute_routine() {
    let req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list"
    });

    let resp = process_mcp_request(&req).await.expect("tools/list must return response");
    assert_eq!(resp["id"], 1);

    let tools = resp["result"]["tools"].as_array().expect("tools must be an array");
    let tool_names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();

    assert!(tool_names.contains(&"list_routines"));
    assert!(tool_names.contains(&"execute_routine"));
    assert!(tool_names.contains(&"create_routine"));
}

#[tokio::test]
async fn test_mcp_execute_routine_validation_and_execution() {
    // 1. Missing path argument
    let empty_path_req = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "execute_routine",
            "arguments": {
                "path": ""
            }
        }
    });

    let empty_resp = process_mcp_request(&empty_path_req).await.unwrap();
    assert_eq!(empty_resp["error"]["code"], -32602);

    // 2. Non-existent path
    let not_found_req = json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "execute_routine",
            "arguments": {
                "path": "non_existent/routine_xyz"
            }
        }
    });

    let not_found_resp = process_mcp_request(&not_found_req).await.unwrap();
    assert_eq!(not_found_resp["error"]["code"], -32602);
}

#[tokio::test]
async fn test_mcp_unknown_method_returns_32601() {
    let unknown_req = json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "unknown/method"
    });

    let resp = process_mcp_request(&unknown_req).await.unwrap();
    assert_eq!(resp["error"]["code"], -32601);
}
