use crate::models::{AssertSpec, AuthSpec, PaynalFile, RequestSpec};
use anyhow::{Context, Result};
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub fn execute_import(file_path: String, out_dir: String) -> Result<()> {
    let path = Path::new(&file_path);
    if !path.exists() {
        anyhow::bail!("Import file or directory not found: {}", file_path);
    }

    let target_dir = Path::new(&out_dir);
    let mut count = 0;

    if path.is_dir() {
        // Check if directory contains .bru files
        let bru_files = find_files_with_ext(path, "bru");
        if !bru_files.is_empty() {
            println!("📦 Importing Bruno collection from directory: {} (found {} .bru files)...", path.display(), bru_files.len());
            for bru_file in bru_files {
                let rel = bru_file.strip_prefix(path).unwrap_or(&bru_file);
                let dest = target_dir.join(rel).with_extension("yaml");
                if import_single_bru(&bru_file, &dest)? {
                    count += 1;
                }
            }
        } else {
            // Check for JSON exports in directory
            let json_files = find_files_with_ext(path, "json");
            if json_files.is_empty() {
                anyhow::bail!("No .bru or .json files found in directory: {}", path.display());
            }
            for json_file in json_files {
                count += import_json_file(&json_file, target_dir)?;
            }
        }
    } else {
        // Single file import
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext == "bru" {
            println!("📦 Importing Bruno request: {}...", path.display());
            let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("imported_request");
            let dest = target_dir.join(format!("{}.yaml", file_stem));
            if import_single_bru(path, &dest)? {
                count += 1;
            }
        } else {
            count += import_json_file(path, target_dir)?;
        }
    }

    println!("✅ Successfully imported {} requests into: {}", count, out_dir);
    Ok(())
}

fn find_files_with_ext(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut result = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let p = entry.path();
            if p.is_dir() {
                result.extend(find_files_with_ext(&p, ext));
            } else if p.is_file() {
                if let Some(file_ext) = p.extension().and_then(|e| e.to_str()) {
                    if file_ext.eq_ignore_ascii_case(ext) {
                        result.push(p);
                    }
                }
            }
        }
    }
    result
}

fn translate_vars(input: &str) -> String {
    // Matches Postman {{var}} and Insomnia {{ _.var }} / {{ var }}
    let re = Regex::new(r"\{\{\s*(?:_\.)?([a-zA-Z0-9_]+)\s*\}\}").unwrap();
    re.replace_all(input, |caps: &regex::Captures| format!("${{{}}}", &caps[1])).to_string()
}

fn import_json_file(path: &Path, target_dir: &Path) -> Result<usize> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read file {}", path.display()))?;

    let json_val: Value = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse JSON in {}", path.display()))?;

    // Check if Postman Collection v2.1
    if json_val.get("info").is_some() && json_val.get("item").is_some() {
        println!("📦 Importing Postman v2.1 collection: {}...", path.display());
        if let Some(items) = json_val["item"].as_array() {
            return import_postman_items(items, target_dir);
        }
    } else if json_val.get("_type").and_then(|t| t.as_str()) == Some("export") {
        println!("📦 Importing Insomnia v4 collection: {}...", path.display());
        if let Some(resources) = json_val["resources"].as_array() {
            return import_insomnia_resources(resources, target_dir);
        }
    }

    anyhow::bail!("Unrecognized collection format in {}. Expected Postman v2.1 or Insomnia v4 export.", path.display());
}

