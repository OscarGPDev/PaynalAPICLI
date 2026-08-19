use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
    #[serde(default = "default_body_types")]
    pub default_body_types: HashMap<String, String>,
}

fn default_validate_certificates() -> bool {
    true
}

fn default_body_types() -> HashMap<String, String> {
    let mut map = HashMap::new();
    map.insert("GET".to_string(), "none".to_string());
    map.insert("POST".to_string(), "json".to_string());
    map.insert("PUT".to_string(), "json".to_string());
    map.insert("PATCH".to_string(), "json".to_string());
    map.insert("DELETE".to_string(), "none".to_string());
    map.insert("HEAD".to_string(), "none".to_string());
    map.insert("OPTIONS".to_string(), "none".to_string());
    map
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
            default_body_types: default_body_types(),
        }
    }
}

impl PaynalManifest {
    pub const FILE_NAME: &'static str = "paynal.json";

    pub fn get_default_body_type(&self, method: &str) -> String {
        let method_upper = method.to_uppercase();
        if let Some(bt) = self.default_body_types.get(&method_upper) {
            return bt.to_lowercase();
        }
        for (k, v) in &self.default_body_types {
            if k.eq_ignore_ascii_case(method) {
                return v.to_lowercase();
            }
        }
        match method_upper.as_str() {
            "POST" | "PUT" | "PATCH" => "json".to_string(),
            _ => "none".to_string(),
        }
    }

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
