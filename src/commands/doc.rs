use crate::manifest::PaynalManifest;
use crate::models::{AssertSpec, PaynalFile, RequestSpec};
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

fn find_yaml_files(dir: &Path) -> Vec<PathBuf> {
    let mut yaml_files = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                yaml_files.extend(find_yaml_files(&path));
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

fn generate_doc_for_file(
    file_path: &Path,
    collections_base: &Path,
    docs_base: &Path,
    io_specs: &[String],
) -> Result<()> {
    let content = fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read {}", file_path.display()))?;

    let paynal_file: PaynalFile = serde_yaml::from_str(&content)
        .with_context(|| format!("Failed to parse YAML {}", file_path.display()))?;

    let rel_path = file_path
        .strip_prefix(collections_base)
        .unwrap_or_else(|_| file_path.file_name().map(Path::new).unwrap_or(file_path));

    let doc_path = docs_base.join(rel_path).with_extension("md");
    if let Some(parent) = doc_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let mut markdown = format!("# Documentation: {}\n\n", paynal_file.name);

    if let Some(desc) = &paynal_file.description {
        markdown.push_str(&format!("**Description:** {}\n\n", desc));
    }

    if !paynal_file.vars.is_empty() {
        markdown.push_str("## File Variables\n\n| Variable | Value |\n| --- | --- |\n");
        for (k, v) in &paynal_file.vars {
            markdown.push_str(&format!("| `{}` | `{}` |\n", k, v));
        }
        markdown.push('\n');
    }

    if paynal_file.is_routine() {
        markdown.push_str("## Routine Steps\n\n");
        for (idx, step) in paynal_file.steps.iter().enumerate() {
            let step_title = step.name.as_deref().unwrap_or(&step.id);
            markdown.push_str(&format!("### {}. {}\n\n", idx + 1, step_title));
            append_request_doc(&mut markdown, &step.request);

            if !step.capture.is_empty() {
                markdown.push_str("\n**Captures:**\n\n| Variable | Source / Path |\n| --- | --- |\n");
                for (k, v) in &step.capture {
                    markdown.push_str(&format!("| `{}` | `{}` |\n", k, v));
                }
                markdown.push('\n');
            }

            if let Some(assert) = &step.assert {
                append_assert_doc(&mut markdown, assert);
            }
        }
    } else if let Some(req) = &paynal_file.request {
        markdown.push_str("## Request Details\n\n");
        append_request_doc(&mut markdown, req);

        if let Some(assert) = &paynal_file.assert {
            append_assert_doc(&mut markdown, assert);
        }
    }

    if !io_specs.is_empty() {
        markdown.push_str("## Input & Output Specification\n\n");
        markdown.push_str("| Field | Type | Description |\n");
        markdown.push_str("| --- | --- | --- |\n");

        for io in io_specs {
            let parts: Vec<&str> = io.splitn(3, ':').collect();
            let field = parts.first().unwrap_or(&"field");
            let r#type = parts.get(1).unwrap_or(&"string");
            let desc = parts.get(2).unwrap_or(&"User description placeholder");

            markdown.push_str(&format!("| `{}` | `{}` | {} |\n", field, r#type, desc));
        }
        markdown.push('\n');
    }

    fs::write(&doc_path, markdown)
        .with_context(|| format!("Failed to write documentation file {}", doc_path.display()))?;

    println!("📝 Generated Markdown documentation: {}", doc_path.display());
    Ok(())
}

pub fn execute_doc(path_str: String, io_specs: Vec<String>) -> Result<()> {
    let manifest = PaynalManifest::load_from_dir(Path::new(".")).unwrap_or_default();
    let root_path = PathBuf::from(&manifest.root_dir);
    let collections_base = root_path.join("collections");
    let docs_base = root_path.join(&manifest.docs_dir);

    let mut target = PathBuf::from(&path_str);
    if !target.exists() {
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
        let files = find_yaml_files(&target);
        if files.is_empty() {
            println!("⚠️  No YAML files found in directory: {}", target.display());
            return Ok(());
        }
        for file in files {
            generate_doc_for_file(&file, &collections_base, &docs_base, &io_specs)?;
        }
    } else {
        generate_doc_for_file(&target, &collections_base, &docs_base, &io_specs)?;
    }

    Ok(())
}

fn append_request_doc(md: &mut String, req: &RequestSpec) {
    md.push_str(&format!("- **Method:** `{}`\n", req.method));
    md.push_str(&format!("- **URL:** `{}`\n", req.url));

    if let Some(auth) = &req.auth {
        match auth {
            crate::models::AuthSpec::Bearer { .. } => {
                md.push_str("- **Authentication:** `Bearer Token`\n");
            }
            crate::models::AuthSpec::Basic { username, .. } => {
                md.push_str(&format!("- **Authentication:** `Basic (user: {})`\n", username));
            }
            crate::models::AuthSpec::ApiKey { key, r#in, .. } => {
                md.push_str(&format!("- **Authentication:** `API Key ({} in {})`\n", key, r#in));
            }
        }
    }

    if let Some(params) = &req.params {
        if !params.is_empty() {
            md.push_str("\n**Query Parameters:**\n\n| Parameter | Value |\n| --- | --- |\n");
            for (k, v) in params {
                md.push_str(&format!("| `{}` | `{}` |\n", k, v));
            }
            md.push('\n');
        }
    }

    if !req.headers.is_empty() {
        md.push_str("\n**Headers:**\n\n| Header | Value |\n| --- | --- |\n");
        for (k, v) in &req.headers {
            md.push_str(&format!("| `{}` | `{}` |\n", k, v));
        }
        md.push('\n');
    }

    if let Some(form) = &req.form_data {
        if !form.is_empty() {
            md.push_str("\n**Form Data:**\n\n| Field | Value |\n| --- | --- |\n");
            for (k, v) in form {
                md.push_str(&format!("| `{}` | `{}` |\n", k, v));
            }
            md.push('\n');
        }
    } else if let Some(body) = &req.body {
        if !body.trim().is_empty() {
            md.push_str("\n**Body:**\n\n```");
            if body.trim_start().starts_with('{') || body.trim_start().starts_with('[') {
                md.push_str("json\n");
            } else {
                md.push('\n');
            }
            md.push_str(body.trim());
            md.push_str("\n```\n\n");
        }
    }
}

fn append_assert_doc(md: &mut String, assert: &AssertSpec) {
    md.push_str("\n**Assertions:**\n\n");
    if let Some(st) = assert.status {
        md.push_str(&format!("- Status: `{}`\n", st));
    }
    if let Some(sr) = &assert.status_range {
        md.push_str(&format!("- Status Range: `{}`\n", sr));
    }
    if let Some(sin) = &assert.status_in {
        md.push_str(&format!("- Status In: `{:?}`\n", sin));
    }
    if let Some(dur) = assert.max_duration_ms {
        md.push_str(&format!("- Max Duration: `{} ms`\n", dur));
    }
    if let Some(schema) = &assert.schema {
        md.push_str(&format!("- JSON Schema: `{}`\n", schema));
    }
    if let Some(hdrs) = &assert.headers {
        for (k, v) in hdrs {
            md.push_str(&format!("- Header `{}` equals `{}`\n", k, v));
        }
    }
    if let Some(json_map) = &assert.json {
        for (k, v) in json_map {
            md.push_str(&format!("- JSON `{}` equals `{}`\n", k, v));
        }
    }
    md.push('\n');
}
