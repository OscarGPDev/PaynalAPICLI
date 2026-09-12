use crate::evaluator::VariableContext;
use crate::manifest::PaynalManifest;
use crate::models::PaynalFile;
use crate::reporters::{FileReport, ReporterType, StepReport, TestReport};
use crate::runner::HttpRunner;
use anyhow::{Context, Result};
use chrono::Local;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Debug, Clone, Default)]
pub struct ExecOptions {
    pub export_override: Option<String>,
    pub parallel: bool,
    pub threads_override: Option<String>,
    pub env_profile: Option<String>,
    pub timeout_override: Option<u64>,
    pub strict_vars: bool,
    pub fail_fast: bool,
    pub dry_run: bool,
    pub verbose: bool,
    pub insecure: bool,
    pub proxy_override: Option<String>,
    pub cli_vars: Vec<String>,
    pub reporter: ReporterType,
    pub report_out: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RunFileOptions {
    pub export_override: Option<String>,
    pub env_profile: Option<String>,
    pub strict_vars: bool,
    pub dry_run: bool,
    pub verbose: bool,
    pub cli_vars: HashMap<String, String>,
}

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
    timeout_override: Option<u64>,
    strict_vars: bool,
    fail_fast: bool,
) -> Result<i32> {
    let options = ExecOptions {
        export_override,
        parallel,
        threads_override,
        env_profile,
        timeout_override,
        strict_vars,
        fail_fast,
        dry_run: false,
        verbose: false,
        insecure: false,
        proxy_override: None,
        cli_vars: Vec::new(),
        reporter: ReporterType::Human,
        report_out: None,
    };
    execute_exec_with_options(path_str, options).await
}

