use crate::evaluator::VariableContext;
use crate::manifest::PaynalManifest;
use crate::models::{AssertSpec, AuthSpec, PaynalFile, RequestSpec, ResolvedOutputSections};
use crate::runner::HttpRunner;
use anyhow::{Context, Result};
use colored::Colorize;
use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path};

#[derive(Debug, Clone, Default)]
pub struct ReqArgs {
    pub method_or_url: String,
    pub url: Option<String>,
    pub headers: Vec<String>,
    pub body: Option<String>,
    pub params: Vec<String>,
    pub bearer: Option<String>,
    pub assert_status: Option<u16>,
    pub assert_json: Vec<String>,
    pub sections: Option<Vec<String>>,
    pub env: Option<String>,
    pub vars: Vec<String>,
    pub timeout: Option<u64>,
    pub insecure: bool,
    pub dry_run: bool,
    pub verbose: bool,
    pub save: Option<String>,
}

pub async fn execute_req(args: ReqArgs) -> Result<i32> {
    // 1. Resolve method and target URL
    let (method, raw_url) = match &args.url {
        Some(u) => (args.method_or_url.to_uppercase(), u.trim().to_string()),
        None => {
            let trimmed = args.method_or_url.trim();
            if let Some((m, u)) = trimmed.split_once(' ') {
                (m.trim().to_uppercase(), u.trim().to_string())
            } else {
                ("GET".to_string(), trimmed.to_string())
            }
        }
    };

    let url_str = if !raw_url.starts_with("http://")
        && !raw_url.starts_with("https://")
        && !raw_url.contains("${")
    {
        format!("http://{}", raw_url)
    } else {
        raw_url
    };

    // 2. Load manifest and variable context
    let manifest = PaynalManifest::load_from_dir(Path::new(".")).unwrap_or_default();
    let mut ctx = VariableContext::new_with_env(args.env.as_deref());

    // 3. Parse CLI --var overrides
    for var_entry in &args.vars {
        if let Some((k, v)) = var_entry.split_once('=') {
            ctx.set(k.trim(), v.trim());
        } else {
            eprintln!("⚠️  Invalid --var format '{}', expected KEY=VAL", var_entry);
        }
    }

    // 4. Parse headers
    let mut headers = HashMap::new();
    for h in &args.headers {
        if let Some((k, v)) = h.split_once(':') {
            headers.insert(k.trim().to_string(), v.trim().to_string());
        } else if let Some((k, v)) = h.split_once('=') {
            headers.insert(k.trim().to_string(), v.trim().to_string());
        }
    }

    // 5. Parse query parameters
    let mut params_map = HashMap::new();
    for p in &args.params {
        if let Some((k, v)) = p.split_once('=') {
            params_map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }

    // 6. Construct RequestSpec
    let auth = args.bearer.as_ref().map(|token| AuthSpec::Bearer {
        token: token.clone(),
    });

    let request_spec = RequestSpec {
        method: method.clone(),
        url: url_str.clone(),
        params: if params_map.is_empty() {
            None
        } else {
            Some(params_map)
        },
        headers: headers.clone(),
        body: args.body.clone(),
        form_data: None,
        timeout_ms: args.timeout,
        retry: None,
        follow_redirects: Some(true),
        auth,
        output_sections: args.sections.clone(),
    };

    // 7. Parse Assertions
    let mut json_asserts = HashMap::new();
    for j in &args.assert_json {
        if let Some((path, val_str)) = j.split_once('=') {
            let val_trimmed = val_str.trim();
            let json_val = serde_json::from_str::<serde_json::Value>(val_trimmed)
                .unwrap_or_else(|_| serde_json::Value::String(val_trimmed.to_string()));
            json_asserts.insert(path.trim().to_string(), json_val);
        }
    }

    let assert_spec = if args.assert_status.is_some() || !json_asserts.is_empty() {
        Some(AssertSpec {
            status: args.assert_status,
            json: if json_asserts.is_empty() {
                None
            } else {
                Some(json_asserts)
            },
            ..Default::default()
        })
    } else {
        None
    };

    // 8. Handle --save <path> option
    if let Some(save_path) = &args.save {
        let path_obj = Path::new(save_path);
        if path_obj.is_absolute()
            || path_obj
                .components()
                .any(|c| matches!(c, Component::ParentDir | Component::RootDir | Component::Prefix(_)))
        {
            anyhow::bail!(
                "Invalid save path '{}': path traversal ('..') or absolute paths are not allowed.",
                save_path
            );
        }

        let clean_path = save_path.trim();
        let stripped = clean_path
            .strip_prefix("collections/")
            .or_else(|| clean_path.strip_prefix("collections\\"))
            .unwrap_or(clean_path);

        let stripped_path = Path::new(stripped);
        if stripped_path.is_absolute()
            || stripped_path
                .components()
                .any(|c| matches!(c, Component::ParentDir | Component::RootDir | Component::Prefix(_)))
        {
            anyhow::bail!(
                "Invalid save path '{}': path traversal ('..') or absolute paths are not allowed.",
                save_path
            );
        }

        let filename = if stripped.ends_with(".yaml") || stripped.ends_with(".yml") {
            stripped.to_string()
        } else {
            format!("{}.yaml", stripped)
        };

        let collections_dir = Path::new(&manifest.root_dir).join("collections");
        let target_file = collections_dir.join(&filename);

        if let Some(parent) = target_file.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }

        let paynal_file = PaynalFile {
            version: "1.0".to_string(),
            name: format!("{} {}", method, url_str),
            description: Some("Created via paynal req --save".to_string()),
            vars: HashMap::new(),
            continue_on_failure: None,
            request: Some(request_spec.clone()),
            assert: assert_spec.clone(),
            steps: Vec::new(),
            output_sections: None,
        };

        let yaml_str = serde_yaml::to_string(&paynal_file)
            .context("Failed to serialize saved request to YAML")?;
        fs::write(&target_file, yaml_str)
            .with_context(|| format!("Failed to write file: {}", target_file.display()))?;

        println!(
            "{} Saved inline request to: {}",
            "💾".cyan().bold(),
            target_file.display()
        );
    }

    // 9. Handle --dry-run
    if args.dry_run {
        let mut preview_headers = headers.clone();
        if let Some(token) = &args.bearer {
            preview_headers.insert("Authorization".to_string(), format!("Bearer {}", token));
        }
        let interpolated_url = ctx.interpolate(&url_str);
        HttpRunner::dry_run_preview(
            &format!("{} {}", method, url_str),
            &method,
            &interpolated_url,
            &preview_headers,
            args.body.as_deref(),
        );
        return Ok(0);
    }

    // 10. Execute HTTP Request
    let timeout_ms = args.timeout.unwrap_or(manifest.timeout_ms);
    let validate_certs = if args.insecure {
        false
    } else {
        manifest.validate_certificates
    };

    let runner = HttpRunner::new_with_timeout(
        validate_certs,
        manifest.proxy.as_deref(),
        Some(timeout_ms),
    );

    let resolved_sections = ResolvedOutputSections::resolve(
        args.sections.as_ref(),
        None,
        None,
        &manifest.output_sections,
        args.verbose,
    );

    let interpolated_url = ctx.interpolate(&url_str);
    let res = runner
        .execute(&request_spec, assert_spec.as_ref(), &ctx)
        .await?;

    runner.print_result_with_sections(
        &format!("{} {}", method, url_str),
        &method,
        &interpolated_url,
        &res,
        resolved_sections,
        Some(&headers),
        args.body.as_deref(),
    );

    if res.asserts_passed {
        Ok(0)
    } else {
        Ok(1)
    }
}
