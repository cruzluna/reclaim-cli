use clap::{
    builder::NonEmptyStringValueParser, value_parser, ArgAction, Args, Parser, Subcommand,
    ValueEnum,
};

const AFTER_HELP: &str = "\
Examples:
  reclaim list
  reclaim list --all --format json
  reclaim get 123
  reclaim patch 123 --set priority=P4 --set snoozeUntil=2026-02-25T17:00:00Z
  reclaim put 123 --set priority=P2 --set due=2026-02-28T17:00:00Z
  reclaim delete 123
  reclaim create --title \"Plan Q1 roadmap\" --priority P2 --event-category WORK
  reclaim events list --start 2026-02-01 --end 2026-02-28 --format json
  reclaim events get 829105 r2d260ojiopn --format json
  reclaim events create --calendar-id 829105 --title \"Team sync\" --start 2026-02-21T18:30:00Z --end 2026-02-21T19:00:00Z
  reclaim events update --calendar-id 829105 --event-id r2d260ojiopn --set priority=P4
  reclaim events delete --calendar-id 829105 --event-id r2d260ojiopn
  RECLAIM_API_KEY=... reclaim list

Agent-friendly tip:
  Use --format json for stable machine-readable output and --json/--set for updates.
";

const DEFAULT_POLICY_ID: &str = "00000000-0000-0000-0000-000000000000";

#[derive(Debug, Parser)]
#[command(
    name = "reclaim",
    bin_name = "reclaim",
    version,
    about = "Simple CLI for Reclaim.ai tasks and events.",
    long_about = "Simple CLI for Reclaim.ai tasks and events.

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
    #[command(
        about = "Replace a task via PUT.",
        long_about = "Replace a task via PUT.\n\nPass --json with a full task object, or pass --set key=value fields.\nIf only --set is passed, reclaim fetches the current task first and then applies your updates."
    )]
    Put(PutArgs),
    #[command(
        about = "Partially update a task via PATCH.",
        long_about = "Partially update a task via PATCH.\n\nPass --json with a partial JSON object and/or repeated --set key=value entries."
    )]
    Patch(PatchArgs),
    #[command(about = "Delete one task by ID.", aliases = ["del", "rm", "remove"])]
    Delete(DeleteArgs),
    #[command(
        about = "Manage calendar events.",
        long_about = "Manage calendar events.\n\nUse create/update/delete for convenient action wrappers, or use apply for raw /schedule-actions/apply-actions requests."
    )]
    Events(EventsArgs),
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

#[derive(Debug, Args)]
pub struct PutArgs {
    #[arg(help = "Task ID.")]
    pub task_id: u64,

    #[arg(
        long,
        value_name = "JSON_OBJECT",
        help = "Full task object JSON to send in PUT. Must be a JSON object."
    )]
    pub json: Option<String>,

    #[arg(
        long = "set",
        value_name = "KEY=VALUE",
        action = ArgAction::Append,
        help = "Field override for PUT. Repeatable. Value supports JSON literals (true, null, numbers, arrays, objects)."
    )]
    pub set: Vec<String>,

    #[arg(
        long = "notification-key",
        help = "Optional notification key forwarded to the Reclaim API."
    )]
    pub notification_key: Option<String>,
}

#[derive(Debug, Args)]
pub struct PatchArgs {
    #[arg(help = "Task ID.")]
    pub task_id: u64,

    #[arg(
        long,
        value_name = "JSON_OBJECT",
        help = "Partial task JSON object to send in PATCH. Must be a JSON object."
    )]
    pub json: Option<String>,

    #[arg(
        long = "set",
        value_name = "KEY=VALUE",
        action = ArgAction::Append,
        help = "Field update for PATCH. Repeatable. Value supports JSON literals (true, null, numbers, arrays, objects)."
    )]
    pub set: Vec<String>,

    #[arg(
        long = "notification-key",
        help = "Optional notification key forwarded to the Reclaim API."
    )]
    pub notification_key: Option<String>,
}

#[derive(Debug, Args)]
pub struct DeleteArgs {
    #[arg(help = "Task ID.")]
    pub task_id: u64,

