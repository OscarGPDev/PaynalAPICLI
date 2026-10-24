use crate::manifest::PaynalManifest;
use crate::models::PaynalFile;
use anyhow::{Context, Result};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GlobalWorkspaceEntry {
    pub name: String,
    pub path: String,
    pub description: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct GlobalRegistry {
    #[serde(default)]
    pub workspaces: Vec<GlobalWorkspaceEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredRoute {
    pub handle: String,      // e.g. "@pagos/auth/login"
    pub workspace: String,   // e.g. "pagos"
    pub path: String,        // e.g. "auth/login"
    pub name: String,        // PaynalFile.name
    pub kind: String,        // "request" or "routine"
    pub method: String,      // e.g. "POST" or "ROUTINE (3 steps)"
    pub url: String,         // request URL or first step URL
    pub description: String, // PaynalFile.description or workspace description
}

pub fn get_registry_path() -> Result<PathBuf> {
    if let Ok(val) = std::env::var("PAYNAL_GLOBAL_REGISTRY_PATH") {
        let trimmed = val.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }

    #[cfg(windows)]
    {
        let appdata = std::env::var("APPDATA")
            .unwrap_or_else(|_| "C:\\Users\\Default\\AppData\\Roaming".to_string());
        Ok(PathBuf::from(appdata).join("paynal").join("registry.json"))
    }

    #[cfg(not(windows))]
    {
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            let trimmed = xdg.trim();
            if !trimmed.is_empty() {
                return Ok(PathBuf::from(trimmed).join("paynal").join("registry.json"));
            }
        }
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        Ok(PathBuf::from(home).join(".config").join("paynal").join("registry.json"))
    }
}

impl GlobalRegistry {
    pub fn load() -> Result<Self> {
        let path = get_registry_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read registry file: {}", path.display()))?;
        if content.trim().is_empty() {
            return Ok(Self::default());
        }
        let reg: Self = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse registry JSON at: {}", path.display()))?;
        Ok(reg)
    }

    pub fn save(&self) -> Result<()> {
        let path = get_registry_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create registry directory: {}", parent.display()))?;
        }
        let json = serde_json::to_string_pretty(self)
            .context("Failed to serialize registry to JSON")?;
        fs::write(&path, json)
            .with_context(|| format!("Failed to write registry file: {}", path.display()))?;
        Ok(())
    }

    pub fn find_workspace(&self, name: &str) -> Option<&GlobalWorkspaceEntry> {
        let clean = name.strip_prefix('@').unwrap_or(name).trim();
        self.workspaces.iter().find(|w| w.name.eq_ignore_ascii_case(clean))
    }

    pub fn add_workspace(&mut self, entry: GlobalWorkspaceEntry) {
        let clean = entry.name.strip_prefix('@').unwrap_or(&entry.name).trim();
        if let Some(pos) = self.workspaces.iter().position(|w| w.name.eq_ignore_ascii_case(clean)) {
            self.workspaces[pos] = entry;
        } else {
            self.workspaces.push(entry);
        }
    }

    pub fn remove_workspace(&mut self, name: &str) -> bool {
        let clean = name.strip_prefix('@').unwrap_or(name).trim();
        if let Some(pos) = self.workspaces.iter().position(|w| w.name.eq_ignore_ascii_case(clean)) {
            self.workspaces.remove(pos);
            true
        } else {
            false
        }
    }
}

pub fn find_yaml_files_recursive(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                files.extend(find_yaml_files_recursive(&path));
            } else if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if ext == "yaml" || ext == "yml" {
                        files.push(path);
                    }
                }
            }
        }
    }
    files.sort();
    files
}

#[allow(dead_code)]
pub fn resolve_cross_workspace_path(raw_target: &str) -> Result<(GlobalWorkspaceEntry, String)> {
    let clean = raw_target.strip_prefix('@').unwrap_or(raw_target).trim();
    let (ws_name, subpath) = match clean.split_once('/') {
        Some((w, s)) => (w, s),
        None => (clean, ""),
    };

    if !subpath.is_empty() {
        let sub_path = Path::new(subpath);
        if sub_path.is_absolute()
            || sub_path.components().any(|c| {
                matches!(
                    c,
                    std::path::Component::ParentDir
                        | std::path::Component::RootDir
                        | std::path::Component::Prefix(_)
                )
            })
        {
            anyhow::bail!(
                "Invalid cross-workspace path '{}': path traversal ('..') or absolute subpaths are not allowed.",
                raw_target
            );
        }
    }

    let registry = GlobalRegistry::load()?;
    let entry = registry
        .find_workspace(ws_name)
        .cloned()
        .with_context(|| format!("Workspace '{}' not found in global registry", ws_name))?;

    Ok((entry, subpath.to_string()))
}

