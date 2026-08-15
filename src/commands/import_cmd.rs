use anyhow::{Context, Result};
use serde_json::Value;
use std::fs;
use std::path::Path;

pub fn execute_import(file_path: String, out_dir: String) -> Result<()> {
    let path = Path::new(&file_path);
    if !path.exists() {
        anyhow::bail!("Import file not found: {}", file_path);
    }

    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read file {}", path.display()))?;

    let json_val: Value = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse JSON in {}", path.display()))?;

    let mut count = 0;

    // Check if Postman Collection v2.1
    if json_val.get("info").is_some() && json_val.get("item").is_some() {
        println!("📦 Importing Postman v2.1 collection...");
        if let Some(items) = json_val["item"].as_array() {
            count += import_postman_items(items, Path::new(&out_dir))?;
        }
    } else if json_val.get("_type").and_then(|t| t.as_str()) == Some("export") {
        println!("📦 Importing Insomnia v4 collection...");
        if let Some(resources) = json_val["resources"].as_array() {
            count += import_insomnia_resources(resources, Path::new(&out_dir))?;
        }
    } else {
        anyhow::bail!("Unrecognized collection format in {}. Expected Postman v2.1 or Insomnia v4 export.", path.display());
    }

    println!("✅ Successfully imported {} requests into: {}", count, out_dir);
    Ok(())
}

fn import_postman_items(items: &[Value], target_dir: &Path) -> Result<usize> {
    let mut count = 0;
    for item in items {
        if let Some(name) = item["name"].as_str() {
            if let Some(request) = item.get("request") {
                let method = request["method"].as_str().unwrap_or("GET").to_string();
                let url = if let Some(raw) = request["url"]["raw"].as_str() {
                    raw.to_string()
                } else if let Some(raw) = request["url"].as_str() {
                    raw.to_string()
                } else {
                    "https://api.example.com".to_string()
                };

                let mut headers = std::collections::HashMap::new();
                if let Some(hdr_arr) = request["header"].as_array() {
                    for h in hdr_arr {
                        if let (Some(k), Some(v)) = (h["key"].as_str(), h["value"].as_str()) {
                            headers.insert(k.to_string(), v.to_string());
                        }
                    }
                }

                let body = request["body"]["raw"].as_str().map(|s| s.to_string());

                let safe_name = name.to_lowercase().replace(' ', "_");
                let file_path = target_dir.join(format!("{}.yaml", safe_name));

                if let Some(parent) = file_path.parent() {
                    let _ = fs::create_dir_all(parent);
                }

                let paynal_file = serde_json::json!({
                    "version": "1",
                    "name": safe_name,
                    "request": {
                        "method": method,
                        "url": url,
                        "headers": headers,
                        "body": body.unwrap_or_default()
                    },
                    "assert": {
                        "status": 200
                    }
                });

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
    let mut count = 0;
    for res in resources {
        if res["_type"].as_str() == Some("request") {
            let name = res["name"].as_str().unwrap_or("imported_request");
            let method = res["method"].as_str().unwrap_or("GET").to_string();
            let url = res["url"].as_str().unwrap_or("https://api.example.com").to_string();

            let mut headers = std::collections::HashMap::new();
            if let Some(hdr_arr) = res["headers"].as_array() {
                for h in hdr_arr {
                    if let (Some(k), Some(v)) = (h["name"].as_str(), h["value"].as_str()) {
                        headers.insert(k.to_string(), v.to_string());
                    }
                }
            }

            let body = res["body"]["text"].as_str().map(|s| s.to_string());

            let safe_name = name.to_lowercase().replace(' ', "_");
            let file_path = target_dir.join(format!("{}.yaml", safe_name));

            if let Some(parent) = file_path.parent() {
                let _ = fs::create_dir_all(parent);
            }

            let paynal_file = serde_json::json!({
                "version": "1",
                "name": safe_name,
                "request": {
                    "method": method,
                    "url": url,
                    "headers": headers,
                    "body": body.unwrap_or_default()
                },
                "assert": {
                    "status": 200
                }
            });

            let yaml_str = serde_yaml::to_string(&paynal_file)?;
            fs::write(&file_path, yaml_str)?;
            count += 1;
        }
    }
    Ok(count)
}
