use crate::manifest::PaynalManifest;
use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::path::Path;

pub fn execute_init(name: Option<String>) -> Result<()> {
    let current_dir = env::current_dir().context("Failed to get current working directory")?;
    let manifest_path = current_dir.join(PaynalManifest::FILE_NAME);

    if manifest_path.exists() {
        println!("⚠️  Workspace already initialized: {}", manifest_path.display());
        return Ok(());
    }

    let project_name = name.unwrap_or_else(|| {
        current_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("PaynalWorkspace")
            .to_string()
    });

    let mut manifest = PaynalManifest::default();
    manifest.project_name = project_name.clone();

    // Save paynal.json
    manifest.save_to_dir(&current_dir)?;

    // Create default directories
    let dirs_to_create = [&manifest.out_dir, &manifest.docs_dir, "./collections"];
    for dir_path in &dirs_to_create {
        let path = Path::new(dir_path);
        if !path.exists() {
            fs::create_dir_all(path)
                .with_context(|| format!("Failed to create directory {}", dir_path))?;
        }
    }

    // Create default paynal.env
    let env_path = current_dir.join("paynal.env");
    if !env_path.exists() {
        let default_env_content = format!(
            "# Global Environment Variables for {}\nPAYNAL_ENV=development\nBASE_URL=https://api.example.com\n",
            project_name
        );
        fs::write(&env_path, default_env_content)
            .with_context(|| format!("Failed to create {}", env_path.display()))?;
    }

    println!("✅ Paynal workspace initialized successfully!");
    println!("   - Manifest: {}", manifest_path.display());
    println!("   - Environment: {}", env_path.display());
    println!("   - Output Dir: {}", manifest.out_dir);

    Ok(())
}