    #[arg(
        long = "notification-key",
        help = "Optional notification key forwarded to the Reclaim API."
    )]
    pub notification_key: Option<String>,
}

#[derive(Debug, Args)]
pub struct EventsArgs {
    #[command(subcommand)]
    pub command: EventsCommand,
}

#[derive(Debug, Subcommand)]
pub enum EventsCommand {
    #[command(about = "List events (GET /events).")]
    List(EventsListArgs),
    #[command(about = "Get one event by calendar ID + event ID.")]
    Get(EventsGetArgs),
    #[command(
        about = "Create an event via AddEventAction.",
        long_about = "Create an event using /schedule-actions/apply-actions with AddEventAction.\n\nUse flags for common fields and --json/--set for advanced fields."
    )]
    Create(EventsCreateArgs),
    #[command(
        about = "Update an event via UpdateEventAction.",
        long_about = "Update an event using /schedule-actions/apply-actions with UpdateEventAction.\n\nUse flags for common fields and --json/--set for advanced fields."
    )]
    Update(EventsUpdateArgs),
    #[command(about = "Delete/cancel an event via CancelEventAction.")]
    Delete(EventsDeleteArgs),
    #[command(
        about = "Apply raw schedule actions JSON.",
        long_about = "Apply raw /schedule-actions/apply-actions JSON.\n\nThis is useful for advanced automation when create/update/delete wrappers are not enough."
    )]
    Apply(EventsApplyArgs),
}

#[derive(Debug, Args)]
pub struct EventsListArgs {
    #[arg(
        long = "calendar-id",
        action = ArgAction::Append,
        help = "Filter by calendar ID. Repeatable."
    )]
    pub calendar_ids: Vec<u64>,

    #[arg(long = "all-connected", help = "Include all connected calendars.")]
    pub all_connected: bool,

    #[arg(long, help = "Optional start date filter (YYYY-MM-DD).")]
    pub start: Option<String>,

    #[arg(long, help = "Optional end date filter (YYYY-MM-DD).")]
    pub end: Option<String>,

    #[arg(long = "source-details", help = "Include source details if available.")]
    pub source_details: bool,

    #[arg(long, help = "Request thin payloads from Reclaim.")]
    pub thin: bool,
}

#[derive(Debug, Args)]
pub struct EventsGetArgs {
    #[arg(help = "Calendar ID.")]
    pub calendar_id: u64,

    #[arg(help = "Event ID.")]
    pub event_id: String,

    #[arg(long = "source-details", help = "Include source details if available.")]
    pub source_details: bool,

    #[arg(long, help = "Request thin payload from Reclaim.")]
    pub thin: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum, Eq, PartialEq)]
pub enum EventTransparency {
    #[value(name = "OPAQUE")]
    Opaque,
    #[value(name = "TRANSPARENT")]
    Transparent,
}

impl EventTransparency {
    pub fn as_str(self) -> &'static str {
        match self {
            EventTransparency::Opaque => "OPAQUE",
            EventTransparency::Transparent => "TRANSPARENT",
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum, Eq, PartialEq)]
pub enum EventVisibility {
    #[value(name = "DEFAULT")]
    Default,
    #[value(name = "PUBLIC")]
    Public,
    #[value(name = "PRIVATE")]
    Private,
}

impl EventVisibility {
    pub fn as_str(self) -> &'static str {
        match self {
            EventVisibility::Default => "DEFAULT",
            EventVisibility::Public => "PUBLIC",
            EventVisibility::Private => "PRIVATE",
        }
    }
}

#[derive(Debug, Args)]
pub struct EventsCreateArgs {
    #[arg(long = "calendar-id", help = "Calendar ID for the new event.")]
    pub calendar_id: u64,

