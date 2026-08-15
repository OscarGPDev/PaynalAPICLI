use crate::models::PaynalFile;
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub fn execute_doc(path_str: String, io_specs: Vec<String>) -> Result<()> {
    let mut target = PathBuf::from(&path_str);
    if !target.exists() {
        let collections_base = Path::new("collections");
        let relative_yaml = if path_str.ends_with(".yaml") || path_str.ends_with(".yml") {
            path_str.clone()
        } else {
            format!("{}.yaml", path_str)
        };
        target = collections_base.join(&relative_yaml);
    }

    if !target.exists() {
        anyhow::bail!("Target path not found: {}", path_str);
    }

    let content = fs::read_to_string(&target)
        .with_context(|| format!("Failed to read {}", target.display()))?;

    let paynal_file: PaynalFile = serde_yaml::from_str(&content)
        .with_context(|| format!("Failed to parse YAML {}", target.display()))?;

    let doc_path = target.with_extension("md");

    let mut markdown = format!(
        "# Documentation: {}\n\n", paynal_file.name
    );

    if let Some(desc) = &paynal_file.description {
        markdown.push_str(&format!("**Description:** {}\n\n", desc));
    }

    if paynal_file.is_routine() {
        markdown.push_str("## Routine Steps\n\n");
        for (idx, step) in paynal_file.steps.iter().enumerate() {
            let step_title = step.name.as_deref().unwrap_or(&step.id);
            markdown.push_str(&format!(
                "### {}. {}\n- **Method:** `{}`\n- **URL:** `{}`\n\n",
                idx + 1,
                step_title,
                step.request.method,
                step.request.url
            ));
        }
    } else if let Some(req) = &paynal_file.request {
        markdown.push_str("## Request Details\n\n");
        markdown.push_str(&format!("- **Method:** `{}`\n", req.method));
        markdown.push_str(&format!("- **URL:** `{}`\n\n", req.url));
    }

    if !io_specs.is_empty() {
        markdown.push_str("## Input & Output Specification\n\n");
        markdown.push_str("| Field | Type | Description |\n");
        markdown.push_str("| --- | --- | --- |\n");

        for io in &io_specs {
            let parts: Vec<&str> = io.splitn(3, ':').collect();
            let field = parts.get(0).unwrap_or(&"field");
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
