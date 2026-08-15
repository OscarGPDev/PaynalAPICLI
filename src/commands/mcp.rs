use crate::manifest::PaynalManifest;
use anyhow::Result;
use serde_json::json;
use std::io::{self, BufRead, Write};
use std::path::Path;
use walkdir::WalkDir;

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

        let method = request["method"].as_str().unwrap_or("");
        let id = &request["id"];

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
                        "version": "0.1.0"
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
                            "description": "Execute an API request or routine by path",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "path": {
                                        "type": "string",
                                        "description": "Relative path to request or routine (e.g. auth/login)"
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
                        let mut routines = Vec::new();

                        if collections_dir.exists() {
                            for entry in WalkDir::new(&collections_dir).into_iter().filter_map(|e| e.ok()) {
                                if entry.path().is_file() {
                                    if let Some(ext) = entry.path().extension() {
                                        if ext == "yaml" || ext == "yml" {
                                            if let Ok(rel) = entry.path().strip_prefix(&collections_dir) {
                                                routines.push(rel.display().to_string());
                                            }
                                        }
                                    }
                                }
                            }
                        }

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
                            let target_path = Path::new(&manifest.root_dir).join("collections").join(rel_yaml);

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
                "result": {}
            }),
        };

        let res_str = serde_json::to_string(&response)?;
        writeln!(handle, "{}", res_str)?;
        handle.flush()?;
    }

    Ok(())
}
