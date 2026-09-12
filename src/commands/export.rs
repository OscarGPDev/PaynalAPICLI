use crate::cli::ExportType;
use crate::models::{PaynalFile, RequestSpec};
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

fn find_yaml_files_recursive(dir: &Path) -> Vec<PathBuf> {
    let mut yaml_files = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                yaml_files.extend(find_yaml_files_recursive(&path));
            } else if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if ext == "yaml" || ext == "yml" {
                        yaml_files.push(path);
                    }
                }
            }
        }
    }
    yaml_files
}

fn export_single_file(target: &Path, r#type: ExportType) -> Result<()> {
    let content = fs::read_to_string(target)
        .with_context(|| format!("Failed to read {}", target.display()))?;

    let paynal_file: PaynalFile = serde_yaml::from_str(&content)
        .with_context(|| format!("Failed to parse YAML {}", target.display()))?;

    match r#type {
        ExportType::Curl => {
            let out_file = target.with_extension("curl.sh");
            let mut curl_cmds = Vec::new();

            if paynal_file.is_routine() {
                for step in &paynal_file.steps {
                    curl_cmds.push(build_curl_command(&step.request));
                }
            } else if let Some(req) = &paynal_file.request {
                curl_cmds.push(build_curl_command(req));
            }

            let full_sh = format!("#!/usr/bin/env bash\n# Exported from Paynal: {}\n\n{}\n", paynal_file.name, curl_cmds.join("\n\n"));
            fs::write(&out_file, full_sh)?;
            println!("📦 Exported cURL script: {}", out_file.display());
        }
        ExportType::Postman => {
            let out_file = target.with_extension("postman_collection.json");
            let mut items = Vec::new();

            if paynal_file.is_routine() {
                for step in &paynal_file.steps {
                    let step_name = step.name.as_deref().unwrap_or(&step.id);
                    items.push(build_postman_item(step_name, &step.request));
                }
            } else if let Some(req) = &paynal_file.request {
                items.push(build_postman_item(&paynal_file.name, req));
            }

            let postman_json = serde_json::json!({
                "info": {
                    "name": paynal_file.name,
                    "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
                },
                "item": items
            });
            fs::write(&out_file, serde_json::to_string_pretty(&postman_json)?)?;
            println!("📦 Exported Postman collection: {}", out_file.display());
        }
        ExportType::Insomnia => {
            let out_file = target.with_extension("insomnia.json");
            let mut resources = Vec::new();

            if paynal_file.is_routine() {
                for step in &paynal_file.steps {
                    let step_name = step.name.as_deref().unwrap_or(&step.id);
                    resources.push(build_insomnia_resource(step_name, &step.request));
                }
            } else if let Some(req) = &paynal_file.request {
                resources.push(build_insomnia_resource(&paynal_file.name, req));
            }

            let insomnia_json = serde_json::json!({
                "_type": "export",
                "__export_format": 4,
                "resources": resources
            });
            fs::write(&out_file, serde_json::to_string_pretty(&insomnia_json)?)?;
            println!("📦 Exported Insomnia collection: {}", out_file.display());
        }
    }

    Ok(())
}

