use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub fn execute_remove(path_str: String, _clean: bool) -> Result<()> {
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
        anyhow::bail!("File not found at: {}", path_str);
    }

    fs::remove_file(&target)
        .with_context(|| format!("Failed to delete file {}", target.display()))?;
    println!("🗑️  Deleted file from disk: {}", target.display());

    Ok(())
}
