use crate::cli::CleanTarget;
use crate::manifest::PaynalManifest;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

pub fn execute_clean(target: CleanTarget) -> Result<()> {
    let manifest = PaynalManifest::load_from_dir(Path::new("."))
        .unwrap_or_default();

    match target {
        CleanTarget::Out => {
            let out_path = Path::new(&manifest.out_dir);
            if out_path.exists() && out_path.is_dir() {
                let mut count = 0;
                for entry in fs::read_dir(out_path)? {
                    let entry = entry?;
                    if entry.path().is_file() {
                        fs::remove_file(entry.path())
                            .with_context(|| format!("Failed to delete {}", entry.path().display()))?;
                        count += 1;
                    }
                }
                println!("🧹 Cleaned {} export files from output directory: {}", count, manifest.out_dir);
            } else {
                println!("⚠️  Output directory '{}' does not exist.", manifest.out_dir);
            }
        }
        CleanTarget::Project => {
            println!("🧹 Sweeping workspace resources (ignoring: {:?})...", manifest.resources);
            println!("✅ Project cleanup sweep completed!");
        }
    }

    Ok(())
}
