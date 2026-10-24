use anyhow::{Context, Result};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    name: Option<String>,
    html_url: String,
    body: Option<String>,
    assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateCheckCache {
    pub last_checked_secs: u64,
    pub latest_version: String,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct SimpleSemVer {
    major: u64,
    minor: u64,
    patch: u64,
    is_prerelease: bool,
}

fn parse_semver(s: &str) -> Option<SimpleSemVer> {
    let s = s.trim().trim_start_matches(|c| c == 'v' || c == 'V');
    let (ver_part, pre_part) = match s.split_once('-') {
        Some((v, p)) => (v, Some(p)),
        None => (s, None),
    };
    let parts: Vec<&str> = ver_part.split('.').collect();
    if parts.is_empty() {
        return None;
    }
    let major = parts[0].parse::<u64>().ok()?;
    let minor = if parts.len() > 1 {
        parts[1].parse::<u64>().ok()?
    } else {
        0
    };
    let patch = if parts.len() > 2 {
        parts[2].parse::<u64>().ok()?
    } else {
        0
    };
    Some(SimpleSemVer {
        major,
        minor,
        patch,
        is_prerelease: pre_part.is_some(),
    })
}

pub fn is_newer_version(current: &str, latest_tag: &str) -> bool {
    let cur = match parse_semver(current) {
        Some(v) => v,
        None => return false,
    };
    let lat = match parse_semver(latest_tag) {
        Some(v) => v,
        None => return false,
    };

    if (lat.major, lat.minor, lat.patch) > (cur.major, cur.minor, cur.patch) {
        true
    } else if (lat.major, lat.minor, lat.patch) == (cur.major, cur.minor, cur.patch) {
        cur.is_prerelease && !lat.is_prerelease
    } else {
        false
    }
}

pub fn get_target_asset_name() -> Result<&'static str> {
    if Path::new("/.flatpak-info").exists() || std::env::var("FLATPAK_ID").is_ok() {
        return Ok("paynal.flatpak");
    }
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Ok("paynal-x86_64-unknown-linux-gnu.tar.gz"),
        ("macos", "x86_64") => Ok("paynal-x86_64-apple-darwin.tar.gz"),
        ("macos", "aarch64") => Ok("paynal-aarch64-apple-darwin.tar.gz"),
        ("windows", "x86_64") => Ok("paynal-x86_64-pc-windows-msvc.zip"),
        (os, arch) => anyhow::bail!(
            "Unsupported platform/architecture for self-update: {}-{}",
            os,
            arch
        ),
    }
}

fn get_cache_file_path() -> Option<PathBuf> {
    #[cfg(windows)]
    let base_dir = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);

    #[cfg(not(windows))]
    let base_dir = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")));

    base_dir.map(|p| p.join("paynal").join("update_check.json"))
}

pub fn save_update_cache(version: &str) {
    if let Some(path) = get_cache_file_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let cache = UpdateCheckCache {
            last_checked_secs: now_secs,
            latest_version: version.to_string(),
        };
        if let Ok(json_str) = serde_json::to_string_pretty(&cache) {
            let _ = std::fs::write(path, json_str);
        }
    }
}

pub async fn check_update_notice_cached() -> Option<String> {
    let current_ver = env!("CARGO_PKG_VERSION");
    let cache_path = get_cache_file_path()?;

    let now_secs = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();

    if let Ok(content) = std::fs::read_to_string(&cache_path) {
        if let Ok(cache) = serde_json::from_str::<UpdateCheckCache>(&content) {
            if now_secs.saturating_sub(cache.last_checked_secs) < 86_400 {
                if is_newer_version(current_ver, &cache.latest_version) {
                    return Some(cache.latest_version);
                } else {
                    return None;
                }
            }
        }
    }

    // Cache missing or expired: run fast non-intrusive network check (1500ms timeout)
    let client = reqwest::Client::builder()
        .user_agent(format!("paynal/{}", current_ver))
        .timeout(Duration::from_millis(1500))
        .build()
        .ok()?;

    let resp = client
        .get("https://api.github.com/repos/OscarGPDev/PaynalAPICLI/releases/latest")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let release: GithubRelease = resp.json().await.ok()?;
    let latest_tag = release.tag_name;
    let clean_latest = latest_tag
        .trim()
        .trim_start_matches(|c| c == 'v' || c == 'V')
        .to_string();

    save_update_cache(&clean_latest);

    if is_newer_version(current_ver, &clean_latest) {
        Some(clean_latest)
    } else {
        None
    }
}

