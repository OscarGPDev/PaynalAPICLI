mod cli;
mod commands;
mod evaluator;
mod manifest;
mod models;
mod runner;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};

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
        } => {
            commands::add::execute_add(path, get, r#type, routine)?;
        }
        Commands::Exec {
            path,
            export,
            parallel,
            threads,
            env,
        } => {
            commands::execute_exec(path, export, parallel, threads, env).await?;
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
    }

    Ok(())
}