pub async fn execute_exec_with_options(
    path_str: String,
    options: ExecOptions,
) -> Result<i32> {
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
            let dir_candidate = collections_base.join(&path_str);
            if dir_candidate.is_dir() {
                target = dir_candidate;
            }
        }
    }

    if !target.exists() {
        anyhow::bail!("Target path not found: {}", path_str);
    }

    let mut files_to_run = Vec::new();
    if target.is_dir() {
        files_to_run = find_yaml_files_recursive(&target);
        if files_to_run.is_empty() {
            println!("⚠️  No YAML routine or request files found in directory: {}", target.display());
            return Ok(0);
        }
        files_to_run.sort();
    } else {
        files_to_run.push(target);
    }

    let timeout_ms = options.timeout_override.unwrap_or(manifest.timeout_ms);
    let validate_certs = if options.insecure {
        false
    } else {
        manifest.validate_certificates
    };
    let proxy = options
        .proxy_override
        .as_deref()
        .or(manifest.proxy.as_deref());

    let runner = std::sync::Arc::new(HttpRunner::new_with_timeout(
        validate_certs,
        proxy,
        Some(timeout_ms),
    ));
    let manifest_arc = std::sync::Arc::new(manifest);

    // Parse CLI --var overrides
    let mut cli_vars_map = HashMap::new();
    for var_entry in &options.cli_vars {
        if let Some((k, v)) = var_entry.split_once('=') {
            cli_vars_map.insert(k.trim().to_string(), v.trim().to_string());
        } else {
            eprintln!("⚠️  Invalid --var format '{}', expected KEY=VAL", var_entry);
        }
    }

    let run_file_opts = std::sync::Arc::new(RunFileOptions {
        export_override: options.export_override.clone(),
        env_profile: options.env_profile.clone(),
        strict_vars: options.strict_vars,
        dry_run: options.dry_run,
        verbose: options.verbose,
        cli_vars: cli_vars_map,
    });

    let start_all = Instant::now();
    let mut test_report = TestReport::default();

    if options.parallel && files_to_run.len() > 1 {
        let global_ctx = VariableContext::new_with_env(options.env_profile.as_deref());
        let raw_thread_mode = options
            .threads_override
            .as_deref()
            .unwrap_or(&manifest_arc.max_threads);

        let thread_mode = global_ctx.interpolate(raw_thread_mode);
        let cpu_cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);

        let max_concurrency = match thread_mode.trim() {
            "CPUMAX" => cpu_cores,
            "FULLMAX" => files_to_run.len().max(1),
            other => other.parse::<usize>().unwrap_or(cpu_cores),
        };

        let permits = max_concurrency.min(tokio::sync::Semaphore::MAX_PERMITS);

        println!(
            "⚡ Executing {} files in parallel (Max Concurrency: {})...",
            files_to_run.len(),
            if thread_mode.trim() == "FULLMAX" {
                format!("FULLMAX ({})", permits)
            } else {
                permits.to_string()
            }
        );

        let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(permits));
        let mut join_set = tokio::task::JoinSet::new();

        for file_path in files_to_run {
            let runner_cloned = std::sync::Arc::clone(&runner);
            let manifest_cloned = std::sync::Arc::clone(&manifest_arc);
            let opts_cloned = std::sync::Arc::clone(&run_file_opts);
            let sem_cloned = std::sync::Arc::clone(&semaphore);

            join_set.spawn(async move {
                let _permit = sem_cloned.acquire().await;
                (
                    file_path.clone(),
                    run_paynal_file_ext(&runner_cloned, &file_path, &manifest_cloned, &opts_cloned).await,
                )
            });
        }

        while let Some(res) = join_set.join_next().await {
            match res {
                Ok((_file_path, Ok(file_report))) => {
                    let passed = file_report.passed;
                    test_report.files.push(file_report);
                    if !passed && options.fail_fast {
                        println!("⏹️  Halting remaining tasks due to --fail-fast.");
                        join_set.abort_all();
                        break;
                    }
                }
                Ok((file_path, Err(e))) => {
                    eprintln!("⚠️  Execution error in {}: {}", file_path.display(), e);
                    test_report.files.push(FileReport {
                        name: file_path.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown").to_string(),
                        path: file_path.display().to_string(),
                        is_routine: false,
                        duration_ms: 0,
                        passed: false,
                        error: Some(e.to_string()),
                        steps: Vec::new(),
                    });
                    if options.fail_fast {
                        println!("⏹️  Halting remaining tasks due to --fail-fast.");
                        join_set.abort_all();
                        break;
                    }
                }
                Err(join_err) => {
                    eprintln!("⚠️  Task join error: {}", join_err);
                    test_report.files.push(FileReport {
                        name: "parallel_task".to_string(),
                        path: String::new(),
                        is_routine: false,
                        duration_ms: 0,
                        passed: false,
                        error: Some(join_err.to_string()),
                        steps: Vec::new(),
                    });
                    if options.fail_fast {
                        println!("⏹️  Halting remaining tasks due to --fail-fast.");
                        join_set.abort_all();
                        break;
                    }
                }
            }
        }
    } else {
        for file_path in &files_to_run {
            match run_paynal_file_ext(&runner, file_path, &manifest_arc, &run_file_opts).await {
                Ok(file_report) => {
                    let passed = file_report.passed;
                    test_report.files.push(file_report);
                    if !passed && options.fail_fast {
                        println!("⏹️  Halting execution due to --fail-fast.");
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("⚠️  Execution error in {}: {}", file_path.display(), e);
                    test_report.files.push(FileReport {
                        name: file_path.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown").to_string(),
                        path: file_path.display().to_string(),
                        is_routine: false,
                        duration_ms: 0,
                        passed: false,
                        error: Some(e.to_string()),
                        steps: Vec::new(),
                    });
                    if options.fail_fast {
                        println!("⏹️  Halting execution due to --fail-fast.");
                        break;
                    }
                }
            }
        }
    }

    test_report.total_duration_ms = start_all.elapsed().as_millis();
    test_report.compute_summary();

    // Render report output according to selected reporter
    match options.reporter {
        ReporterType::Human => {
            let summary_str = test_report.to_human_summary();
            println!("{}", summary_str);
            if let Some(out_path) = &options.report_out {
                if let Some(parent) = Path::new(out_path).parent() {
                    let _ = fs::create_dir_all(parent);
                }
                if let Err(e) = fs::write(out_path, &summary_str) {
                    eprintln!("⚠️  Failed to write report to {}: {}", out_path, e);
                } else {
                    println!("📄 Report written to: {}", out_path);
                }
            }
        }
        ReporterType::Json => {
            let json_str = test_report.to_json()?;
            if let Some(out_path) = &options.report_out {
                if let Some(parent) = Path::new(out_path).parent() {
                    let _ = fs::create_dir_all(parent);
                }
                if let Err(e) = fs::write(out_path, &json_str) {
                    eprintln!("⚠️  Failed to write JSON report to {}: {}", out_path, e);
                } else {
                    println!("📄 JSON report written to: {}", out_path);
                }
            } else {
                println!("{}", json_str);
            }
        }
        ReporterType::Junit => {
            let xml_str = test_report.to_junit_xml();
            if let Some(out_path) = &options.report_out {
                if let Some(parent) = Path::new(out_path).parent() {
                    let _ = fs::create_dir_all(parent);
                }
                if let Err(e) = fs::write(out_path, &xml_str) {
                    eprintln!("⚠️  Failed to write JUnit XML report to {}: {}", out_path, e);
                } else {
                    println!("📄 JUnit XML report written to: {}", out_path);
                }
            } else {
                println!("{}", xml_str);
            }
        }
    }

    if test_report.errored_files > 0 {
        Ok(2)
    } else if test_report.failed_files > 0 {
        Ok(1)
    } else {
        Ok(0)
    }
}