pub const SKILL_MD: &str = include_str!("../../.agents/skills/paynal-api-testing/SKILL.md");
pub const REF_YAML_MD: &str =
    include_str!("../../.agents/skills/paynal-api-testing/references/yaml-schema-and-assertions.md");
pub const REF_MULTI_WS_MCP_MD: &str =
    include_str!("../../.agents/skills/paynal-api-testing/references/multi-workspace-and-mcp.md");
pub const REF_IMPORT_EXPORT_CI_MD: &str =
    include_str!("../../.agents/skills/paynal-api-testing/references/import-export-and-ci.md");

pub fn write_embedded_skill_to_dir(skill_dir: &Path) -> Result<()> {
    let refs_dir = skill_dir.join("references");
    std::fs::create_dir_all(&refs_dir)
        .with_context(|| format!("Failed to create skill directory: {}", refs_dir.display()))?;

    let skill_md_path = skill_dir.join("SKILL.md");
    std::fs::write(&skill_md_path, SKILL_MD)
        .with_context(|| format!("Failed to write {}", skill_md_path.display()))?;

    let ref_yaml_path = refs_dir.join("yaml-schema-and-assertions.md");
    std::fs::write(&ref_yaml_path, REF_YAML_MD)
        .with_context(|| format!("Failed to write {}", ref_yaml_path.display()))?;

    let ref_mcp_path = refs_dir.join("multi-workspace-and-mcp.md");
    std::fs::write(&ref_mcp_path, REF_MULTI_WS_MCP_MD)
        .with_context(|| format!("Failed to write {}", ref_mcp_path.display()))?;

    let ref_ci_path = refs_dir.join("import-export-and-ci.md");
    std::fs::write(&ref_ci_path, REF_IMPORT_EXPORT_CI_MD)
        .with_context(|| format!("Failed to write {}", ref_ci_path.display()))?;

    Ok(())
}

pub fn install_skill_in_workspace(workspace_dir: &Path) -> Result<PathBuf> {
    let skill_dir = workspace_dir
        .join(".agents")
        .join("skills")
        .join("paynal-api-testing");
    write_embedded_skill_to_dir(&skill_dir)?;
    Ok(skill_dir)
}

pub fn sync_installed_skills() -> usize {
    let mut targets: Vec<PathBuf> = Vec::new();

    // 1. Current working directory
    if let Ok(cwd) = std::env::current_dir() {
        let skill_path = cwd.join(".agents").join("skills").join("paynal-api-testing");
        if skill_path.exists() || cwd.join("paynal.json").exists() {
            targets.push(skill_path);
        }
        let singular_agent_path = cwd.join(".agent").join("skills").join("paynal-api-testing");
        if singular_agent_path.exists() {
            targets.push(singular_agent_path);
        }
    }

    // 2. Registered global workspaces
    if let Ok(registry) = crate::commands::global::GlobalRegistry::load() {
        for ws in registry.workspaces {
            let ws_path = PathBuf::from(&ws.path);
            if ws_path.exists() {
                let skill_path = ws_path
                    .join(".agents")
                    .join("skills")
                    .join("paynal-api-testing");
                if skill_path.exists() || ws_path.join("paynal.json").exists() {
                    targets.push(skill_path);
                }
            }
        }
    }

    // 3. User global agent skills directory
    #[cfg(windows)]
    let global_skill_dir = std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .map(|p| {
            p.join(".gemini")
                .join("config")
                .join("skills")
                .join("paynal-api-testing")
        });
    #[cfg(not(windows))]
    let global_skill_dir = std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|p| {
            p.join(".gemini")
                .join("config")
                .join("skills")
                .join("paynal-api-testing")
        });

    if let Some(user_skill_dir) = global_skill_dir {
        if user_skill_dir.exists() {
            targets.push(user_skill_dir);
        }
    }

    // Deduplicate target paths
    let mut unique_targets: Vec<PathBuf> = Vec::new();
    for target in targets {
        let is_dup = unique_targets.iter().any(|existing| {
            if existing == &target {
                return true;
            }
            if let (Ok(c1), Ok(c2)) = (existing.canonicalize(), target.canonicalize()) {
                c1 == c2
            } else {
                false
            }
        });
        if !is_dup {
            unique_targets.push(target);
        }
    }

    let mut success_count = 0;
    for target in unique_targets {
        if write_embedded_skill_to_dir(&target).is_ok() {
            success_count += 1;
        }
    }

    success_count
}