    #[arg(
        long,
        value_parser = NonEmptyStringValueParser::new(),
        help = "Event title."
    )]
    pub title: String,

    #[arg(long, help = "Start timestamp (ISO 8601), e.g. 2026-02-21T18:30:00Z.")]
    pub start: String,

    #[arg(long, help = "End timestamp (ISO 8601), e.g. 2026-02-21T19:00:00Z.")]
    pub end: String,

    #[arg(
        long = "policy-id",
        default_value = DEFAULT_POLICY_ID,
        help = "Policy UUID used in AddEventAction."
    )]
    pub policy_id: String,

    #[arg(
        long = "attendee",
        action = ArgAction::Append,
        help = "Attendee email. Repeatable."
    )]
    pub attendees: Vec<String>,

    #[arg(long, help = "Optional event description.")]
    pub description: Option<String>,

    #[arg(long, help = "Optional event location.")]
    pub location: Option<String>,

    #[arg(long, value_enum, help = "Optional event priority (P1-P4).")]
    pub priority: Option<Priority>,

    #[arg(long, value_enum, help = "Optional event visibility.")]
    pub visibility: Option<EventVisibility>,

    #[arg(long, value_enum, help = "Optional event transparency.")]
    pub transparency: Option<EventTransparency>,

    #[arg(
        long = "guests-can-modify",
        default_value_t = false,
        help = "Whether guests can modify the event."
    )]
    pub guests_can_modify: bool,

    #[arg(
        long = "guests-can-invite-others",
        default_value_t = true,
        help = "Whether guests can invite others."
    )]
    pub guests_can_invite_others: bool,

    #[arg(
        long = "guests-can-see-other-guests",
        default_value_t = true,
        help = "Whether guests can see other guests."
    )]
    pub guests_can_see_other_guests: bool,

    #[arg(
        long,
        value_name = "JSON_OBJECT",
        help = "Additional AddEventAction fields as a JSON object."
    )]
    pub json: Option<String>,

    #[arg(
        long = "set",
        value_name = "KEY=VALUE",
        action = ArgAction::Append,
        help = "Additional AddEventAction field override. Repeatable."
    )]
    pub set: Vec<String>,
}

#[derive(Debug, Args)]
pub struct EventsUpdateArgs {
    #[arg(long = "calendar-id", help = "Calendar ID for the event.")]
    pub calendar_id: u64,

    #[arg(long = "event-id", help = "Event ID to update.")]
    pub event_id: String,

    #[arg(
        long = "policy-id",
        default_value = DEFAULT_POLICY_ID,
        help = "Policy UUID used in UpdateEventAction."
    )]
    pub policy_id: String,

    #[arg(long, help = "Optional updated title.")]
    pub title: Option<String>,

    #[arg(long, help = "Optional updated description.")]
    pub description: Option<String>,

    #[arg(long, help = "Optional updated location.")]
    pub location: Option<String>,

    #[arg(long, value_enum, help = "Optional updated priority (P1-P4).")]
    pub priority: Option<Priority>,

    #[arg(long, value_enum, help = "Optional updated visibility.")]
    pub visibility: Option<EventVisibility>,

    #[arg(long, value_enum, help = "Optional updated transparency.")]
    pub transparency: Option<EventTransparency>,

    #[arg(long, help = "Optional updated start timestamp (ISO 8601).")]
    pub start: Option<String>,

    #[arg(long, help = "Optional updated end timestamp (ISO 8601).")]
    pub end: Option<String>,

    #[arg(
        long,
        value_name = "JSON_OBJECT",
        help = "Additional UpdateEventAction fields as a JSON object."
    )]
    pub json: Option<String>,

    #[arg(
        long = "set",
        value_name = "KEY=VALUE",
        action = ArgAction::Append,
        help = "Additional UpdateEventAction field override. Repeatable."
    )]
    pub set: Vec<String>,
}

#[derive(Debug, Args)]
pub struct EventsDeleteArgs {
    #[arg(long = "calendar-id", help = "Calendar ID for the event.")]
    pub calendar_id: u64,

    #[arg(long = "event-id", help = "Event ID to delete/cancel.")]
    pub event_id: String,

    #[arg(
        long = "policy-id",
        default_value = DEFAULT_POLICY_ID,
        help = "Policy UUID used in CancelEventAction."
    )]
    pub policy_id: String,

    #[arg(
        long = "message",
        help = "Optional notification message for attendees."
    )]
    pub message: Option<String>,
}

#[derive(Debug, Args)]
pub struct EventsApplyArgs {
    #[arg(
        long,
        value_name = "JSON_OBJECT",
        help = "Raw ApplyScheduleActionsRequest JSON object."
    )]
    pub json: String,
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
