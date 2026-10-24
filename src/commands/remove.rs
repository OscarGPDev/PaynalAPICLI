use anyhow::{Context, Result};
use std::fs;
use std::path::{Component, Path};

pub fn execute_remove(path_str: String, _clean: bool) -> Result<()> {
    let raw_path = Path::new(&path_str);
    if raw_path.is_absolute()
        || raw_path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::RootDir | Component::Prefix(_)))
    {
        anyhow::bail!(
            "Invalid path '{}': path traversal ('..') or absolute paths are not allowed.",
            path_str
        );
    }

    let stripped = path_str
        .strip_prefix("collections/")
        .or_else(|| path_str.strip_prefix("collections\\"))
        .unwrap_or(&path_str);

    let stripped_path = Path::new(stripped);
    if stripped_path.is_absolute()
        || stripped_path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::RootDir | Component::Prefix(_)))
    {
        anyhow::bail!(
            "Invalid path '{}': path traversal ('..') or absolute paths are not allowed.",
            path_str
        );
    }

    let collections_base = Path::new("collections");
    let relative_yaml = if stripped.ends_with(".yaml") || stripped.ends_with(".yml") {
        stripped.to_string()
    } else {
        format!("{}.yaml", stripped)
    };

    let mut target_path = collections_base.join(&relative_yaml);
    if !target_path.exists() {
        let alt_path = collections_base.join(stripped);
        if alt_path.is_file() {
            target_path = alt_path;
        }
    }

    if !target_path.exists() {
        anyhow::bail!("File not found at: {}", path_str);
    }

    if collections_base.exists() && target_path.exists() {
        let canonical_base = collections_base
            .canonicalize()
            .context("Failed to canonicalize collections base directory")?;
        let canonical_target = target_path
            .canonicalize()
            .context("Failed to canonicalize target path")?;
        if !canonical_target.starts_with(&canonical_base) {
            anyhow::bail!(
                "Invalid path '{}': path traversal ('..') or absolute paths are not allowed.",
                path_str
            );
        }
    }

    fs::remove_file(&target_path)
        .with_context(|| format!("Failed to delete file {}", target_path.display()))?;
    println!("🗑️  Deleted file from disk: {}", target_path.display());

    Ok(())
}

