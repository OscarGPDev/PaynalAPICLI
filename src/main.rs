use anyhow::Result;
use clap::Parser;
use colored::Colorize;
use paynal::cli::{Cli, Commands};
use paynal::commands;

#[tokio::main]
async fn main() -> Result<()> {
    #[cfg(windows)]
    {
        if let Ok(current_exe) = std::env::current_exe() {
            if let Some(exe_dir) = current_exe.parent() {
                let old_exe = exe_dir.join("paynal.old.exe");
                if old_exe.exists() {
                    let _ = std::fs::remove_file(old_exe);
                }
            }
        }
    }

    let cli = Cli::parse();

    if let Some(ws_name) = &cli.workspace {
        let clean_name = ws_name.strip_prefix('@').unwrap_or(ws_name).trim();
        let registry = commands::global::GlobalRegistry::load()?;
        if let Some(entry) = registry.find_workspace(clean_name) {
            let ws_path = std::path::PathBuf::from(&entry.path);
            if !ws_path.exists() {
                anyhow::bail!(
                    "Global workspace '{}' directory does not exist: {}",
                    entry.name,
                    entry.path
                );
            }
            std::env::set_current_dir(&ws_path)?;
        } else {
            anyhow::bail!(
                "Global workspace '{}' not found in registry. Run 'paynal global list' to view registered workspaces.",
                ws_name
            );
        }
    }

    let should_check_notice = match &cli.command {
        Commands::Init { .. }
        | Commands::Add { .. }
        | Commands::Doc { .. }
        | Commands::Export { .. }
        | Commands::Import { .. } => true,
        Commands::Exec { reporter, .. } => *reporter == paynal::reporters::ReporterType::Human,
        _ => false,
    };

    match cli.command {
        Commands::Init { name } => {
            commands::init::execute_init(name)?;
        }
        Commands::Add {
            path,
            get,
            r#type,
            routine,
            body,
        } => {
            commands::add::execute_add(path, get, r#type, routine, body)?;
        }
        Commands::Exec {
            path,
            export,
            parallel,
            threads,
            env,
            timeout,
            strict_vars,
            fail_fast,
            dry_run,
            verbose,
            insecure,
            proxy,
            vars,
            reporter,
            out,
        } => {
            let options = commands::exec::ExecOptions {
                export_override: export,
                parallel,
                threads_override: threads,
                env_profile: env,
                timeout_override: timeout,
                strict_vars,
                fail_fast,
                dry_run,
                verbose,
                insecure,
                proxy_override: proxy,
                cli_vars: vars,
                reporter,
                report_out: out,
            };
            let exit_code = commands::exec::execute_exec_with_options(path, options).await?;
            if exit_code != 0 {
                std::process::exit(exit_code);
            }
        }
        Commands::Remove { path, clean } => {
            commands::execute_remove(path, clean)?;
        }
        Commands::Clean { target } => {
            commands::execute_clean(target)?;
        }
        Commands::Doc { path, io } => {
            commands::execute_doc(path, io)?;
        }
        Commands::Export { path, r#type } => {
            commands::execute_export(path, r#type)?;
        }
        Commands::Mcp => {
            commands::execute_mcp().await?;
        }
        Commands::Import { file, out } => {
            commands::execute_import(file, out)?;
        }
        Commands::Ui { env } => {
            commands::execute_tui(env).await?;
        }
        Commands::Man { subcommand, out } => {
            commands::execute_man(subcommand, out)?;
        }
        Commands::Update {
            check,
            yes,
            sync_skill,
        } => {
            commands::execute_update(check, yes, sync_skill).await?;
        }
        Commands::Global { action } => match action {
            paynal::cli::GlobalAction::Add {
                name,
                path,
                description,
            } => {
                commands::global::execute_add(name, path, description)?;
            }
            paynal::cli::GlobalAction::List { json } => {
                commands::global::execute_list(json)?;
            }
            paynal::cli::GlobalAction::Discover { query, json } => {
                commands::global::execute_discover(query, json)?;
            }
            paynal::cli::GlobalAction::Show { target } => {
                commands::global::execute_show(target)?;
            }
            paynal::cli::GlobalAction::Remove { name } => {
                commands::global::execute_remove(name)?;
            }
        },
        Commands::Req {
            method_or_url,
            url,
            headers,
            body,
            params,
            bearer,
            assert_status,
            assert_json,
            sections,
            env,
            vars,
            timeout,
            insecure,
            dry_run,
            verbose,
            save,
        } => {
            let args = commands::req_cmd::ReqArgs {
                method_or_url,
                url,
                headers,
                body,
                params,
                bearer,
                assert_status,
                assert_json,
                sections,
                env,
                vars,
                timeout,
                insecure,
                dry_run,
                verbose,
                save,
            };
            let exit_code = commands::req_cmd::execute_req(args).await?;
            if exit_code != 0 {
                std::process::exit(exit_code);
            }
        }
    }

    if should_check_notice {
        if let Some(new_ver) = commands::update::check_update_notice_cached().await {
            eprintln!(
                "\n{}",
                format!(
                    "💡 Update available: v{} → v{}. Run 'paynal update' to upgrade.",
                    env!("CARGO_PKG_VERSION"),
                    new_ver
                )
                .yellow()
            );
        }
    }

    Ok(())
}
