use crate::manifest::PaynalManifest;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

pub fn execute_add(
    path_str: String,
    get: bool,
    r#type: String,
    routine: bool,
    body_override: Option<String>,
) -> Result<()> {
    let method = if get { "GET".to_string() } else { r#type.to_uppercase() };
    
    // Load manifest to determine defaultBodyType
    let manifest = PaynalManifest::load_from_dir(Path::new(".")).unwrap_or_default();
    let body_type = body_override
        .map(|s| s.to_lowercase())
        .unwrap_or_else(|| manifest.get_default_body_type(&method));

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

    let (req_header, req_body, step_header, step_body) = match body_type.as_str() {
        "json" => (
            "    Content-Type: \"application/json\"",
            "  body: |\n    {\n      \"key\": \"value\"\n    }",
            "        Content-Type: \"application/json\"",
            "      body: |\n        {\n          \"key\": \"value\"\n        }",
        ),
        "form" | "urlencoded" => (
            "    Content-Type: \"application/x-www-form-urlencoded\"",
            "  body: \"field1=value1&field2=value2\"",
            "        Content-Type: \"application/x-www-form-urlencoded\"",
            "      body: \"field1=value1&field2=value2\"",
        ),
        "multipart" | "formdata" | "form-data" => (
            "    # Content-Type is set automatically for multipart/form-data",
            "  formData:\n    description: \"Sample file upload\"\n    file: \"@./assets/sample.png\"",
            "        # Content-Type is set automatically for multipart/form-data",
            "      formData:\n        description: \"Sample file upload\"\n        file: \"@./assets/sample.png\"",
        ),
        "file" | "binary" => (
            "    Content-Type: \"application/octet-stream\"",
            "  body: \"@./assets/sample.bin\"",
            "        Content-Type: \"application/octet-stream\"",
            "      body: \"@./assets/sample.bin\"",
        ),
        "xml" => (
            "    Content-Type: \"application/xml\"",
            "  body: |\n    <?xml version=\"1.0\" encoding=\"UTF-8\"?>\n    <root>\n      <data>value</data>\n    </root>",
            "        Content-Type: \"application/xml\"",
            "      body: |\n        <?xml version=\"1.0\" encoding=\"UTF-8\"?>\n        <root>\n          <data>value</data>\n        </root>",
        ),
        "text" | "raw" => (
            "    Content-Type: \"text/plain\"",
            "  body: \"\"",
            "        Content-Type: \"text/plain\"",
            "      body: \"\"",
        ),
        _ => (
            "    Accept: \"application/json\"",
            "  body: \"\"",
            "        Accept: \"application/json\"",
            "      body: \"\"",
        ),
    };

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
{step_header}
{step_body}
    capture:
      resData: "$.data"
    assert:
      status: 200
"#,
            name = file_stem,
            method = method,
            step_header = step_header,
            step_body = step_body
        )
    } else {
        format!(
            r#"version: "1"
name: "{name}"
request:
  method: "{method}"
  url: "${{BASE_URL}}/endpoint"
  headers:
{req_header}
{req_body}
assert:
  status: 200
"#,
            name = file_stem,
            method = method,
            req_header = req_header,
            req_body = req_body
        )
    };

    fs::write(&target_path, content)
        .with_context(|| format!("Failed to create {}", target_path.display()))?;

    println!("✅ Created request template ({}, body: {}) at: {}", method, body_type, target_path.display());
    Ok(())
}
