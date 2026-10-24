use crate::commands::exec::{run_paynal_file_ext, RunFileOptions};
use crate::manifest::PaynalManifest;
use crate::runner::HttpRunner;
use anyhow::Result;
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

fn collect_yaml_files_relative(dir: &Path, base: &Path) -> Vec<String> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                files.extend(collect_yaml_files_relative(&path, base));
            } else if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if ext == "yaml" || ext == "yml" {
                        if let Ok(rel) = path.strip_prefix(base) {
                            files.push(rel.display().to_string());
                        }
                    }
                }
            }
        }
    }
    files
}

pub async fn process_mcp_request(request: &serde_json::Value) -> Option<serde_json::Value> {
    let has_id = request.get("id").is_some() && !request["id"].is_null();
    if !has_id {
        // Per JSON-RPC 2.0: Server MUST NOT reply to notifications
        return None;
    }

    let id = &request["id"];
    let method = request["method"].as_str().unwrap_or("");

    let response = match method {
        "initialize" => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "paynal-mcp-server",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        }),
        "tools/list" => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "tools": [
                    {
                        "name": "list_routines",
                        "description": "List all API requests and routines available in the Paynal workspace",
                        "inputSchema": {
                            "type": "object",
                            "properties": {}
                        }
                    },
                    {
                        "name": "execute_routine",
                        "description": "Execute an API request or routine by path (supports local path or @workspace/path)",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "path": {
                                    "type": "string",
                                    "description": "Relative path to request or routine (e.g. auth/login or @workspace/auth/login)"
                                }
                            },
                            "required": ["path"]
                        }
                    },
                    {
                        "name": "create_routine",
                        "description": "Create a new API request or multi-step routine YAML file in the workspace",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "path": {
                                    "type": "string",
                                    "description": "Relative path to store the request or routine (e.g. auth/refresh_token)"
                                },
                                "content": {
                                    "type": "string",
                                    "description": "YAML specification content for the request or routine"
                                }
                            },
                            "required": ["path", "content"]
                        }
                    },
                    {
                        "name": "list_global_workspaces",
                        "description": "List all registered global workspaces in the Paynal directory",
                        "inputSchema": {
                            "type": "object",
                            "properties": {}
                        }
                    },
                    {
                        "name": "discover_global_routines",
                        "description": "Discover and search API requests and routines across all registered global workspaces",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "query": {
                                    "type": "string",
                                    "description": "Optional search term to filter routes by handle, name, method, URL, or description"
                                }
                            }
                        }
                    },
                    {
                        "name": "execute_inline_request",
                        "description": "Execute an ad-hoc inline HTTP request with optional headers, body, assertions, or workspace context",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "method": {
                                    "type": "string",
                                    "description": "HTTP method (e.g. GET, POST, PUT, DELETE, PATCH). Defaults to GET"
                                },
                                "url": {
                                    "type": "string",
                                    "description": "Target HTTP/HTTPS URL"
                                },
                                "headers": {
                                    "type": "object",
                                    "description": "Optional HTTP headers key-value map",
                                    "additionalProperties": { "type": "string" }
                                },
                                "body": {
                                    "type": "string",
                                    "description": "Optional raw request body payload"
                                },
                                "workspace": {
                                    "type": "string",
                                    "description": "Optional workspace name to load environment variables and manifest settings from"
                                }
                            },
                            "required": ["url"]
                        }
                    }
                ]
            }
        }),
        "tools/call" => {
            let tool_name = request["params"]["name"].as_str().unwrap_or("");
            match tool_name {
                "list_routines" => {
                    let manifest = PaynalManifest::load_from_dir(Path::new(".")).unwrap_or_default();
                    let collections_dir = Path::new(&manifest.root_dir).join("collections");
                    let routines = if collections_dir.exists() {
                        collect_yaml_files_relative(&collections_dir, &collections_dir)
                    } else {
                        Vec::new()
                    };

                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "content": [
                                {
                                    "type": "text",
                                    "text": serde_json::to_string_pretty(&routines).unwrap_or_default()
                                }
                            ]
                        }
                    })
                }
                "execute_routine" => {
                    let path_arg = request["params"]["arguments"]["path"].as_str().unwrap_or("");
                    if path_arg.trim().is_empty() {
                        json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "error": {
                                "code": -32602,
                                "message": "Invalid params: path is required"
                            }
                        })
                    } else if path_arg.starts_with('@') {
                        let stripped = &path_arg[1..];
                        let (ws_name, subpath) = match stripped.split_once('/') {
                            Some((w, s)) => (w, s),
                            None => (stripped, ""),
                        };
                        let sub_path = Path::new(subpath);
                        if sub_path.is_absolute()
                            || sub_path.components().any(|c| {
                                matches!(
                                    c,
                                    std::path::Component::ParentDir
                                        | std::path::Component::RootDir
                                        | std::path::Component::Prefix(_)
                                )
                            })
                        {
                            json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "error": {
                                    "code": -32602,
                                    "message": "Invalid path: path traversal ('..') or absolute paths are not allowed"
                                }
                            })
                        } else {
                            let registry = crate::commands::global::GlobalRegistry::load().unwrap_or_default();
                            if let Some(entry) = registry.find_workspace(ws_name) {
                                let ws_path = PathBuf::from(&entry.path);
                                let manifest = PaynalManifest::load_from_dir(&ws_path).unwrap_or_default();
                                let collections_base = ws_path.join("collections");
                                let rel_yaml = if subpath.ends_with(".yaml") || subpath.ends_with(".yml") {
                                    subpath.to_string()
                                } else {
                                    format!("{}.yaml", subpath)
                                };
                                let target = collections_base.join(&rel_yaml);
                                if !target.exists() {
                                    json!({
                                        "jsonrpc": "2.0",
                                        "id": id,
                                        "error": {
                                            "code": -32602,
                                            "message": format!("Routine or request file not found in workspace '{}': {}", ws_name, rel_yaml)
                                        }
                                    })
                                } else {
                                    let runner = HttpRunner::new(
                                        manifest.validate_certificates,
                                        manifest.proxy.as_deref(),
                                    );
                                    let opts = RunFileOptions {
                                        export_override: None,
                                        env_profile: None,
                                        strict_vars: false,
                                        dry_run: false,
                                        verbose: false,
                                        cli_vars: HashMap::new(),
                                        silent: true,
                                        workspace_dir: Some(ws_path.clone()),
                                    };
                                    match run_paynal_file_ext(&runner, &target, &manifest, &opts).await {
                                        Ok(report) => {
                                            let all_passed = report.passed;
                                            json!({
                                                "jsonrpc": "2.0",
                                                "id": id,
                                                "result": {
                                                    "content": [
                                                        {
                                                            "type": "text",
                                                            "text": if all_passed {
                                                                format!("Successfully executed routine: {}. All assertions passed.", path_arg)
                                                            } else {
                                                                format!("Executed routine: {}. One or more assertions failed.", path_arg)
                                                            }
                                                        }
                                                    ],
                                                    "isError": !all_passed
                                                }
                                            })
                                        }
                                        Err(e) => {
                                            json!({
                                                "jsonrpc": "2.0",
                                                "id": id,
                                                "result": {
                                                    "content": [
                                                        {
                                                            "type": "text",
                                                            "text": format!("Execution failed: {}", e)
                                                        }
                                                    ],
                                                    "isError": true
                                                }
                                            })
                                        }
                                    }
                                }
                            } else {
                                json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "error": {
                                        "code": -32602,
                                        "message": format!("Global workspace '@{}' not found in registry", ws_name)
                                    }
                                })
                            }
                        }
                    } else {
                        let local_path = Path::new(path_arg);
                        if local_path.is_absolute()
                            || local_path.components().any(|c| {
                                matches!(
                                    c,
                                    std::path::Component::ParentDir
                                        | std::path::Component::RootDir
                                        | std::path::Component::Prefix(_)
                                )
                            })
                        {
                            json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "error": {
                                    "code": -32602,
                                    "message": "Invalid path: path traversal ('..') or absolute paths are not allowed"
                                }
                            })
                        } else {
                            let manifest = PaynalManifest::load_from_dir(Path::new(".")).unwrap_or_default();
                            let mut target = PathBuf::from(path_arg);
                            if !target.exists() {
                                let collections_base = Path::new(&manifest.root_dir).join("collections");
                                let rel_yaml = if path_arg.ends_with(".yaml") || path_arg.ends_with(".yml") {
                                    path_arg.to_string()
                                } else {
                                    format!("{}.yaml", path_arg)
                                };
                                target = collections_base.join(&rel_yaml);
                            }

                            if !target.exists() {
                                json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "error": {
                                        "code": -32602,
                                        "message": format!("Routine or request file not found: {}", path_arg)
                                    }
                                })
                            } else {
                            let runner = HttpRunner::new(
                                manifest.validate_certificates,
                                manifest.proxy.as_deref(),
                            );
                            let opts = RunFileOptions {
                                export_override: None,
                                env_profile: None,
                                strict_vars: false,
                                dry_run: false,
                                verbose: false,
                                cli_vars: HashMap::new(),
                                silent: true,
                                workspace_dir: None,
                            };
                            match run_paynal_file_ext(&runner, &target, &manifest, &opts).await {
                                Ok(report) => {
                                    let all_passed = report.passed;
                                    json!({
                                        "jsonrpc": "2.0",
                                        "id": id,
                                        "result": {
                                            "content": [
                                                {
                                                    "type": "text",
                                                    "text": if all_passed {
                                                        format!("Successfully executed routine: {}. All assertions passed.", path_arg)
                                                    } else {
                                                        format!("Executed routine: {}. One or more assertions failed.", path_arg)
                                                    }
                                                }
                                            ],
                                            "isError": !all_passed
                                        }
                                    })
                                }
                                Err(e) => {
                                    json!({
                                        "jsonrpc": "2.0",
                                        "id": id,
                                        "result": {
                                            "content": [
                                                {
                                                    "type": "text",
                                                    "text": format!("Execution failed: {}", e)
                                                }
                                            ],
                                            "isError": true
                                        }
                                    })
                                }
                            }
                        }
                    }
                }
                }
                "create_routine" => {
                    let path_arg = request["params"]["arguments"]["path"].as_str().unwrap_or("");
                    let content_arg = request["params"]["arguments"]["content"].as_str().unwrap_or("");

                    if path_arg.is_empty() || content_arg.is_empty() {
                        json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "error": {
                                "code": -32602,
                                "message": "Invalid params: path and content are required"
                            }
                        })
                    } else {
                        let manifest = PaynalManifest::load_from_dir(Path::new(".")).unwrap_or_default();
                        let rel_yaml = if path_arg.ends_with(".yaml") || path_arg.ends_with(".yml") {
                            path_arg.to_string()
                        } else {
                            format!("{}.yaml", path_arg)
                        };
                        let rel_path_obj = Path::new(&rel_yaml);
                        if rel_path_obj.is_absolute()
                            || rel_path_obj.components().any(|c| {
                                matches!(
                                    c,
                                    std::path::Component::ParentDir
                                        | std::path::Component::RootDir
                                        | std::path::Component::Prefix(_)
                                )
                            })
                        {
                            json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "error": {
                                    "code": -32602,
                                    "message": "Invalid relative_path: path traversal ('..') or absolute paths are not allowed"
                                }
                            })
                        } else {
                            let target_path = Path::new(&manifest.root_dir).join("collections").join(&rel_yaml);

                            if let Some(parent) = target_path.parent() {
                                let _ = std::fs::create_dir_all(parent);
                            }

                            match std::fs::write(&target_path, content_arg) {
                                Ok(_) => json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "result": {
                                        "content": [
                                            {
                                                "type": "text",
                                                "text": format!("Successfully created routine at {}", target_path.display())
                                            }
                                        ]
                                    }
                                }),
                                Err(e) => json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "error": {
                                        "code": -32603,
                                        "message": format!("Failed to write file: {}", e)
                                    }
                                }),
                            }
                        }
                    }
                }
                "list_global_workspaces" => {
                    let registry = crate::commands::global::GlobalRegistry::load().unwrap_or_default();
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "content": [
                                {
                                    "type": "text",
                                    "text": serde_json::to_string_pretty(&registry.workspaces).unwrap_or_default()
                                }
                            ]
                        }
                    })
                }
                "discover_global_routines" => {
                    let query = request["params"]["arguments"]["query"].as_str().map(|s| s.to_string());
                    let registry = crate::commands::global::GlobalRegistry::load().unwrap_or_default();
                    let mut routes = crate::commands::global::collect_discovered_routes(&registry);
                    if let Some(q) = query {
                        let q_lower = q.to_lowercase();
                        routes.retain(|r| {
                            r.handle.to_lowercase().contains(&q_lower)
                                || r.workspace.to_lowercase().contains(&q_lower)
                                || r.path.to_lowercase().contains(&q_lower)
                                || r.name.to_lowercase().contains(&q_lower)
                                || r.method.to_lowercase().contains(&q_lower)
                                || r.url.to_lowercase().contains(&q_lower)
                                || r.description.to_lowercase().contains(&q_lower)
                        });
                    }
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "content": [
                                {
                                    "type": "text",
                                    "text": serde_json::to_string_pretty(&routes).unwrap_or_default()
                                }
                            ]
                        }
                    })
                }
                "execute_inline_request" => {
                    let url = request["params"]["arguments"]["url"].as_str().unwrap_or("");
                    if url.trim().is_empty() {
                        json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "error": {
                                "code": -32602,
                                "message": "Invalid params: url is required"
                            }
                        })
                    } else {
                        let method = request["params"]["arguments"]["method"].as_str().unwrap_or("GET").to_uppercase();
                        let body = request["params"]["arguments"]["body"].as_str().map(|s| s.to_string());
                        let ws_opt = request["params"]["arguments"]["workspace"].as_str().map(|s| s.trim().strip_prefix('@').unwrap_or(s).to_string());

                        let mut headers = HashMap::new();
                        if let Some(h_obj) = request["params"]["arguments"]["headers"].as_object() {
                            for (k, v) in h_obj {
                                if let Some(v_str) = v.as_str() {
                                    headers.insert(k.clone(), v_str.to_string());
                                }
                            }
                        }

                        let (manifest, ctx) = if let Some(ws_name) = &ws_opt {
                            let registry = crate::commands::global::GlobalRegistry::load().unwrap_or_default();
                            if let Some(entry) = registry.find_workspace(ws_name) {
                                let ws_path = PathBuf::from(&entry.path);
                                let mf = PaynalManifest::load_from_dir(&ws_path).unwrap_or_default();
                                let ct = crate::evaluator::VariableContext::new_with_dir(&ws_path, None);
                                (mf, ct)
                            } else {
                                (PaynalManifest::load_from_dir(Path::new(".")).unwrap_or_default(), crate::evaluator::VariableContext::new())
                            }
                        } else {
                            (PaynalManifest::load_from_dir(Path::new(".")).unwrap_or_default(), crate::evaluator::VariableContext::new())
                        };

                        let runner = HttpRunner::new_with_timeout(
                            manifest.validate_certificates,
                            manifest.proxy.as_deref(),
                            Some(manifest.timeout_ms),
                        );

                        let req_spec = crate::models::RequestSpec {
                            method: method.clone(),
                            url: url.to_string(),
                            params: None,
                            headers: headers.clone(),
                            body: body.clone(),
                            form_data: None,
                            timeout_ms: None,
                            retry: None,
                            follow_redirects: Some(true),
                            auth: None,
                            output_sections: None,
                        };

                        match runner.execute(&req_spec, None, &ctx).await {
                            Ok(res) => {
                                let mut headers_map = serde_json::Map::new();
                                for (k, v) in &res.headers {
                                    headers_map.insert(k.as_str().to_string(), serde_json::Value::String(v.to_str().unwrap_or("").to_string()));
                                }
                                let result_obj = json!({
                                    "status": res.status,
                                    "statusText": res.status_text,
                                    "durationMs": res.duration_ms,
                                    "headers": headers_map,
                                    "assertsPassed": res.asserts_passed,
                                    "assertMessages": res.assert_messages,
                                    "body": res.body,
                                });

                                json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "result": {
                                        "content": [
                                            {
                                                "type": "text",
                                                "text": serde_json::to_string_pretty(&result_obj).unwrap_or_default()
                                            }
                                        ],
                                        "isError": !res.asserts_passed
                                    }
                                })
                            }
                            Err(e) => {
                                json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "result": {
                                        "content": [
                                            {
                                                "type": "text",
                                                "text": format!("Execution failed: {}", e)
                                            }
                                        ],
                                        "isError": true
                                    }
                                })
                            }
                        }
                    }
                }
                _ => json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32601,
                        "message": "Tool not found"
                    }
                }),
            }
        }
        _ => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32601,
                "message": format!("Method not found: {}", method)
            }
        }),
    };

    Some(response)
}

pub async fn execute_mcp() -> Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut handle = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        if line.trim().is_empty() {
            continue;
        }

        let request: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if let Some(response) = process_mcp_request(&request).await {
            let res_str = serde_json::to_string(&response)?;
            writeln!(handle, "{}", res_str)?;
            handle.flush()?;
        }
    }

    Ok(())
}
