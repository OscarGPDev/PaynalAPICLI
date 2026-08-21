use crate::cli::Cli;
use anyhow::{Context, Result};
use clap::CommandFactory;
use clap_mangen::Man;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

pub fn execute_man(subcommand: Option<String>, out_dir: Option<String>) -> Result<()> {
    let cmd = Cli::command();

    if let Some(dir_str) = out_dir {
        let dir_path = Path::new(&dir_str);
        if !dir_path.exists() {
            fs::create_dir_all(dir_path)
                .with_context(|| format!("Failed to create directory {}", dir_path.display()))?;
        }

        clap_mangen::generate_to(cmd, dir_path)
            .with_context(|| format!("Failed to generate man pages to {}", dir_path.display()))?;

        println!("📖 Generated UNIX man pages in: {}", dir_path.display());
    } else {
        let target_cmd = if let Some(target_sub) = &subcommand {
            let target_lower = target_sub.to_lowercase();
            let found = cmd
                .get_subcommands()
                .find(|s| s.get_name() == target_lower || s.get_all_aliases().any(|a| a == target_lower))
                .cloned();
            found.unwrap_or(cmd)
        } else {
            cmd
        };

        let mut buffer = Vec::new();
        Man::new(target_cmd).render(&mut buffer)?;

        let child = Command::new("man")
            .arg("-l")
            .arg("-")
            .stdin(Stdio::piped())
            .spawn();

        match child {
            Ok(mut proc) => {
                if let Some(mut stdin) = proc.stdin.take() {
                    let _ = stdin.write_all(&buffer);
                }
                let _ = proc.wait();
            }
            Err(_) => {
                if let Ok(mut less_proc) = Command::new("less").stdin(Stdio::piped()).spawn() {
                    if let Some(mut stdin) = less_proc.stdin.take() {
                        let _ = stdin.write_all(&buffer);
                    }
                    let _ = less_proc.wait();
                } else {
                    let roff_str = String::from_utf8_lossy(&buffer);
                    println!("{}", roff_str);
                }
            }
        }
    }

    Ok(())
}
