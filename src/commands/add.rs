use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

pub fn execute_add(
    path_str: String,
    get: bool,
    r#type: String,
    routine: bool,
) -> Result<()> {
    let method = if get { "GET".to_string() } else { r#type.to_uppercase() };
    
    // Normalize target file path (append .yaml if not present)
    let relative_path = if path_str.ends_with(".yaml") || path_str.ends_with(".yml") {
        path_str.clone()
    } else {
        format!("{}.yaml", path_str)
    };

    let target_path = Path::new("collections").join(&relative_path);

    if let Some(parent) = target_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create parent directory {}", parent.display()))?;
        }
    }

    if target_path.exists() {
        println!("⚠️  Target file already exists: {}", target_path.display());
        return Ok(());
    }

    let file_stem = target_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Request");

    let content = if routine {
        format!(
            r#"version: "1"
name: "{name}"
description: "Multi-step routine for {name}"
vars:
  baseUrl: "${{BASE_URL}}"

steps:
  - id: "step-1"
    name: "Initial Step"
    request:
      method: "{method}"
      url: "${{baseUrl}}/endpoint"
      headers:
        Content-Type: "application/json"
    capture:
      resData: "$.data"
    assert:
      status: 200
"#,
            name = file_stem,
            method = method
        )
    } else {
        format!(
            r#"version: "1"
name: "{name}"
request:
  method: "{method}"
  url: "${{BASE_URL}}/endpoint"
  headers:
    Content-Type: "application/json"
  body: ""
assert:
  status: 200
"#,
            name = file_stem,
            method = method
        )
    };

    fs::write(&target_path, content)
        .with_context(|| format!("Failed to create {}", target_path.display()))?;

    println!("✅ Created request template at: {}", target_path.display());
    Ok(())
}