fn import_postman_items(items: &[Value], target_dir: &Path) -> Result<usize> {
    let mut count = 0;
    for item in items {
        if let Some(name) = item["name"].as_str() {
            if let Some(request) = item.get("request") {
                let method = request["method"].as_str().unwrap_or("GET").to_string();
                let raw_url = if let Some(raw) = request["url"]["raw"].as_str() {
                    raw.to_string()
                } else if let Some(raw) = request["url"].as_str() {
                    raw.to_string()
                } else {
                    "https://api.example.com".to_string()
                };
                let url = translate_vars(&raw_url);

                // Headers
                let mut headers = HashMap::new();
                if let Some(hdr_arr) = request["header"].as_array() {
                    for h in hdr_arr {
                        if let (Some(k), Some(v)) = (h["key"].as_str(), h["value"].as_str()) {
                            headers.insert(translate_vars(k), translate_vars(v));
                        }
                    }
                }

                // Query Parameters
                let mut params = HashMap::new();
                if let Some(query_arr) = request["url"]["query"].as_array() {
                    for q in query_arr {
                        if let (Some(k), Some(v)) = (q["key"].as_str(), q["value"].as_str()) {
                            params.insert(translate_vars(k), translate_vars(v));
                        }
                    }
                }
                let params_opt = if params.is_empty() { None } else { Some(params) };

                // Body & Form Data
                let mut form_data = HashMap::new();
                let mut body = None;

                if let Some(body_obj) = request.get("body") {
                    let mode = body_obj["mode"].as_str().unwrap_or("");
                    if mode == "formdata" {
                        if let Some(form_arr) = body_obj["formdata"].as_array() {
                            for f in form_arr {
                                if let Some(k) = f["key"].as_str() {
                                    if f["type"].as_str() == Some("file") {
                                        let src = f["src"].as_str().unwrap_or("");
                                        form_data.insert(k.to_string(), format!("@{}", src));
                                    } else if let Some(v) = f["value"].as_str() {
                                        form_data.insert(k.to_string(), translate_vars(v));
                                    }
                                }
                            }
                        }
                    } else if let Some(raw_body) = body_obj["raw"].as_str() {
                        body = Some(translate_vars(raw_body));
                    }
                }
                let form_data_opt = if form_data.is_empty() { None } else { Some(form_data) };

                // Auth
                let mut auth = None;
                if let Some(auth_obj) = request.get("auth") {
                    let auth_type = auth_obj["type"].as_str().unwrap_or("");
                    if auth_type == "bearer" {
                        if let Some(bearer_arr) = auth_obj["bearer"].as_array() {
                            for b in bearer_arr {
                                if b["key"].as_str() == Some("token") {
                                    if let Some(token_val) = b["value"].as_str() {
                                        auth = Some(AuthSpec::Bearer {
                                            token: translate_vars(token_val),
                                        });
                                    }
                                }
                            }
                        }
                    } else if auth_type == "basic" {
                        let mut user = String::new();
                        let mut pass = None;
                        if let Some(basic_arr) = auth_obj["basic"].as_array() {
                            for b in basic_arr {
                                if b["key"].as_str() == Some("username") {
                                    user = translate_vars(b["value"].as_str().unwrap_or(""));
                                } else if b["key"].as_str() == Some("password") {
                                    pass = b["value"].as_str().map(|v| translate_vars(v));
                                }
                            }
                        }
                        if !user.is_empty() {
                            auth = Some(AuthSpec::Basic {
                                username: user,
                                password: pass,
                            });
                        }
                    }
                }

                let safe_name = name.to_lowercase().replace(' ', "_");
                let file_path = target_dir.join(format!("{}.yaml", safe_name));

                if let Some(parent) = file_path.parent() {
                    let _ = fs::create_dir_all(parent);
                }

                let paynal_file = PaynalFile {
                    version: "1".to_string(),
                    name: safe_name,
                    description: Some(format!("Imported from Postman: {}", name)),
                    vars: HashMap::new(),
                    request: Some(RequestSpec {
                        method,
                        url,
                        params: params_opt,
                        headers,
                        body,
                        form_data: form_data_opt,
                        timeout_ms: None,
                        retry: None,
                        follow_redirects: None,
                        auth,
                    }),
                    steps: Vec::new(),
                    assert: Some(AssertSpec {
                        status: Some(200),
                        ..Default::default()
                    }),
                    continue_on_failure: None,
                };

                let yaml_str = serde_yaml::to_string(&paynal_file)?;
                fs::write(&file_path, yaml_str)?;
                count += 1;
            }

            if let Some(sub_items) = item["item"].as_array() {
                let sub_dir = target_dir.join(name.to_lowercase().replace(' ', "_"));
                count += import_postman_items(sub_items, &sub_dir)?;
            }
        }
    }
    Ok(count)
}

