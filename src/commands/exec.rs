use crate::evaluator::VariableContext;
use crate::manifest::PaynalManifest;
use crate::models::PaynalFile;
use crate::runner::HttpRunner;
use anyhow::{Context, Result};
use chrono::Local;
use std::fs;
use std::path::{Path, PathBuf};

fn find_yaml_files_recursive(dir: &Path) -> Vec<PathBuf> {
    let mut yaml_files = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                yaml_files.extend(find_yaml_files_recursive(&path));
            } else if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if ext == "yaml" || ext == "yml" {
                        yaml_files.push(path);
                    }
                }
            }
        }
    }
    yaml_files
}

pub async fn execute_exec(
    path_str: String,
    export_override: Option<String>,
    parallel: bool,
    threads_override: Option<String>,
    env_profile: Option<String>,
) -> Result<()> {
    let manifest = PaynalManifest::load_from_dir(Path::new("."))
        .unwrap_or_default();

    let root_path = PathBuf::from(&manifest.root_dir);
    let collections_base = root_path.join("collections");

    // Resolve target path
    let mut target = PathBuf::from(&path_str);
    if !target.exists() {
        let relative_yaml = if path_str.ends_with(".yaml") || path_str.ends_with(".yml") {
            path_str.clone()
        } else {
            format!("{}.yaml", path_str)
        };
        target = collections_base.join(&relative_yaml);

        if !target.exists() {
            // Check if it's a directory without file extension
            let dir_candidate = collections_base.join(&path_str);
            if dir_candidate.is_dir() {
                target = dir_candidate;
            }
        }
    }

    if !target.exists() {
        anyhow::bail!("Target path not found: {}", path_str);
    }

    let files_to_run = if target.is_dir() {
        let mut yaml_files = find_yaml_files_recursive(&target);
        yaml_files.sort();
        yaml_files
    } else {
        vec![target]
    };

    if files_to_run.is_empty() {
        println!("⚠️  No request/routine YAML files found at target.");
        return Ok(());
    }

    let runner = std::sync::Arc::new(HttpRunner::new(
        manifest.validate_certificates,
        manifest.proxy.as_deref(),
    ));
    let manifest = std::sync::Arc::new(manifest);
    let env_profile_arc = std::sync::Arc::new(env_profile);

    if parallel && files_to_run.len() > 1 {
        let global_ctx = VariableContext::new_with_env(env_profile_arc.as_deref());
        let raw_thread_mode = threads_override
            .as_deref()
            .unwrap_or(&manifest.max_threads);

        let thread_mode = global_ctx.interpolate(raw_thread_mode);
        let cpu_cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);

        let max_concurrency = match thread_mode.trim() {
            "CPUMAX" => cpu_cores,
            "FULLMAX" => usize::MAX,
            other => other.parse::<usize>().unwrap_or(cpu_cores),
        };

        println!(
            "⚡ Executing {} files in parallel (Max Concurrency: {})...",
            files_to_run.len(),
            if max_concurrency == usize::MAX {
                "FULLMAX (unlimited)".to_string()
            } else {
                max_concurrency.to_string()
            }
        );

        let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(max_concurrency));
        let mut join_set = tokio::task::JoinSet::new();

        for file_path in files_to_run {
            let runner_cloned = std::sync::Arc::clone(&runner);
            let manifest_cloned = std::sync::Arc::clone(&manifest);
            let env_cloned = std::sync::Arc::clone(&env_profile_arc);
            let export_cloned = export_override.clone();
            let sem_cloned = std::sync::Arc::clone(&semaphore);

            join_set.spawn(async move {
                let _permit = sem_cloned.acquire().await;
                run_paynal_file(
                    &runner_cloned,
                    &file_path,
                    &manifest_cloned,
                    export_cloned.as_deref(),
                    env_cloned.as_ref().as_deref(),
                )
                .await
            });
        }

        while let Some(res) = join_set.join_next().await {
            if let Err(e) = res {
                eprintln!("⚠️  Parallel task error: {}", e);
            }
        }
    } else {
        for file_path in &files_to_run {
            run_paynal_file(
                &runner,
                file_path,
                &manifest,
                export_override.as_deref(),
                env_profile_arc.as_ref().as_deref(),
            )
            .await?;
        }
    }

    Ok(())
}

async fn run_paynal_file(
    runner: &HttpRunner,
    file_path: &Path,
    manifest: &PaynalManifest,
    export_override: Option<&str>,
    env_profile: Option<&str>,
) -> Result<()> {
    let content = fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read YAML file {}", file_path.display()))?;

    let paynal_file: PaynalFile = serde_yaml::from_str(&content)
        .with_context(|| format!("Failed to parse YAML file {}", file_path.display()))?;

    let mut ctx = VariableContext::new_with_env(env_profile);
    ctx.extend(&paynal_file.vars);

    let mut export_logs = Vec::new();

    if paynal_file.is_routine() {
        println!("🔄 Executing Routine: {}", paynal_file.name);
        for step in &paynal_file.steps {
            let res = runner
                .execute(&step.request, step.assert.as_ref(), &ctx)
                .await?;

            let step_name = step.name.as_deref().unwrap_or(&step.id);
            let url = ctx.interpolate(&step.request.url);
            runner.print_result(step_name, &step.request.method, &url, &res);

            // Extract captured variables into context for subsequent steps
            runner.extract_captures(&res, &step.capture, &mut ctx);

            export_logs.push(format!(
                "Step: {}\nURL: {} {}\nStatus: {}\nDuration: {}ms\nBody:\n{}\n",
                step_name, step.request.method, url, res.status, res.duration_ms, res.body
            ));
        }
    } else if let Some(req) = &paynal_file.request {
        println!("🚀 Executing Single Request: {}", paynal_file.name);
        let res = runner
            .execute(req, paynal_file.assert.as_ref(), &ctx)
            .await?;

        let url = ctx.interpolate(&req.url);
        runner.print_result(&paynal_file.name, &req.method, &url, &res);

        export_logs.push(format!(
            "Request: {}\nURL: {} {}\nStatus: {}\nDuration: {}ms\nBody:\n{}\n",
            paynal_file.name, req.method, url, res.status, res.duration_ms, res.body
        ));
    } else {
        println!("⚠️  File {} contains neither a single request nor routine steps.", file_path.display());
        return Ok(());
    }

    // Save export if requested or if output directory exists
    if export_override.is_some() || Path::new(&manifest.out_dir).exists() {
        let export_path = match export_override {
            Some(path) => PathBuf::from(path),
            None => {
                let timestamp = Local::now().format("%Y%m%d_%H%M%S");
                let stem = file_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("output");
                PathBuf::from(&manifest.out_dir).join(format!("{}.{}.txt", stem, timestamp))
            }
        };

        if let Some(parent) = export_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let full_log = export_logs.join("\n========================================\n");
        if let Err(e) = fs::write(&export_path, full_log) {
            eprintln!("⚠️  Failed to save export to {}: {}", export_path.display(), e);
        } else {
            println!("💾 Export saved to: {}", export_path.display());
        }
    }

    Ok(())
}