pub fn execute_export(path_str: String, r#type: ExportType) -> Result<()> {
    let mut target = PathBuf::from(&path_str);
    if !target.exists() {
        let collections_base = Path::new("collections");
        let relative_yaml = if path_str.ends_with(".yaml") || path_str.ends_with(".yml") {
            path_str.clone()
        } else {
            format!("{}.yaml", path_str)
        };
        target = collections_base.join(&relative_yaml);

        if !target.exists() {
            let dir_candidate = collections_base.join(&path_str);
            if dir_candidate.is_dir() {
                target = dir_candidate;
            }
        }
    }

    if !target.exists() {
        anyhow::bail!("Target path not found: {}", path_str);
    }

    if target.is_dir() {
        let files = find_yaml_files_recursive(&target);
        if files.is_empty() {
            println!("⚠️  No YAML files found in directory: {}", target.display());
            return Ok(());
        }
        for file in files {
            export_single_file(&file, r#type)?;
        }
    } else {
        export_single_file(&target, r#type)?;
    }

    Ok(())
}

fn build_curl_command(req: &RequestSpec) -> String {
    let full_url = if let Some(params) = &req.params {
        if !params.is_empty() {
            let query_str = params
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join("&");
            if req.url.contains('?') {
                format!("{}&{}", req.url, query_str)
            } else {
                format!("{}?{}", req.url, query_str)
            }
        } else {
            req.url.clone()
        }
    } else {
        req.url.clone()
    };

    let mut cmd = format!("curl -X {} \"{}\"", req.method, full_url);

    if let Some(auth) = &req.auth {
        match auth {
            crate::models::AuthSpec::Bearer { token } => {
                cmd.push_str(&format!(" -H \"Authorization: Bearer {}\"", token));
            }
            crate::models::AuthSpec::Basic { username, password } => {
                if let Some(p) = password {
                    cmd.push_str(&format!(" -u \"{}:{}\"", username, p));
                } else {
                    cmd.push_str(&format!(" -u \"{}\"", username));
                }
            }
            crate::models::AuthSpec::ApiKey { key, value, r#in } => {
                if r#in.to_lowercase() != "query" {
                    cmd.push_str(&format!(" -H \"{}: {}\"", key, value));
                }
            }
        }
    }

    for (k, v) in &req.headers {
        cmd.push_str(&format!(" -H \"{}: {}\"", k, v));
    }

    if let Some(form_fields) = &req.form_data {
        for (k, v) in form_fields {
            if v.starts_with('@') {
                cmd.push_str(&format!(" -F \"{}={}\"", k, v));
            } else {
                cmd.push_str(&format!(" -F \"{}={}\"", k, v));
            }
        }
    } else if let Some(b) = &req.body {
        if !b.is_empty() {
            if b.starts_with('@') {
                cmd.push_str(&format!(" --data-binary \"{}\"", b));
            } else {
                let escaped_body = b.trim().replace('\'', "'\\''");
                cmd.push_str(&format!(" -d '{}'", escaped_body));
            }
        }
    }
    cmd
}

fn build_postman_item(name: &str, req: &RequestSpec) -> serde_json::Value {
    let body_json = if let Some(form_fields) = &req.form_data {
        let formdata_items: Vec<_> = form_fields
            .iter()
            .map(|(k, v)| {
                if v.starts_with('@') {
                    serde_json::json!({
                        "key": k,
                        "type": "file",
                        "src": v.trim_start_matches('@')
                    })
                } else {
                    serde_json::json!({
                        "key": k,
                        "value": v,
                        "type": "text"
                    })
                }
            })
            .collect();
        serde_json::json!({
            "mode": "formdata",
            "formdata": formdata_items
        })
    } else {
        serde_json::json!({
            "mode": "raw",
            "raw": req.body.as_deref().unwrap_or("")
        })
    };

    let mut request_map = serde_json::json!({
        "method": req.method,
        "url": { "raw": req.url },
        "header": req.headers.iter().map(|(k, v)| serde_json::json!({"key": k, "value": v})).collect::<Vec<_>>(),
        "body": body_json
    });

    if let Some(auth) = &req.auth {
        let auth_json = match auth {
            crate::models::AuthSpec::Bearer { token } => serde_json::json!({
                "type": "bearer",
                "bearer": [{ "key": "token", "value": token, "type": "string" }]
            }),
            crate::models::AuthSpec::Basic { username, password } => serde_json::json!({
                "type": "basic",
                "basic": [
                    { "key": "username", "value": username, "type": "string" },
                    { "key": "password", "value": password.as_deref().unwrap_or(""), "type": "string" }
                ]
            }),
            crate::models::AuthSpec::ApiKey { key, value, r#in } => serde_json::json!({
                "type": "apikey",
                "apikey": [
                    { "key": "key", "value": key, "type": "string" },
                    { "key": "value", "value": value, "type": "string" },
                    { "key": "in", "value": r#in, "type": "string" }
                ]
            }),
        };
        request_map["auth"] = auth_json;
    }

    serde_json::json!({
        "name": name,
        "request": request_map
    })
}

fn build_insomnia_resource(name: &str, req: &RequestSpec) -> serde_json::Value {
    let body_json = if let Some(form_fields) = &req.form_data {
        let params: Vec<_> = form_fields
            .iter()
            .map(|(k, v)| {
                if v.starts_with('@') {
                    serde_json::json!({
                        "name": k,
                        "type": "file",
                        "fileName": v.trim_start_matches('@')
                    })
                } else {
                    serde_json::json!({
                        "name": k,
                        "value": v
                    })
                }
            })
            .collect();
        serde_json::json!({
            "mimeType": "multipart/form-data",
            "params": params
        })
    } else {
        serde_json::json!({
            "text": req.body.as_deref().unwrap_or("")
        })
    };

    let mut res_map = serde_json::json!({
        "_type": "request",
        "name": name,
        "method": req.method,
        "url": req.url,
        "headers": req.headers.iter().map(|(k, v)| serde_json::json!({"name": k, "value": v})).collect::<Vec<_>>(),
        "body": body_json
    });

    if let Some(params) = &req.params {
        let param_items: Vec<_> = params
            .iter()
            .map(|(k, v)| serde_json::json!({"name": k, "value": v}))
            .collect();
        res_map["parameters"] = serde_json::json!(param_items);
    }

    if let Some(auth) = &req.auth {
        let auth_json = match auth {
            crate::models::AuthSpec::Bearer { token } => serde_json::json!({
                "type": "bearer",
                "token": token
            }),
            crate::models::AuthSpec::Basic { username, password } => serde_json::json!({
                "type": "basic",
                "username": username,
                "password": password.as_deref().unwrap_or("")
            }),
            crate::models::AuthSpec::ApiKey { key, value, .. } => serde_json::json!({
                "type": "apikey",
                "key": key,
                "value": value
            }),
        };
        res_map["authentication"] = auth_json;
    }

    res_map
}