pub fn collect_discovered_routes(registry: &GlobalRegistry) -> Vec<DiscoveredRoute> {
    let mut routes = Vec::new();
    for ws in &registry.workspaces {
        let ws_path = Path::new(&ws.path);
        let collections_dir = ws_path.join("collections");
        if !collections_dir.exists() {
            continue;
        }

        let files = find_yaml_files_recursive(&collections_dir);
        for file in files {
            let Ok(rel_path) = file.strip_prefix(&collections_dir) else {
                continue;
            };
            let rel_str = rel_path.to_string_lossy().replace('\\', "/");
            let clean_subpath = if let Some(stripped) = rel_str.strip_suffix(".yaml") {
                stripped
            } else if let Some(stripped) = rel_str.strip_suffix(".yml") {
                stripped
            } else {
                &rel_str
            };

            let Ok(content) = fs::read_to_string(&file) else {
                continue;
            };
            let Ok(paynal_file) = serde_yaml::from_str::<PaynalFile>(&content) else {
                continue;
            };

            let is_routine = paynal_file.is_routine();
            let kind = if is_routine { "routine".to_string() } else { "request".to_string() };

            let method = if is_routine {
                format!("ROUTINE ({} steps)", paynal_file.steps.len())
            } else {
                paynal_file.request.as_ref().map(|r| r.method.clone()).unwrap_or_else(|| "GET".to_string())
            };

            let url = if is_routine {
                paynal_file.steps.first().map(|s| s.request.url.clone()).unwrap_or_default()
            } else {
                paynal_file.request.as_ref().map(|r| r.url.clone()).unwrap_or_default()
            };

            let description = paynal_file.description.clone().unwrap_or_else(|| ws.description.clone());

            routes.push(DiscoveredRoute {
                handle: format!("@{}/{}", ws.name, clean_subpath),
                workspace: ws.name.clone(),
                path: clean_subpath.to_string(),
                name: paynal_file.name,
                kind,
                method,
                url,
                description,
            });
        }
    }
    routes
}

pub fn execute_add(name: Option<String>, path: Option<String>, description: String) -> Result<()> {
    let target_path = if let Some(p) = path {
        PathBuf::from(p)
    } else {
        std::env::current_dir().context("Failed to get current directory")?
    };

    let canonical = target_path.canonicalize().unwrap_or(target_path);
    if !canonical.exists() {
        anyhow::bail!("Target path does not exist: {}", canonical.display());
    }

    let resolved_name = if let Some(n) = name {
        n.strip_prefix('@').unwrap_or(&n).trim().to_string()
    } else {
        let manifest = PaynalManifest::load_from_dir(&canonical).unwrap_or_default();
        if !manifest.project_name.is_empty() {
            manifest.project_name
        } else {
            canonical
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| "workspace".to_string())
        }
    };

    if resolved_name.is_empty() {
        anyhow::bail!("Workspace name cannot be empty");
    }

    let mut registry = GlobalRegistry::load()?;
    let path_str = canonical.to_string_lossy().to_string();

    registry.add_workspace(GlobalWorkspaceEntry {
        name: resolved_name.clone(),
        path: path_str.clone(),
        description: description.clone(),
    });
    registry.save()?;

    println!(
        "{} Registered global workspace '{}' at {}",
        "✓".green().bold(),
        resolved_name.cyan().bold(),
        path_str.dimmed()
    );
    Ok(())
}

pub fn execute_list(json: bool) -> Result<()> {
    let registry = GlobalRegistry::load()?;

    if json {
        println!("{}", serde_json::to_string_pretty(&registry.workspaces)?);
        return Ok(());
    }

    if registry.workspaces.is_empty() {
        println!("No workspaces registered yet. Run 'paynal global add' in any Paynal workspace to register it.");
        return Ok(());
    }

    println!(
        "\n{:<18} {:<10} {:<12} {:<32} {}",
        "NAME".bold(),
        "ROUTINES".bold(),
        "STATUS".bold(),
        "PATH".bold(),
        "DESCRIPTION".bold()
    );
    println!("{}", "─".repeat(95).dimmed());

    for ws in &registry.workspaces {
        let ws_path = PathBuf::from(&ws.path);
        let exists = ws_path.exists();
        let status_str = if exists {
            "✓".green().to_string()
        } else {
            "✗ missing".red().to_string()
        };

        let routine_count = if exists {
            let coll = ws_path.join("collections");
            if coll.exists() {
                find_yaml_files_recursive(&coll).len()
            } else {
                0
            }
        } else {
            0
        };

        println!(
            "{:<18} {:<10} {:<12} {:<32} {}",
            ws.name.cyan().bold(),
            routine_count,
            status_str,
            ws.path.dimmed(),
            ws.description
        );
    }
    println!();
    Ok(())
}

