use anyhow::Result;
use clap::Parser;
use paynal::cli::{Cli, Commands};
use paynal::commands;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

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
    }

    Ok(())
}
