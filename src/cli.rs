use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(
    name = "paynal",
    author,
    version,
    about = "CLI-first API client, routine runner, and AI agent integration engine"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Initialize a new Paynal workspace in the current directory
    Init {
        /// Project name (defaults to current folder name)
        #[arg(short, long)]
        name: Option<String>,
    },

    /// Add a new request or routine YAML template
    Add {
        /// Target path in collection (e.g. collectionName/folder1/requestName)
        path: String,

        /// Shortcut for HTTP GET method
        #[arg(long, default_value_t = false)]
        get: bool,

        /// HTTP Method (GET, POST, PUT, DELETE, PATCH, HEAD, OPTIONS)
        #[arg(short, long, default_value = "GET")]
        r#type: String,

        /// Create a multi-step routine template instead of a single request
        #[arg(long, default_value_t = false)]
        routine: bool,

        /// Body template type override (json, form, text, xml, none)
        #[arg(short = 'b', long = "body", alias = "body-type")]
        body: Option<String>,
    },

    /// Execute a request, routine, or an entire collection folder
    Exec {
        /// Target request, routine, or collection directory
        path: String,

        /// Custom export output file path
        #[arg(short, long)]
        export: Option<String>,

        /// Run requests in parallel when executing a collection folder
        #[arg(long, default_value_t = false)]
        parallel: bool,

        /// Max threads override (CPUMAX, FULLMAX, or integer)
        #[arg(long)]
        threads: Option<String>,

        /// Environment profile (e.g. local, staging, prod) loads paynal.env.<profile>
        #[arg(long, short = 'E')]
        env: Option<String>,
    },

    /// Remove a request or routine from the workspace index
    Remove {
        /// Target request or routine path
        path: String,

        /// Physically delete the file from disk
        #[arg(long, default_value_t = false)]
        clean: bool,
    },

    /// Clean unindexed orphan files or output directory
    Clean {
        /// Target to clean: "project" (unindexed files) or "out" (output dir)
        target: CleanTarget,
    },

    /// Generate Markdown documentation for a request or routine
    Doc {
        /// Target request, routine, or collection folder
        path: String,

        /// Add Input/Output fields in "field:type:Description" format
        #[arg(long = "IO", value_delimiter = ',')]
        io: Vec<String>,
    },

    /// Export requests or collections to standard formats (curl, insomnia, postman)
    Export {
        /// Target request, routine, or collection folder
        path: String,

        /// Format type to export to
        #[arg(short, long, value_enum)]
        r#type: ExportType,
    },

    /// Start Model Context Protocol (MCP) server mode for AI Agents
    Mcp,

    /// Import requests from external formats (Postman v2.1, Insomnia v4)
    Import {
        /// Path to Postman or Insomnia JSON file
        file: String,

        /// Output directory for imported YAML collection files
        #[arg(short, long, default_value = "collections")]
        out: String,
    },

    /// Launch interactive Terminal User Interface (TUI) dashboard
    #[command(alias = "tui")]
    Ui {
        /// Environment profile (e.g. local, staging, prod)
        #[arg(long, short = 'E')]
        env: Option<String>,
    },
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CleanTarget {
    Project,
    Out,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportType {
    Curl,
    Insomnia,
    Postman,
}