pub fn execute_discover(query: Option<String>, json: bool) -> Result<()> {
    let registry = GlobalRegistry::load()?;
    let mut routes = collect_discovered_routes(&registry);

    if let Some(q) = query {
        let q_lower = q.to_lowercase();
        routes.retain(|r| {
            r.handle.to_lowercase().contains(&q_lower)
                || r.workspace.to_lowercase().contains(&q_lower)
                || r.path.to_lowercase().contains(&q_lower)
                || r.name.to_lowercase().contains(&q_lower)
                || r.method.to_lowercase().contains(&q_lower)
                || r.url.to_lowercase().contains(&q_lower)
                || r.description.to_lowercase().contains(&q_lower)
        });
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&routes)?);
        return Ok(());
    }

    if routes.is_empty() {
        println!("No routes found across registered workspaces.");
        return Ok(());
    }

    println!(
        "\n{:<28} {:<18} {:<35} {}",
        "HANDLE (@workspace/path)".bold(),
        "METHOD".bold(),
        "URL".bold(),
        "NAME / DESCRIPTION".bold()
    );
    println!("{}", "─".repeat(105).dimmed());

    for r in &routes {
        let display_desc = if !r.description.is_empty() {
            format!("{} ({})", r.name, r.description)
        } else {
            r.name.clone()
        };

        println!(
            "{:<28} {:<18} {:<35} {}",
            r.handle.cyan().bold(),
            r.method.yellow(),
            r.url.dimmed(),
            display_desc
        );
    }
    println!();
    Ok(())
}

pub fn execute_show(target: String) -> Result<()> {
    let clean = target.strip_prefix('@').unwrap_or(&target).trim();
    let (ws_name, subpath) = clean
        .split_once('/')
        .context("Invalid target format. Expected '@workspace/path' or 'workspace/path'")?;

    let sub_path = Path::new(subpath);
    if sub_path.is_absolute()
        || sub_path.components().any(|c| {
            matches!(
                c,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        anyhow::bail!(
            "Invalid route path '{}': path traversal ('..') or absolute paths are not allowed.",
            subpath
        );
    }

    let registry = GlobalRegistry::load()?;
    let entry = registry
        .find_workspace(ws_name)
        .with_context(|| format!("Workspace '{}' not found in global registry", ws_name))?;

    let ws_path = PathBuf::from(&entry.path);
    if !ws_path.exists() {
        anyhow::bail!("Workspace directory does not exist: {}", entry.path);
    }

    let collections_dir = ws_path.join("collections");
    let mut file_path = collections_dir.join(subpath);
    if !file_path.exists() {
        let candidate_yaml = collections_dir.join(format!("{}.yaml", subpath));
        let candidate_yml = collections_dir.join(format!("{}.yml", subpath));
        if candidate_yaml.exists() {
            file_path = candidate_yaml;
        } else if candidate_yml.exists() {
            file_path = candidate_yml;
        } else {
            anyhow::bail!(
                "Route file not found: checked {} and {}.yaml in workspace '{}'",
                subpath,
                subpath,
                ws_name
            );
        }
    }

    let canon_collections = collections_dir
        .canonicalize()
        .with_context(|| format!("Failed to canonicalize collections directory '{}'", collections_dir.display()))?;
    let canon_file = file_path
        .canonicalize()
        .with_context(|| format!("Failed to canonicalize routine file '{}'", file_path.display()))?;
    if !canon_file.starts_with(&canon_collections) {
        anyhow::bail!(
            "Access denied: '{}' resolves outside the workspace collections directory.",
            subpath
        );
    }

    let content = fs::read_to_string(&file_path)
        .with_context(|| format!("Failed to read file: {}", file_path.display()))?;

    if let Ok(paynal_file) = serde_yaml::from_str::<PaynalFile>(&content) {
        println!("🏷️  Handle:      @{}/{}", ws_name, subpath);
        println!("🌐 Workspace:   {} ({})", entry.name, entry.path);
        println!("📄 File:        {}", file_path.display());
        println!("📝 Name:        {}", paynal_file.name);
        if let Some(desc) = &paynal_file.description {
            println!("💬 Description: {}", desc);
        }
        if paynal_file.is_routine() {
            println!("🔄 Type:        ROUTINE ({} steps)", paynal_file.steps.len());
        } else if let Some(req) = &paynal_file.request {
            println!("🚀 Type:        REQUEST [{} {}]", req.method, req.url);
        }
    }

    println!("\n{}", "─".repeat(60).dimmed());
    println!("{}", content.trim_end());
    println!("{}\n", "─".repeat(60).dimmed());
    Ok(())
}

pub fn execute_remove(name: String) -> Result<()> {
    let mut registry = GlobalRegistry::load()?;
    let clean = name.strip_prefix('@').unwrap_or(&name).trim();

    if registry.remove_workspace(clean) {
        registry.save()?;
        println!("{} Removed global workspace '{}'", "✓".green().bold(), clean);
        Ok(())
    } else {
        anyhow::bail!("Workspace '{}' not found in global registry", clean);
    }
}