fn import_insomnia_resources(resources: &[Value], target_dir: &Path) -> Result<usize> {
    // Build folder tree mapping: folder_id -> (folder_name, Option<parent_folder_id>)
    let mut folder_map: HashMap<String, (String, Option<String>)> = HashMap::new();
    for res in resources {
        if res["_type"].as_str() == Some("request_group") {
            if let Some(id) = res["_id"].as_str() {
                let name = res["name"].as_str().unwrap_or("group").to_lowercase().replace(' ', "_");
                let parent_id = res["parentId"].as_str().map(|s| s.to_string());
                folder_map.insert(id.to_string(), (name, parent_id));
            }
        }
    }

    let mut count = 0;
    for res in resources {
        if res["_type"].as_str() == Some("request") {
            let name = res["name"].as_str().unwrap_or("imported_request");
            let method = res["method"].as_str().unwrap_or("GET").to_string();
            let raw_url = res["url"].as_str().unwrap_or("https://api.example.com").to_string();
            let url = translate_vars(&raw_url);

            // Headers
            let mut headers = HashMap::new();
            if let Some(hdr_arr) = res["headers"].as_array() {
                for h in hdr_arr {
                    if let (Some(k), Some(v)) = (h["name"].as_str(), h["value"].as_str()) {
                        headers.insert(translate_vars(k), translate_vars(v));
                    }
                }
            }

            // Query parameters
            let mut params = HashMap::new();
            if let Some(param_arr) = res["parameters"].as_array() {
                for p in param_arr {
                    if let (Some(k), Some(v)) = (p["name"].as_str(), p["value"].as_str()) {
                        params.insert(translate_vars(k), translate_vars(v));
                    }
                }
            }
            let params_opt = if params.is_empty() { None } else { Some(params) };

            // Body & Form Data
            let mut form_data = HashMap::new();
            let mut body = None;

            if let Some(body_obj) = res.get("body") {
                let mime = body_obj["mimeType"].as_str().unwrap_or("");
                if mime == "multipart/form-data" {
                    if let Some(param_arr) = body_obj["params"].as_array() {
                        for p in param_arr {
                            if let Some(name_str) = p["name"].as_str() {
                                if let Some(file_str) = p["fileName"].as_str() {
                                    form_data.insert(name_str.to_string(), format!("@{}", file_str));
                                } else if let Some(val_str) = p["value"].as_str() {
                                    form_data.insert(name_str.to_string(), translate_vars(val_str));
                                }
                            }
                        }
                    }
                } else if let Some(text_body) = body_obj["text"].as_str() {
                    body = Some(translate_vars(text_body));
                }
            }
            let form_data_opt = if form_data.is_empty() { None } else { Some(form_data) };

            // Auth
            let mut auth = None;
            if let Some(auth_obj) = res.get("authentication") {
                let auth_type = auth_obj["type"].as_str().unwrap_or("");
                if auth_type == "bearer" {
                    if let Some(token) = auth_obj["token"].as_str() {
                        auth = Some(AuthSpec::Bearer {
                            token: translate_vars(token),
                        });
                    }
                } else if auth_type == "basic" {
                    let user = auth_obj["username"].as_str().unwrap_or("");
                    let pass = auth_obj["password"].as_str().map(|p| translate_vars(p));
                    if !user.is_empty() {
                        auth = Some(AuthSpec::Basic {
                            username: translate_vars(user),
                            password: pass,
                        });
                    }
                } else if auth_type == "apikey" {
                    let key = auth_obj["key"].as_str().unwrap_or("");
                    let val = auth_obj["value"].as_str().unwrap_or("");
                    if !key.is_empty() {
                        auth = Some(AuthSpec::ApiKey {
                            key: translate_vars(key),
                            value: translate_vars(val),
                            r#in: "header".to_string(),
                        });
                    }
                }
            }

            // Resolve folder hierarchy using parentId
            let mut folder_path = PathBuf::new();
            let mut curr_parent = res["parentId"].as_str().map(|s| s.to_string());
            let mut hierarchy = Vec::new();
            while let Some(parent_id) = curr_parent {
                if let Some((folder_name, next_parent)) = folder_map.get(&parent_id) {
                    hierarchy.push(folder_name.clone());
                    curr_parent = next_parent.clone();
                } else {
                    break;
                }
            }
            hierarchy.reverse();
            for seg in hierarchy {
                folder_path = folder_path.join(seg);
            }

            let safe_name = name.to_lowercase().replace(' ', "_");
            let file_path = target_dir.join(&folder_path).join(format!("{}.yaml", safe_name));

            if let Some(parent) = file_path.parent() {
                let _ = fs::create_dir_all(parent);
            }

            let paynal_file = PaynalFile {
                version: "1".to_string(),
                name: safe_name,
                description: Some(format!("Imported from Insomnia: {}", name)),
                vars: HashMap::new(),
                request: Some(RequestSpec {
                    method,
                    url,
                    params: params_opt,
                    headers,
                    body,
                    form_data: form_data_opt,
                    timeout_ms: None,
                    retry: None,
                    follow_redirects: None,
                    auth,
                }),
                steps: Vec::new(),
                assert: Some(AssertSpec {
                    status: Some(200),
                    ..Default::default()
                }),
                continue_on_failure: None,
            };

            let yaml_str = serde_yaml::to_string(&paynal_file)?;
            fs::write(&file_path, yaml_str)?;
            count += 1;
        }
    }
    Ok(count)
}