pub async fn run_paynal_file(
    runner: &HttpRunner,
    file_path: &Path,
    manifest: &PaynalManifest,
    export_override: Option<&str>,
    env_profile: Option<&str>,
    strict_vars: bool,
) -> Result<bool> {
    let opts = RunFileOptions {
        export_override: export_override.map(|s| s.to_string()),
        env_profile: env_profile.map(|s| s.to_string()),
        strict_vars,
        dry_run: false,
        verbose: false,
        cli_vars: HashMap::new(),
    };
    let report = run_paynal_file_ext(runner, file_path, manifest, &opts).await?;
    Ok(report.passed)
}

pub async fn run_paynal_file_ext(
    runner: &HttpRunner,
    file_path: &Path,
    manifest: &PaynalManifest,
    opts: &RunFileOptions,
) -> Result<FileReport> {
    let start_file_time = Instant::now();
    let content = fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read YAML file {}", file_path.display()))?;

    let paynal_file: PaynalFile = serde_yaml::from_str(&content)
        .with_context(|| format!("Failed to parse YAML file {}", file_path.display()))?;

    let mut ctx = VariableContext::new_with_env(opts.env_profile.as_deref());
    ctx.strict_vars = opts.strict_vars;
    ctx.extend(&paynal_file.vars);
    // CLI variables override file and env variables
    ctx.extend(&opts.cli_vars);

    let mut export_logs = Vec::new();
    let mut file_report = FileReport {
        name: paynal_file.name.clone(),
        path: file_path.display().to_string(),
        is_routine: paynal_file.is_routine(),
        duration_ms: 0,
        passed: true,
        error: None,
        steps: Vec::new(),
    };

    if paynal_file.is_routine() {
        let file_continue = paynal_file.continue_on_failure.unwrap_or(false);
        println!("🔄 Executing Routine: {}", paynal_file.name);
        let mut halted = false;

        for step in &paynal_file.steps {
            let step_name = step.name.as_deref().unwrap_or(&step.id);

            if halted {
                file_report.steps.push(StepReport {
                    id: step.id.clone(),
                    name: step_name.to_string(),
                    method: step.request.method.clone(),
                    url: step.request.url.clone(),
                    status: 0,
                    duration_ms: 0,
                    passed: false,
                    skipped: true,
                    asserts: Vec::new(),
                    failure_messages: Vec::new(),
                    error: None,
                });
                continue;
            }

            let mut step_ctx = ctx.clone();
            if !step.vars.is_empty() {
                step_ctx.extend(&step.vars);
            }

            if opts.strict_vars {
                step_ctx.validate_no_unresolved_placeholders(&step.request.url)?;
                for v in step.request.headers.values() {
                    step_ctx.validate_no_unresolved_placeholders(v)?;
                }
                if let Some(b) = &step.request.body {
                    step_ctx.validate_no_unresolved_placeholders(b)?;
                }
            }

            let interpolated_url = step_ctx.interpolate(&step.request.url);
            let mut interpolated_headers = HashMap::new();
            for (k, v) in &step.request.headers {
                interpolated_headers.insert(step_ctx.interpolate(k), step_ctx.interpolate(v));
            }
            let interpolated_body = step.request.body.as_ref().map(|b| step_ctx.interpolate(b));

            if opts.dry_run {
                HttpRunner::dry_run_preview(
                    step_name,
                    &step.request.method,
                    &interpolated_url,
                    &interpolated_headers,
                    interpolated_body.as_deref(),
                );

                file_report.steps.push(StepReport {
                    id: step.id.clone(),
                    name: step_name.to_string(),
                    method: step.request.method.clone(),
                    url: interpolated_url,
                    status: 200,
                    duration_ms: 0,
                    passed: true,
                    skipped: false,
                    asserts: Vec::new(),
                    failure_messages: Vec::new(),
                    error: None,
                });
                continue;
            }

            let step_retry = step.retry.as_ref().or(step.request.retry.as_ref());
            let res = runner
                .execute_with_retry(&step.request, step.assert.as_ref(), &step_ctx, step_retry)
                .await?;

            if !res.asserts_passed {
                file_report.passed = false;
            }

            runner.print_result_full(
                step_name,
                &step.request.method,
                &interpolated_url,
                &res,
                opts.verbose,
                Some(&interpolated_headers),
                interpolated_body.as_deref(),
            );

            // Extract captured variables into context for subsequent steps
            runner.extract_captures(&res, &step.capture, &mut ctx);

            let step_failures: Vec<String> = res
                .assert_messages
                .iter()
                .filter(|m| m.contains("FAILED") || m.contains("mismatch") || m.contains("exceeded") || m.contains("not found") || m.contains("NOT found"))
                .cloned()
                .collect();

            file_report.steps.push(StepReport {
                id: step.id.clone(),
                name: step_name.to_string(),
                method: step.request.method.clone(),
                url: interpolated_url.clone(),
                status: res.status,
                duration_ms: res.duration_ms,
                passed: res.asserts_passed,
                skipped: false,
                asserts: res.assert_messages.clone(),
                failure_messages: step_failures,
                error: None,
            });

            export_logs.push(format!(
                "Step: {}\nURL: {} {}\nStatus: {}\nDuration: {}ms\nBody:\n{}\n",
                step_name, step.request.method, interpolated_url, res.status, res.duration_ms, res.body
            ));

            // Halt routine on step assertion failure unless continue_on_failure is enabled
            let step_continue = step.continue_on_failure.unwrap_or(file_continue);
            if !res.asserts_passed && !step_continue {
                println!(
                    "⏹️  Step '{}' failed assertions. Halting routine early (continueOnFailure=false).",
                    step_name
                );
                halted = true;
            }
        }
    } else if let Some(req) = &paynal_file.request {
        println!("🚀 Executing Single Request: {}", paynal_file.name);
        if opts.strict_vars {
            ctx.validate_no_unresolved_placeholders(&req.url)?;
            for v in req.headers.values() {
                ctx.validate_no_unresolved_placeholders(v)?;
            }
            if let Some(b) = &req.body {
                ctx.validate_no_unresolved_placeholders(b)?;
            }
        }

        let interpolated_url = ctx.interpolate(&req.url);
        let mut interpolated_headers = HashMap::new();
        for (k, v) in &req.headers {
            interpolated_headers.insert(ctx.interpolate(k), ctx.interpolate(v));
        }
        let interpolated_body = req.body.as_ref().map(|b| ctx.interpolate(b));

        if opts.dry_run {
            HttpRunner::dry_run_preview(
                &paynal_file.name,
                &req.method,
                &interpolated_url,
                &interpolated_headers,
                interpolated_body.as_deref(),
            );

            file_report.steps.push(StepReport {
                id: "request-1".to_string(),
                name: paynal_file.name.clone(),
                method: req.method.clone(),
                url: interpolated_url,
                status: 200,
                duration_ms: 0,
                passed: true,
                skipped: false,
                asserts: Vec::new(),
                failure_messages: Vec::new(),
                error: None,
            });
        } else {
            let res = runner
                .execute_with_retry(req, paynal_file.assert.as_ref(), &ctx, req.retry.as_ref())
                .await?;

            if !res.asserts_passed {
                file_report.passed = false;
            }

            runner.print_result_full(
                &paynal_file.name,
                &req.method,
                &interpolated_url,
                &res,
                opts.verbose,
                Some(&interpolated_headers),
                interpolated_body.as_deref(),
            );

            let step_failures: Vec<String> = res
                .assert_messages
                .iter()
                .filter(|m| m.contains("FAILED") || m.contains("mismatch") || m.contains("exceeded") || m.contains("not found") || m.contains("NOT found"))
                .cloned()
                .collect();

            file_report.steps.push(StepReport {
                id: "request-1".to_string(),
                name: paynal_file.name.clone(),
                method: req.method.clone(),
                url: interpolated_url.clone(),
                status: res.status,
                duration_ms: res.duration_ms,
                passed: res.asserts_passed,
                skipped: false,
                asserts: res.assert_messages.clone(),
                failure_messages: step_failures,
                error: None,
            });

            export_logs.push(format!(
                "Request: {}\nURL: {} {}\nStatus: {}\nDuration: {}ms\nBody:\n{}\n",
                paynal_file.name, req.method, interpolated_url, res.status, res.duration_ms, res.body
            ));
        }
    } else {
        println!("⚠️  File {} contains neither a single request nor routine steps.", file_path.display());
    }

    file_report.duration_ms = start_file_time.elapsed().as_millis();

    // Save export if requested or if output directory exists (skipped in dry-run unless explicitly passed)
    let export_target = opts.export_override.as_deref();
    if (!opts.dry_run && Path::new(&manifest.out_dir).exists()) || export_target.is_some() {
        let export_path = match export_target {
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

    Ok(file_report)
}
