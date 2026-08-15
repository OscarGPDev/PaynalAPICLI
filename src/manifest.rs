use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{Context, Result};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PaynalManifest {
    pub project_name: String,
    pub version: String,
    pub root_dir: String,
    pub out_dir: String,
    pub docs_dir: String,
    pub resources: Vec<String>,
    pub max_threads: String,
    pub default_export_format: String,
    #[serde(default = "default_validate_certificates")]
    pub validate_certificates: bool,
    #[serde(default)]
    pub proxy: Option<String>,
}

fn default_validate_certificates() -> bool {
    true
}

impl Default for PaynalManifest {
    fn default() -> Self {
        Self {
            project_name: "Paynalapicli".to_string(),
            version: "0.1.0".to_string(),
            root_dir: ".".to_string(),
            out_dir: "./output".to_string(),
            docs_dir: "./docs".to_string(),
            resources: vec![
                "node_modules".to_string(),
                ".git".to_string(),
                "target".to_string(),
                "vendor".to_string(),
            ],
            max_threads: "CPUMAX".to_string(),
            default_export_format: "curl".to_string(),
            validate_certificates: true,
            proxy: None,
        }
    }
}

impl PaynalManifest {
    pub const FILE_NAME: &'static str = "paynal.json";

    #[allow(dead_code)]
    pub fn load_from_dir(dir: &Path) -> Result<Self> {
        let manifest_path = dir.join(Self::FILE_NAME);
        let content = fs::read_to_string(&manifest_path)
            .with_context(|| format!("Failed to read {}", manifest_path.display()))?;
        let manifest: PaynalManifest = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse {}", manifest_path.display()))?;
        Ok(manifest)
    }

    pub fn save_to_dir(&self, dir: &Path) -> Result<PathBuf> {
        let manifest_path = dir.join(Self::FILE_NAME);
        let json_content = serde_json::to_string_pretty(self)?;
        fs::write(&manifest_path, json_content)
            .with_context(|| format!("Failed to write {}", manifest_path.display()))?;
        Ok(manifest_path)
    }
}