fn import_single_bru(bru_path: &Path, dest_path: &Path) -> Result<bool> {
    let content = fs::read_to_string(bru_path)
        .with_context(|| format!("Failed to read Bruno file {}", bru_path.display()))?;

    if let Some(paynal_file) = parse_bru(&content, bru_path) {
        if let Some(parent) = dest_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let yaml_str = serde_yaml::to_string(&paynal_file)?;
        fs::write(dest_path, yaml_str)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

fn parse_bru(content: &str, file_path: &Path) -> Option<PaynalFile> {
    let default_name = file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("bru_request")
        .to_string();

    let mut name = default_name.clone();
    let mut method = "GET".to_string();
    let mut url = "https://api.example.com".to_string();
    let mut headers = HashMap::new();
    let mut params = HashMap::new();
    let mut body = None;
    let mut auth = None;
    let mut status_assert = Some(200);

    // Extract blocks: block_name { ... }
    let blocks = extract_bru_blocks(content);

    for (block_name, block_content) in &blocks {
        let bname = block_name.trim().to_lowercase();
        match bname.as_str() {
            "meta" => {
                for line in block_content.lines() {
                    let trimmed = line.trim();
                    if let Some((k, v)) = trimmed.split_once(':') {
                        if k.trim() == "name" {
                            let n = v.trim().trim_matches('"');
                            if !n.is_empty() {
                                name = n.to_string();
                            }
                        }
                    }
                }
            }
            "get" | "post" | "put" | "delete" | "patch" | "head" | "options" => {
                method = bname.to_uppercase();
                for line in block_content.lines() {
                    let trimmed = line.trim();
                    if let Some((k, v)) = trimmed.split_once(':') {
                        if k.trim() == "url" {
                            url = translate_vars(v.trim());
                        }
                    }
                }
            }
            "headers" => {
                for line in block_content.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
                        continue;
                    }
                    if let Some((k, v)) = trimmed.split_once(':') {
                        headers.insert(translate_vars(k.trim()), translate_vars(v.trim()));
                    }
                }
            }
            "params:query" => {
                for line in block_content.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
                        continue;
                    }
                    if let Some((k, v)) = trimmed.split_once(':') {
                        params.insert(translate_vars(k.trim()), translate_vars(v.trim()));
                    }
                }
            }
            "body:json" | "body:text" => {
                let trimmed = block_content.trim();
                if !trimmed.is_empty() {
                    body = Some(translate_vars(trimmed));
                }
            }
            "auth:bearer" => {
                for line in block_content.lines() {
                    let trimmed = line.trim();
                    if let Some((k, v)) = trimmed.split_once(':') {
                        if k.trim() == "token" {
                            auth = Some(AuthSpec::Bearer {
                                token: translate_vars(v.trim()),
                            });
                        }
                    }
                }
            }
            "auth:basic" => {
                let mut user = String::new();
                let mut pass = None;
                for line in block_content.lines() {
                    let trimmed = line.trim();
                    if let Some((k, v)) = trimmed.split_once(':') {
                        if k.trim() == "username" {
                            user = translate_vars(v.trim());
                        } else if k.trim() == "password" {
                            pass = Some(translate_vars(v.trim()));
                        }
                    }
                }
                if !user.is_empty() {
                    auth = Some(AuthSpec::Basic {
                        username: user,
                        password: pass,
                    });
                }
            }
            "assert" => {
                for line in block_content.lines() {
                    let trimmed = line.trim();
                    // e.g. "res.status: eq 200" or "res.status: 200"
                    if trimmed.starts_with("res.status") {
                        if let Some((_, v)) = trimmed.split_once(':') {
                            let val_part = v.trim().trim_start_matches("eq").trim();
                            if let Ok(st) = val_part.parse::<u16>() {
                                status_assert = Some(st);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    let params_opt = if params.is_empty() { None } else { Some(params) };

    Some(PaynalFile {
        version: "1".to_string(),
        name: name.to_lowercase().replace(' ', "_"),
        description: Some(format!("Imported from Bruno: {}", name)),
        vars: HashMap::new(),
        request: Some(RequestSpec {
            method,
            url,
            params: params_opt,
            headers,
            body,
            form_data: None,
            timeout_ms: None,
            retry: None,
            follow_redirects: None,
            auth,
        }),
        steps: Vec::new(),
        assert: Some(AssertSpec {
            status: status_assert,
            ..Default::default()
        }),
        continue_on_failure: None,
    })
}

fn extract_bru_blocks(content: &str) -> Vec<(String, String)> {
    let mut blocks = Vec::new();
    let chars: Vec<char> = content.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // Skip whitespace
        while i < len && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= len {
            break;
        }

        // Read block header up to '{'
        let header_start = i;
        while i < len && chars[i] != '{' {
            i += 1;
        }
        if i >= len {
            break;
        }

        let header = chars[header_start..i].iter().collect::<String>().trim().to_string();
        i += 1; // skip '{'

        // Read block body until matching '}'
        let body_start = i;
        let mut depth = 1;
        while i < len && depth > 0 {
            if chars[i] == '{' {
                depth += 1;
            } else if chars[i] == '}' {
                depth -= 1;
            }
            if depth > 0 {
                i += 1;
            }
        }

        let body = chars[body_start..i].iter().collect::<String>();
        if i < len && chars[i] == '}' {
            i += 1; // skip final '}'
        }

        if !header.is_empty() {
            blocks.push((header, body));
        }
    }

    blocks
}
