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
            let collections_dir = Path::new("collections");
            let mut files_cleaned = 0;
            let mut dirs_cleaned = 0;
            if collections_dir.exists() {
                sweep_dir(collections_dir, &manifest.resources, &mut files_cleaned, &mut dirs_cleaned)?;
            }
            println!("🧹 Swept collections: removed {} temporary files and {} empty directories.", files_cleaned, dirs_cleaned);
        }
    }

    Ok(())
}

fn sweep_dir(dir: &Path, ignore: &[String], files_removed: &mut usize, dirs_removed: &mut usize) -> Result<()> {
    if !dir.exists() || !dir.is_dir() {
        return Ok(());
    }

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_string();

        if ignore.iter().any(|ig| ig == &file_name) {
            continue;
        }

        if path.is_dir() {
            sweep_dir(&path, ignore, files_removed, dirs_removed)?;
            if let Ok(mut rd) = fs::read_dir(&path) {
                if rd.next().is_none() {
                    let _ = fs::remove_dir(&path);
                    *dirs_removed += 1;
                }
            }
        } else if path.is_file() {
            let should_clean = file_name.ends_with(".tmp")
                || file_name.ends_with(".bak")
                || file_name.ends_with(".swp")
                || file_name.ends_with('~');

            if should_clean {
                if fs::remove_file(&path).is_ok() {
                    *files_removed += 1;
                }
            }
        }
    }

    Ok(())
}
