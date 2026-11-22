use clap::{Parser, Subcommand};
use std::path::PathBuf;

pub mod commands;

pub use commands::run;

#[derive(Parser, Debug)]
#[command(
    name = "sker",
    version,
    about = "Offline-first negative knowledge guardrails and compliance compiler for AI agents"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    #[command(about = "Compile rules into an immutable bundle and rkyv cache")]
    Compile {
        #[arg(long, default_value = ".", help = "Path to target repository root")]
        repo: PathBuf,

        #[arg(long, help = "Path to playbook directory or manifest (overrides repo)")]
        playbook: Option<PathBuf>,

        #[arg(
            long,
            help = "Output directory for compiled bundle [default: <repo>/.sker]"
        )]
        out: Option<PathBuf>,

        #[arg(
            long = "update-lock",
            help = "Update or generate .sker.lock for federated rule sources"
        )]
        update_lock: bool,
    },

    #[command(about = "Run incremental rule analysis on git diff lines")]
    Check {
        #[arg(
            long,
            help = "Path to compiled bundle (.rkyv or .json file, or directory)"
        )]
        bundle: Option<PathBuf>,

        #[arg(long, default_value = ".", help = "Path to target repository root")]
        repo: PathBuf,

        #[arg(long, help = "Enable strict authoritative CI recomputation mode")]
        ci: bool,

        #[arg(
            long,
            requires = "ci",
            help = "Path to write SARIF 2.1.0 diagnostic output"
        )]
        sarif: Option<PathBuf>,

        #[arg(long, requires = "ci", help = "Path to write CheckReport JSON report")]
        json: Option<PathBuf>,

        #[arg(long, help = "Target base Git revision to diff against")]
        base: Option<String>,

        #[arg(
            long,
            help = "Path to agent credential file [default: .sker/credential.json]"
        )]
        credential: Option<PathBuf>,
    },

    #[command(about = "Retrieve applicable rules by intent and target paths")]
    Consult {
        #[arg(
            long,
            help = "Path to compiled bundle (.rkyv or .json file, or directory)"
        )]
        bundle: Option<PathBuf>,

        #[arg(long, help = "Natural language description of proposed code changes")]
        intent: Option<String>,

        #[arg(
            long = "paths",
            value_delimiter = ',',
            help = "Target file or directory paths to be modified"
        )]
        named_paths: Vec<String>,

        #[arg(help = "Natural language description (positional fallback)")]
        pos_intent: Option<String>,

        #[arg(help = "Target file or directory paths (positional fallback)")]
        pos_paths: Vec<String>,

        #[arg(long, default_value = ".", help = "Path to target repository root")]
        repo: PathBuf,
    },

    #[command(about = "Full-text rule search by keyword or tag")]
    Search {
        #[arg(
            long,
            help = "Path to compiled bundle (.rkyv or .json file, or directory)"
        )]
        bundle: Option<PathBuf>,

        #[arg(long, help = "Search query string or tag keyword")]
        query: Option<String>,

        #[arg(help = "Search query string (positional fallback)")]
        pos_query: Option<String>,

        #[arg(long, default_value = ".", help = "Path to target repository root")]
        repo: PathBuf,
    },

    #[command(about = "Generate BLAKE3 compliance credential for agent diffs")]
    Attest {
        #[arg(long, default_value = ".", help = "Path to target repository root")]
        repo: PathBuf,

        #[arg(
            long,
            default_value = "[]",
            help = "JSON array of semantic verdicts: '[{\"rule_id\":\"...\",\"state\":\"comply\",\"declaration\":\"...\"}]'"
        )]
        semantic: String,

        #[arg(long, help = "Path to write the generated credential file")]
        output: Option<PathBuf>,
    },

    #[command(about = "Propose a new rule entry from incident rationale")]
    Propose {
        #[arg(help = "Path to draft rule entry YAML file")]
        entry: PathBuf,

        #[arg(help = "Incident rationale or motivation for proposing this rule")]
        rationale: String,

        #[arg(long, default_value = ".", help = "Path to target repository root")]
        repo: PathBuf,
    },

    #[command(about = "Shadow replay active rules over historical git commits")]
    Replay {
        #[arg(help = "Starting commit hash or git reference")]
        from: String,

        #[arg(help = "Ending commit hash or git reference")]
        to: String,

        #[arg(long, default_value = ".", help = "Path to target repository root")]
        repo: PathBuf,
    },

    #[command(about = "Initialize Sker configuration and agent guides")]
    Init {
        #[arg(long, default_value = ".", help = "Path to target repository root")]
        repo: PathBuf,

        #[arg(
            long = "agent",
            value_delimiter = ',',
            help = "Agent integrations to enable (e.g. claude,codex,codebuddy)"
        )]
        agents: Vec<String>,

        #[arg(
            long = "git-hooks",
            help = "Install a pre-commit hook running sker check"
        )]
        git_hooks: bool,
    },

    #[command(about = "Manage declarative agent integrations")]
    Agent {
        #[command(subcommand)]
        operation: AgentCommand,

        #[arg(long, default_value = ".", help = "Path to target repository root")]
        repo: PathBuf,
    },

    #[command(about = "Diagnose git repository, rules, and agent configuration")]
    Doctor {
        #[arg(long, default_value = ".", help = "Path to target repository root")]
        repo: PathBuf,
    },

    #[command(about = "Manage legacy violation baseline snapshots")]
    Baseline {
        #[command(subcommand)]
        operation: BaselineCommand,

        #[arg(long, default_value = ".", help = "Path to target repository root")]
        repo: PathBuf,
    },

    #[command(about = "Start Model Context Protocol (MCP) server over stdio")]
    Mcp {
        #[arg(long, default_value = ".", help = "Path to target repository root")]
        repo: PathBuf,
    },

    #[command(about = "Execute tool-use / pre-save interception hook")]
    Hook {
        #[arg(long, default_value = ".", help = "Path to target repository root")]
        repo: PathBuf,

        #[arg(
            long,
            help = "Exit with code 1 if deterministic BLOCK rules are violated"
        )]
        hard_block: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum BaselineCommand {
    #[command(about = "Snapshot current workspace violations into .sker/baseline.json")]
    Freeze,

    #[command(about = "Display active baseline debt exemptions and stable anchors")]
    List,

    #[command(about = "Re-evaluate and migrate baseline anchors against updated source code")]
    Update,

    #[command(about = "Prune resolved baseline items that no longer trigger violations")]
    Prune,
}

#[derive(Subcommand, Debug)]
pub enum AgentCommand {
    #[command(about = "List built-in agent presets and enabled integrations")]
    List,

    #[command(about = "Enable a preset or custom agent integration")]
    Add {
        #[arg(help = "Preset name, or 'custom' for a declarative integration")]
        name: String,

        #[arg(long = "name", help = "Custom integration name (with 'custom')")]
        custom_name: Option<String>,

        #[arg(long = "rules-file", help = "Custom rules file path (with 'custom')")]
        rules_file: Option<String>,

        #[arg(
            long = "mcp-file",
            help = "Custom MCP config file path (with 'custom')"
        )]
        mcp_file: Option<String>,
    },

    #[command(about = "Disable an agent integration in .sker.yaml")]
    Remove {
        #[arg(help = "Agent integration name")]
        name: String,
    },

    #[command(about = "Print a generic mcpServers JSON snippet for any MCP tool")]
    McpConfig,
}