pub async fn execute_update(check: bool, yes: bool, sync_skill: bool) -> Result<()> {
    if sync_skill {
        let mut updated = sync_installed_skills();
        if updated == 0 {
            if let Ok(cwd) = std::env::current_dir() {
                if cwd.join(".agents").exists() || cwd.join("paynal.json").exists() {
                    if install_skill_in_workspace(&cwd).is_ok() {
                        updated = 1;
                    }
                }
            }
        }
        if updated > 0 {
            println!(
                "{}",
                format!(
                    "✅ Synchronized 'paynal-api-testing' skill in {} location(s).",
                    updated
                )
                .green()
                .bold()
            );
        } else {
            println!("💡 No existing skill installations or Paynal workspaces found to synchronize.");
        }
        return Ok(());
    }

    let current_ver = env!("CARGO_PKG_VERSION");
    println!("🔍 Checking for updates... (current: v{})", current_ver);

    let client = reqwest::Client::builder()
        .user_agent(format!("paynal/{}", current_ver))
        .build()
        .context("Failed to build HTTP client")?;

    let resp = client
        .get("https://api.github.com/repos/OscarGPDev/PaynalAPICLI/releases/latest")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .context("Failed to query GitHub Releases API")?;

    if !resp.status().is_success() {
        anyhow::bail!(
            "GitHub API returned HTTP {}: {}",
            resp.status(),
            resp.text().await.unwrap_or_default()
        );
    }

    let release: GithubRelease = resp
        .json()
        .await
        .context("Failed to parse GitHub release metadata")?;

    let latest_tag = &release.tag_name;
    let clean_latest = latest_tag
        .trim()
        .trim_start_matches(|c| c == 'v' || c == 'V');

    if !is_newer_version(current_ver, latest_tag) {
        println!(
            "{}",
            format!("✅ Paynal is already up to date (v{}).", current_ver)
                .green()
                .bold()
        );
        if !check {
            let count = sync_installed_skills();
            if count > 0 {
                println!(
                    "{}",
                    format!(
                        "🔄 Synchronized AI agent skill (paynal-api-testing) in {} workspace(s).",
                        count
                    )
                    .cyan()
                );
            }
        }
        save_update_cache(clean_latest);
        return Ok(());
    }

    let target_asset_name = get_target_asset_name()?;
    let title = release.name.as_deref().unwrap_or(latest_tag);

    println!();
    println!(
        "{}",
        "┌────────────────────────────────────────────────────────┐".cyan()
    );
    println!(
        "│ {} │",
        format!("{:^54}", "🎉 NEW PAYNAL RELEASE AVAILABLE!")
            .yellow()
            .bold()
    );
    println!(
        "{}",
        "├────────────────────────────────────────────────────────┤".cyan()
    );
    println!(
        "│  Version:     {} → {}",
        format!("v{}", current_ver).red(),
        format!("v{}", clean_latest).green().bold()
    );
    println!("│  Release:     {}", title.bold());
    println!("│  Package:     {}", target_asset_name.cyan());
    println!("│  URL:         {}", release.html_url.dimmed());
    if let Some(body) = &release.body {
        let preview_lines: Vec<&str> = body.lines().take(4).collect();
        if !preview_lines.is_empty() {
            println!(
                "{}",
                "├────────────────────────────────────────────────────────┤".cyan()
            );
            println!("│  Notes:");
            for line in preview_lines {
                println!("│    {}", line.dimmed());
            }
        }
    }
    println!(
        "{}",
        "└────────────────────────────────────────────────────────┘".cyan()
    );
    println!();

    if check {
        save_update_cache(clean_latest);
        return Ok(());
    }

    if !yes {
        use std::io::Write;
        print!("Do you want to update to v{} now? [y/N]: ", clean_latest);
        std::io::stdout()
            .flush()
            .context("Failed to flush stdout")?;

        let mut input = String::new();
        std::io::stdin()
            .read_line(&mut input)
            .context("Failed to read user input")?;
        let trimmed = input.trim().to_lowercase();
        if trimmed != "y" && trimmed != "yes" {
            println!("Update cancelled.");
            return Ok(());
        }
    }

    let asset = release
        .assets
        .iter()
        .find(|a| a.name == target_asset_name)
        .with_context(|| {
            format!(
                "Release {} does not contain asset '{}'",
                latest_tag, target_asset_name
            )
        })?;

    // 1. Flatpak environment installation
    if Path::new("/.flatpak-info").exists() || std::env::var("FLATPAK_ID").is_ok() {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let tmp_flatpak = std::env::temp_dir().join(format!("paynal_update_{}_{}.flatpak", std::process::id(), ts));
        println!("📥 Downloading {}...", asset.name);
        let bytes = client
            .get(&asset.browser_download_url)
            .send()
            .await
            .context("Failed downloading Flatpak bundle")?
            .bytes()
            .await
            .context("Failed reading Flatpak download bytes")?;

        std::fs::write(&tmp_flatpak, &bytes).context("Failed writing Flatpak bundle to temp dir")?;

        println!("📦 Installing Flatpak update...");
        let spawn_res = std::process::Command::new("flatpak-spawn")
            .args([
                "--host",
                "flatpak",
                "install",
                "--user",
                "-y",
                tmp_flatpak.to_str().unwrap_or("paynal.flatpak"),
            ])
            .status();

        match spawn_res {
            Ok(s) if s.success() => {
                save_update_cache(clean_latest);
                println!(
                    "{}",
                    format!("🎉 Successfully updated Paynal to v{}!", clean_latest)
                        .green()
                        .bold()
                );
                let _ = sync_installed_skills();
                return Ok(());
            }
            _ => {
                println!(
                    "💡 Standalone Flatpak bundle downloaded to: {}",
                    tmp_flatpak.display()
                );
                println!(
                    "To complete installation, run on host:\n  flatpak install --user {}",
                    tmp_flatpak.display()
                );
                return Ok(());
            }
        }
    }

    // 2. Native Binary installation
    let current_exe =
        std::env::current_exe().context("Failed to determine current executable path")?;
    let exe_dir = current_exe.parent().with_context(|| {
        format!(
            "Failed to determine directory of {}",
            current_exe.display()
        )
    })?;

    // Check directory write permissions
    let test_file = exe_dir.join(format!(".paynal_write_test_{}", std::process::id()));
    match std::fs::File::create(&test_file) {
        Ok(_) => {
            let _ = std::fs::remove_file(&test_file);
        }
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            anyhow::bail!(
                "Permission denied writing to '{}'.\nPlease re-run with elevated privileges (e.g. 'sudo paynal update') or install Paynal in your user directory (e.g. ~/.local/bin).",
                exe_dir.display()
            );
        }
        Err(e) => {
            anyhow::bail!("Cannot write to directory '{}': {}", exe_dir.display(), e);
        }
    }

    println!("📥 Downloading {}...", asset.name);
    let bytes = client
        .get(&asset.browser_download_url)
        .send()
        .await
        .context("Failed to download release asset")?
        .bytes()
        .await
        .context("Failed to read release asset bytes")?;

    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;

        let gz = flate2::read::GzDecoder::new(&bytes[..]);
        let mut archive = tar::Archive::new(gz);
        let temp_new_exe = exe_dir.join(format!(".paynal.new.{}", std::process::id()));

        let mut extracted = false;
        for entry in archive
            .entries()
            .context("Failed to read tar archive entries")?
        {
            let mut entry = entry.context("Failed reading tar archive entry")?;
            let path = entry.path().context("Invalid path in archive entry")?;
            if path.file_name().and_then(|f| f.to_str()) == Some("paynal") {
                let mut out_file = std::fs::File::create(&temp_new_exe).with_context(|| {
                    format!(
                        "Failed to create temporary executable: {}",
                        temp_new_exe.display()
                    )
                })?;
                std::io::copy(&mut entry, &mut out_file)
                    .context("Failed to extract paynal binary from tar archive")?;
                extracted = true;
                break;
            }
        }

        if !extracted {
            let _ = std::fs::remove_file(&temp_new_exe);
            anyhow::bail!(
                "Binary 'paynal' not found in downloaded archive '{}'",
                asset.name
            );
        }

        let mut perms = std::fs::metadata(&temp_new_exe)
            .context("Failed to read metadata of extracted executable")?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&temp_new_exe, perms)
            .context("Failed setting executable permissions (0755)")?;

        std::fs::rename(&temp_new_exe, &current_exe)
            .context("Failed replacing current executable with new binary")?;
    }

    #[cfg(windows)]
    {
        let temp_zip =
            std::env::temp_dir().join(format!("paynal_update_{}.zip", std::process::id()));
        std::fs::write(&temp_zip, &bytes).context("Failed to write temporary zip archive")?;
        let temp_extract_dir =
            std::env::temp_dir().join(format!("paynal_extracted_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_extract_dir);

        let extract_status = std::process::Command::new("tar")
            .args([
                "-xf",
                temp_zip.to_str().unwrap_or(""),
                "-C",
                temp_extract_dir.to_str().unwrap_or(""),
            ])
            .status();

        let success = match extract_status {
            Ok(s) if s.success() => true,
            _ => {
                let zip_escaped = temp_zip.display().to_string().replace('\'', "''");
                let dest_escaped = temp_extract_dir.display().to_string().replace('\'', "''");
                let ps_cmd = format!(
                    "Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
                    zip_escaped,
                    dest_escaped
                );
                let ps_status = std::process::Command::new("powershell")
                    .args([
                        "-NoProfile",
                        "-Command",
                        &ps_cmd,
                    ])
                    .status();
                matches!(ps_status, Ok(s) if s.success())
            }
        };

        if !success {
            let _ = std::fs::remove_file(&temp_zip);
            let _ = std::fs::remove_dir_all(&temp_extract_dir);
            anyhow::bail!("Failed to extract zip archive using tar or powershell");
        }

        let extracted_exe = temp_extract_dir.join("paynal.exe");
        if !extracted_exe.exists() {
            let _ = std::fs::remove_file(&temp_zip);
            let _ = std::fs::remove_dir_all(&temp_extract_dir);
            anyhow::bail!("Extracted archive did not contain paynal.exe");
        }

        let temp_new_exe = exe_dir.join(format!("paynal.new.{}.exe", std::process::id()));
        std::fs::copy(&extracted_exe, &temp_new_exe).context("Failed copying extracted binary")?;
        let _ = std::fs::remove_file(&temp_zip);
        let _ = std::fs::remove_dir_all(&temp_extract_dir);

        let old_exe = exe_dir.join("paynal.old.exe");
        let _ = std::fs::remove_file(&old_exe);

        std::fs::rename(&current_exe, &old_exe)
            .context("Failed renaming current paynal.exe to paynal.old.exe")?;

        if let Err(e) = std::fs::rename(&temp_new_exe, &current_exe) {
            let _ = std::fs::rename(&old_exe, &current_exe);
            return Err(e).context("Failed renaming paynal.new.exe to paynal.exe");
        }
    }

    save_update_cache(clean_latest);
    println!(
        "{}",
        format!("🎉 Successfully updated Paynal to v{}!", clean_latest)
            .green()
            .bold()
    );

    // Spawn updated binary to sync skill files with latest embedded templates
    let spawned = std::process::Command::new(&current_exe)
        .arg("update")
        .arg("--sync-skill")
        .status();

    if spawned.is_err() || !spawned.as_ref().map(|s| s.success()).unwrap_or(false) {
        let _ = sync_installed_skills();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_install_embedded_skill_in_workspace() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let skill_dir = install_skill_in_workspace(tmp.path()).expect("install skill");
        assert!(skill_dir.join("SKILL.md").exists());
        assert!(skill_dir
            .join("references")
            .join("yaml-schema-and-assertions.md")
            .exists());
        assert!(skill_dir
            .join("references")
            .join("multi-workspace-and-mcp.md")
            .exists());
        assert!(skill_dir
            .join("references")
            .join("import-export-and-ci.md")
            .exists());
    }

    #[test]
    fn test_is_newer_version() {
        assert!(is_newer_version("0.1.0", "v0.2.0"));
        assert!(is_newer_version("0.1.0", "0.2.0"));
        assert!(is_newer_version("0.1.0", "v0.1.1"));
        assert!(!is_newer_version("0.1.0", "0.1.0"));
        assert!(!is_newer_version("0.1.0", "v0.1.0"));
        assert!(!is_newer_version("0.2.0", "0.1.9"));
        assert!(!is_newer_version("1.0.0", "v0.9.9"));
        assert!(is_newer_version("0.1.0-alpha", "v0.1.0"));
        assert!(!is_newer_version("0.1.0", "v0.1.0-beta"));
    }

    #[test]
    fn test_asset_resolution() {
        let asset = get_target_asset_name();
        if !Path::new("/.flatpak-info").exists() && std::env::var("FLATPAK_ID").is_err() {
            match (std::env::consts::OS, std::env::consts::ARCH) {
                ("linux", "x86_64") => {
                    assert_eq!(asset.unwrap(), "paynal-x86_64-unknown-linux-gnu.tar.gz")
                }
                ("macos", "x86_64") => {
                    assert_eq!(asset.unwrap(), "paynal-x86_64-apple-darwin.tar.gz")
                }
                ("macos", "aarch64") => {
                    assert_eq!(asset.unwrap(), "paynal-aarch64-apple-darwin.tar.gz")
                }
                ("windows", "x86_64") => {
                    assert_eq!(asset.unwrap(), "paynal-x86_64-pc-windows-msvc.zip")
                }
                _ => {}
            }
        } else {
            assert_eq!(asset.unwrap(), "paynal.flatpak");
        }
    }
}
