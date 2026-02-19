use clap::{builder::NonEmptyStringValueParser, value_parser, Args, Parser, Subcommand, ValueEnum};

const AFTER_HELP: &str = "\
Examples:
  reclaim list
  reclaim list --all --format json
  reclaim get 123
  reclaim create --title \"Plan Q1 roadmap\" --priority P2 --event-category WORK
  RECLAIM_API_KEY=... reclaim list

Agent-friendly tip:
  Use --format json for stable machine-readable output.
";

#[derive(Debug, Parser)]
#[command(
    name = "reclaim",
    bin_name = "reclaim",
    version,
    about = "Simple CLI for Reclaim.ai tasks.",
    long_about = "Simple CLI for Reclaim.ai tasks.

Set your API key with RECLAIM_API_KEY or pass --api-key.
Use --format json when another tool/agent will parse the output.",
    after_help = AFTER_HELP
)]
pub struct Cli {
    #[arg(
        long,
        env = "RECLAIM_API_KEY",
        global = true,
        hide_env_values = true,
        help = "Reclaim API key. Falls back to RECLAIM_API_KEY."
    )]
    pub api_key: Option<String>,

    #[arg(
        long,
        env = "RECLAIM_BASE_URL",
        default_value = "https://api.app.reclaim.ai/api",
        global = true,
        help = "Reclaim API base URL."
    )]
    pub base_url: String,

    #[arg(
        long,
        env = "RECLAIM_TIMEOUT_SECS",
        default_value_t = 15,
        global = true,
        help = "HTTP timeout in seconds."
    )]
    pub timeout_secs: u64,

    #[arg(
        long,
        value_enum,
        default_value_t = OutputFormat::Human,
        global = true,
        help = "Output format."
    )]
    pub format: OutputFormat,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(about = "List tasks (active by default).", alias = "ls")]
    List(ListArgs),
    #[command(about = "Get one task by ID.", alias = "show")]
    Get(GetArgs),
    #[command(about = "Create a new task.")]
    Create(CreateArgs),
}

#[derive(Debug, Args)]
pub struct ListArgs {
    #[arg(
        long,
        short = 'a',
        help = "Include all tasks, including archived/cancelled/deleted."
    )]
    pub all: bool,
}

#[derive(Debug, Args)]
pub struct GetArgs {
    #[arg(help = "Task ID.")]
    pub task_id: u64,
}

#[derive(Debug, Clone, Copy, ValueEnum, Eq, PartialEq)]
pub enum Priority {
    #[value(name = "P1")]
    P1,
    #[value(name = "P2")]
    P2,
    #[value(name = "P3")]
    P3,
    #[value(name = "P4")]
    P4,
}

impl Priority {
    pub fn as_str(self) -> &'static str {
        match self {
            Priority::P1 => "P1",
            Priority::P2 => "P2",
            Priority::P3 => "P3",
            Priority::P4 => "P4",
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum, Eq, PartialEq)]
pub enum EventCategory {
    #[value(name = "WORK")]
    Work,
    #[value(name = "PERSONAL")]
    Personal,
}

impl EventCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            EventCategory::Work => "WORK",
            EventCategory::Personal => "PERSONAL",
        }
    }
}

#[derive(Debug, Args)]
pub struct CreateArgs {
    #[arg(
        long,
        value_parser = NonEmptyStringValueParser::new(),
        help = "Task title (required)."
    )]
    pub title: String,

    #[arg(long, help = "Optional notes/description for the task.")]
    pub notes: Option<String>,

    #[arg(long, value_enum, help = "Optional priority (P1-P4).")]
    pub priority: Option<Priority>,

    #[arg(
        long,
        help = "Optional due timestamp (ISO 8601), e.g. 2026-02-19T15:00:00Z."
    )]
    pub due: Option<String>,

    #[arg(
        long = "time-chunks-required",
        help = "Optional total time in 15-minute chunks."
    )]
    pub time_chunks_required: Option<u32>,

    #[arg(
        long = "event-category",
        value_enum,
        default_value_t = EventCategory::Work,
        help = "Task category. Defaults to WORK."
    )]
    pub event_category: EventCategory,

    #[arg(
        long = "min-chunk-size",
        value_parser = value_parser!(u32).range(1..),
        help = "Minimum chunk size in 15-minute increments."
    )]
    pub min_chunk_size: Option<u32>,

    #[arg(
        long = "max-chunk-size",
        value_parser = value_parser!(u32).range(1..),
        help = "Maximum chunk size in 15-minute increments."
    )]
    pub max_chunk_size: Option<u32>,

    #[arg(
        long = "always-private",
        default_value_t = true,
        help = "Whether calendar blocks should be private (true/false). Defaults to true."
    )]
    pub always_private: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum, Eq, PartialEq)]
pub enum OutputFormat {
    Human,
    Json,
}
