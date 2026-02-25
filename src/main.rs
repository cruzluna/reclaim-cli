mod cli;
mod dashboard;
mod error;
mod reclaim_api;

use clap::Parser;
use cli::{
    Cli, Command, EventsApplyArgs, EventsCommand, EventsCreateArgs, EventsDeleteArgs,
    EventsUpdateArgs, OutputFormat, PatchArgs, PutArgs, TaskCompletionFilter,
};
use error::CliError;
use reclaim_api::{
    CreateTaskRequest, EventListQuery, HttpReclaimApi, ReclaimApi, Task, TaskFilter,
};
use serde_json::{json, Map, Value};
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: {error}");
            if let Some(hint) = error.hint() {
                eprintln!("Hint: {hint}");
            }
            ExitCode::from(2)
        }
    }
}

async fn run() -> Result<(), CliError> {
    let cli = Cli::parse();
    let format = cli.format;
    let command = cli.command;

    let api = HttpReclaimApi::new(cli.api_key, cli.base_url, cli.timeout_secs)?;

    match command {
        Command::List(args) => {
            let filter = if args.all {
                TaskFilter::All
            } else {
                TaskFilter::Active
            };
            let mut tasks = api.list_tasks(filter).await?;
            apply_task_completion_filter(&mut tasks, args.filter);

            match format {
                OutputFormat::Json => print_json(&tasks)?,
                OutputFormat::Human => print_task_list_human(args.all, args.filter, &tasks),
            }
        }
        Command::Dashboard(args) => {
            if matches!(format, OutputFormat::Json) {
                return Err(CliError::InvalidInput {
                    message:
                        "The dashboard is an interactive TUI and only supports --format human."
                            .to_string(),
                    hint: Some("Run: reclaim dashboard".to_string()),
                });
            }

            dashboard::run_dashboard(&api, args.all).await?;
        }
        Command::Get(args) => {
            let task = api.get_task(args.task_id).await?;

            match format {
                OutputFormat::Json => print_json(&task)?,
                OutputFormat::Human => print_task_human(&task),
            }
        }
        Command::Put(args) => {
            let request = build_put_payload(&api, &args).await?;
            let updated = api
                .put_task(args.task_id, request, args.notification_key.as_deref())
                .await?;

            match format {
                OutputFormat::Json => print_json(&updated)?,
                OutputFormat::Human => print_mutation_human("Updated (PUT)", &updated),
            }
        }
        Command::Patch(args) => {
            let request = build_patch_payload(&args)?;
            let updated = api
                .patch_task(args.task_id, request, args.notification_key.as_deref())
                .await?;

            match format {
                OutputFormat::Json => print_json(&updated)?,
                OutputFormat::Human => print_mutation_human("Updated (PATCH)", &updated),
            }
        }
        Command::Delete(args) => {
            let api_response = api
                .delete_task(args.task_id, args.notification_key.as_deref())
                .await?;
            let result = DeleteTaskOutput {
                task_id: args.task_id,
                deleted: true,
                api_response,
            };

            match format {
                OutputFormat::Json => print_json(&result)?,
                OutputFormat::Human => {
                    println!("Deleted task #{}.", result.task_id);
                    if !result.api_response.is_null() {
                        let rendered = serde_json::to_string_pretty(&result.api_response).map_err(
                            |error| {
                                CliError::Output(format!(
                                    "Could not render delete response JSON: {error}"
                                ))
                            },
                        )?;
                        println!("API response:\n{rendered}");
                    }
                }
            }
        }
        Command::Events(args) => match args.command {
            EventsCommand::List(event_args) => {
                let query = EventListQuery {
                    calendar_ids: event_args.calendar_ids,
                    all_connected: event_args.all_connected.then_some(true),
                    start: event_args.start,
                    end: event_args.end,
                    source_details: event_args.source_details.then_some(true),
                    thin: event_args.thin.then_some(true),
                };
                let events = api.list_events(query).await?;

                match format {
                    OutputFormat::Json => print_json(&events)?,
                    OutputFormat::Human => print_events_list_human(&events),
                }
            }
            EventsCommand::Get(event_args) => {
                let event = api
                    .get_event(
                        event_args.calendar_id,
                        &event_args.event_id,
                        event_args.source_details.then_some(true),
                        event_args.thin.then_some(true),
                    )
                    .await?;

                match format {
                    OutputFormat::Json => print_json(&event)?,
                    OutputFormat::Human => print_event_human(&event)?,
                }
            }
            EventsCommand::Create(event_args) => {
                let request = build_event_create_request(&event_args)?;
                let response = api.apply_schedule_actions(request).await?;
                let output = EventsMutationOutput {
                    operation: "create".to_string(),
                    calendar_id: event_args.calendar_id,
                    event_id: None,
                    response,
                };

                match format {
                    OutputFormat::Json => print_json(&output)?,
                    OutputFormat::Human => print_events_mutation_human(&output)?,
                }
            }
            EventsCommand::Update(event_args) => {
                let request = build_event_update_request(&event_args)?;
                let response = api.apply_schedule_actions(request).await?;
                let output = EventsMutationOutput {
                    operation: "update".to_string(),
                    calendar_id: event_args.calendar_id,
                    event_id: Some(event_args.event_id),
                    response,
                };

                match format {
                    OutputFormat::Json => print_json(&output)?,
                    OutputFormat::Human => print_events_mutation_human(&output)?,
                }
            }
            EventsCommand::Delete(event_args) => {
                let request = build_event_delete_request(&event_args)?;
                let response = api.apply_schedule_actions(request).await?;
                let output = EventsMutationOutput {
                    operation: "delete".to_string(),
                    calendar_id: event_args.calendar_id,
                    event_id: Some(event_args.event_id),
                    response,
                };

                match format {
                    OutputFormat::Json => print_json(&output)?,
                    OutputFormat::Human => print_events_mutation_human(&output)?,
                }
            }
            EventsCommand::Apply(event_args) => {
                let request = build_events_apply_request(&event_args)?;
                let response = api.apply_schedule_actions(request).await?;

                match format {
                    OutputFormat::Json => print_json(&response)?,
                    OutputFormat::Human => print_event_apply_human(&response)?,
                }
            }
        },
        Command::Create(args) => {
            if let Some(due) = &args.due {
                if due.trim().is_empty() {
                    return Err(CliError::InvalidInput {
                        message: "Invalid --due value: it cannot be empty.".to_string(),
                        hint: Some(
                            "Use ISO 8601, for example: --due 2026-02-19T15:00:00Z".to_string(),
                        ),
                    });
                }
            }

            if (args.min_chunk_size.is_some() || args.max_chunk_size.is_some())
                && args.time_chunks_required.is_none()
            {
                return Err(CliError::InvalidInput {
                    message: "Invalid chunk options: --min-chunk-size/--max-chunk-size require --time-chunks-required."
                        .to_string(),
                    hint: Some(
                        "Pass --time-chunks-required with chunk size options, e.g. --time-chunks-required 4 --min-chunk-size 2 --max-chunk-size 4"
                            .to_string(),
                    ),
                });
            }

            let mut min_chunk_size = args.min_chunk_size;
            let mut max_chunk_size = args.max_chunk_size;

            if let Some(total_chunks) = args.time_chunks_required {
                if min_chunk_size.is_none() {
                    min_chunk_size = Some(1);
                }
                if max_chunk_size.is_none() {
                    max_chunk_size = Some(total_chunks);
                }

                if let Some(min) = min_chunk_size {
                    if min > total_chunks {
                        return Err(CliError::InvalidInput {
                            message: format!(
                                "Invalid --min-chunk-size value: {min} exceeds --time-chunks-required ({total_chunks})."
                            ),
                            hint: Some(
                                "Use a min chunk size less than or equal to --time-chunks-required."
                                    .to_string(),
                            ),
                        });
                    }
                }

                if let Some(max) = max_chunk_size {
                    if max > total_chunks {
                        return Err(CliError::InvalidInput {
                            message: format!(
                                "Invalid --max-chunk-size value: {max} exceeds --time-chunks-required ({total_chunks})."
                            ),
                            hint: Some(
                                "Use a max chunk size less than or equal to --time-chunks-required."
                                    .to_string(),
                            ),
                        });
                    }
                }
            }

            if let (Some(min), Some(max)) = (min_chunk_size, max_chunk_size) {
                if min > max {
                    return Err(CliError::InvalidInput {
                        message: format!(
                            "Invalid chunk bounds: --min-chunk-size ({min}) cannot exceed --max-chunk-size ({max})."
                        ),
                        hint: Some("Choose chunk sizes where min <= max.".to_string()),
                    });
                }
            }

            let request = CreateTaskRequest {
                title: args.title,
                notes: args.notes,
                priority: args.priority.map(|priority| priority.as_str().to_owned()),
                due: args.due,
                time_chunks_required: args.time_chunks_required,
                event_category: Some(args.event_category.as_str().to_owned()),
                min_chunk_size,
                max_chunk_size,
                always_private: Some(args.always_private),
            };

            let created = api.create_task(request).await?;
            match format {
                OutputFormat::Json => print_json(&created)?,
                OutputFormat::Human => {
                    println!("Created task #{}: {}", created.id, created.title);
                    if let Some(status) = created.status.as_deref() {
                        println!("Status: {status}");
                    }
                    if let Some(due) = created.due.as_deref() {
                        println!("Due: {due}");
                    }
                }
            }
        }
    }

    Ok(())
}

#[derive(Debug, serde::Serialize)]
struct DeleteTaskOutput {
    task_id: u64,
    deleted: bool,
    api_response: Value,
}

#[derive(Debug, serde::Serialize)]
struct EventsMutationOutput {
    operation: String,
    calendar_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    event_id: Option<String>,
    response: Value,
}

fn build_events_apply_request(args: &EventsApplyArgs) -> Result<Value, CliError> {
    let request = Value::Object(parse_json_object_argument(&args.json, "--json")?);
    let has_actions = request
        .get("actionsTaken")
        .and_then(|value| value.as_array())
        .map(|actions| !actions.is_empty())
        .unwrap_or(false);

    if !has_actions {
        return Err(CliError::InvalidInput {
            message:
                "Invalid --json request: actionsTaken is required and must be a non-empty array."
                    .to_string(),
            hint: Some(
                "Example: --json '{\"actionsTaken\":[{\"type\":\"CancelEventAction\",...}]}'"
                    .to_string(),
            ),
        });
    }

    Ok(request)
}

fn build_event_create_request(args: &EventsCreateArgs) -> Result<Value, CliError> {
    let start = args.start.trim();
    let end = args.end.trim();
    if start.is_empty() || end.is_empty() {
        return Err(CliError::InvalidInput {
            message: "Invalid event time range: --start and --end are required.".to_string(),
            hint: Some(
                "Use ISO 8601 timestamps, e.g. --start 2026-02-21T18:30:00Z --end 2026-02-21T19:00:00Z"
                    .to_string(),
            ),
        });
    }

    let policy_id = args.policy_id.trim();
    if policy_id.is_empty() {
        return Err(CliError::InvalidInput {
            message: "Invalid --policy-id value: it cannot be empty.".to_string(),
            hint: Some(
                "Use a UUID, or omit --policy-id to use 00000000-0000-0000-0000-000000000000."
                    .to_string(),
            ),
        });
    }

    let mut action = Map::new();
    action.insert(
        "type".to_string(),
        Value::String("AddEventAction".to_string()),
    );
    action.insert("hash".to_string(), Value::String(String::new()));
    action.insert("policyId".to_string(), Value::String(policy_id.to_string()));
    action.insert("eventKey".to_string(), Value::String(String::new()));
    action.insert("calendarId".to_string(), json!(args.calendar_id));
    action.insert("title".to_string(), Value::String(args.title.clone()));
    action.insert(
        "dateRange".to_string(),
        json!({
            "type": "FixedDateTimeRange",
            "start": start,
            "end": end
        }),
    );
    action.insert("guestsCanModify".to_string(), json!(args.guests_can_modify));
    action.insert(
        "guestsCanInviteOthers".to_string(),
        json!(args.guests_can_invite_others),
    );
    action.insert(
        "guestsCanSeeOtherGuests".to_string(),
        json!(args.guests_can_see_other_guests),
    );

    let attendees = args
        .attendees
        .iter()
        .map(|email| email.trim())
        .filter(|email| !email.is_empty())
        .map(|email| json!({ "email": email }))
        .collect::<Vec<_>>();
    action.insert("attendees".to_string(), Value::Array(attendees));

    if let Some(description) = args
        .description
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        action.insert(
            "description".to_string(),
            Value::String(description.to_string()),
        );
    }

    if let Some(location) = args
        .location
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        action.insert("location".to_string(), Value::String(location.to_string()));
    }

    if let Some(priority) = args.priority {
        action.insert(
            "priority".to_string(),
            Value::String(priority.as_str().to_string()),
        );
    }
    if let Some(visibility) = args.visibility {
        action.insert(
            "visibility".to_string(),
            Value::String(visibility.as_str().to_string()),
        );
    }
    if let Some(transparency) = args.transparency {
        action.insert(
            "transparency".to_string(),
            Value::String(transparency.as_str().to_string()),
        );
    }

    if let Some(raw_json) = args.json.as_deref() {
        let updates = parse_json_object_argument(raw_json, "--json")?;
        merge_object_fields(&mut action, updates);
    }
    let updates = parse_set_entries(&args.set)?;
    merge_object_fields(&mut action, updates);

    Ok(json!({ "actionsTaken": [Value::Object(action)] }))
}

fn build_event_update_request(args: &EventsUpdateArgs) -> Result<Value, CliError> {
    let policy_id = args.policy_id.trim();
    if policy_id.is_empty() {
        return Err(CliError::InvalidInput {
            message: "Invalid --policy-id value: it cannot be empty.".to_string(),
            hint: Some(
                "Use a UUID, or omit --policy-id to use 00000000-0000-0000-0000-000000000000."
                    .to_string(),
            ),
        });
    }

    if args.start.is_some() ^ args.end.is_some() {
        return Err(CliError::InvalidInput {
            message: "Invalid date range update: --start and --end must be passed together."
                .to_string(),
            hint: Some(
                "Pass both --start and --end, or neither. For partial advanced updates, use --json."
                    .to_string(),
            ),
        });
    }

    let mut action = Map::new();
    action.insert(
        "type".to_string(),
        Value::String("UpdateEventAction".to_string()),
    );
    action.insert("hash".to_string(), Value::String(String::new()));
    action.insert("policyId".to_string(), Value::String(policy_id.to_string()));
    action.insert("calendarId".to_string(), json!(args.calendar_id));
    action.insert("eventId".to_string(), Value::String(args.event_id.clone()));

    if let Some(title) = args
        .title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        action.insert("title".to_string(), Value::String(title.to_string()));
    }
    if let Some(description) = args
        .description
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        action.insert(
            "description".to_string(),
            Value::String(description.to_string()),
        );
    }
    if let Some(location) = args
        .location
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        action.insert("location".to_string(), Value::String(location.to_string()));
    }
    if let Some(priority) = args.priority {
        action.insert(
            "priority".to_string(),
            Value::String(priority.as_str().to_string()),
        );
    }
    if let Some(visibility) = args.visibility {
        action.insert(
            "visibility".to_string(),
            Value::String(visibility.as_str().to_string()),
        );
    }
    if let Some(transparency) = args.transparency {
        action.insert(
            "transparency".to_string(),
            Value::String(transparency.as_str().to_string()),
        );
    }

    if let (Some(start), Some(end)) = (args.start.as_deref(), args.end.as_deref()) {
        let start = start.trim();
        let end = end.trim();
        if start.is_empty() || end.is_empty() {
            return Err(CliError::InvalidInput {
                message: "Invalid date range update: --start and --end cannot be empty."
                    .to_string(),
                hint: Some(
                    "Use ISO 8601 timestamps, e.g. --start 2026-02-21T18:30:00Z --end 2026-02-21T19:00:00Z"
                        .to_string(),
                ),
            });
        }

        action.insert(
            "dateRange".to_string(),
            json!({
                "type": "FixedDateTimeRange",
                "start": start,
                "end": end
            }),
        );
    }

    if let Some(raw_json) = args.json.as_deref() {
        let updates = parse_json_object_argument(raw_json, "--json")?;
        merge_object_fields(&mut action, updates);
    }
    let updates = parse_set_entries(&args.set)?;
    merge_object_fields(&mut action, updates);

    let has_update_fields = action.keys().any(|key| {
        !matches!(
            key.as_str(),
            "type" | "hash" | "policyId" | "calendarId" | "eventId"
        )
    });
    if !has_update_fields {
        return Err(CliError::InvalidInput {
            message: "Event update requires at least one field change.".to_string(),
            hint: Some(
                "Pass one of: --title/--description/--location/--priority/--start+--end, or use --json/--set."
                    .to_string(),
            ),
        });
    }

    Ok(json!({ "actionsTaken": [Value::Object(action)] }))
}

fn build_event_delete_request(args: &EventsDeleteArgs) -> Result<Value, CliError> {
    let policy_id = args.policy_id.trim();
    if policy_id.is_empty() {
        return Err(CliError::InvalidInput {
            message: "Invalid --policy-id value: it cannot be empty.".to_string(),
            hint: Some(
                "Use a UUID, or omit --policy-id to use 00000000-0000-0000-0000-000000000000."
                    .to_string(),
            ),
        });
    }

    let mut action = Map::new();
    action.insert(
        "type".to_string(),
        Value::String("CancelEventAction".to_string()),
    );
    action.insert("hash".to_string(), Value::String(String::new()));
    action.insert("policyId".to_string(), Value::String(policy_id.to_string()));
    action.insert(
        "eventKey".to_string(),
        Value::String(format!("{}/{}", args.calendar_id, args.event_id)),
    );

    if let Some(message) = args
        .message
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        action.insert(
            "notificationMessage".to_string(),
            Value::String(message.to_string()),
        );
    }

    Ok(json!({ "actionsTaken": [Value::Object(action)] }))
}

async fn build_put_payload(api: &impl ReclaimApi, args: &PutArgs) -> Result<Value, CliError> {
    if args.json.is_none() && args.set.is_empty() {
        return Err(CliError::InvalidInput {
            message: "PUT requires update data. Pass --json and/or one or more --set entries."
                .to_string(),
            hint: Some(
                "Examples: --json '{\"title\":\"Plan sprint\"}' or --set priority=P4".to_string(),
            ),
        });
    }

    let mut payload = if let Some(raw_json) = args.json.as_deref() {
        parse_json_object_argument(raw_json, "--json")?
    } else {
        let existing = api.get_task(args.task_id).await?;
        let existing_json = serde_json::to_value(existing).map_err(|error| {
            CliError::Output(format!(
                "Could not serialize existing task for PUT: {error}"
            ))
        })?;

        existing_json.as_object().cloned().ok_or_else(|| {
            CliError::Output(
                "Could not serialize existing task for PUT: expected object payload.".to_string(),
            )
        })?
    };

    let updates = parse_set_entries(&args.set)?;
    merge_object_fields(&mut payload, updates);

    Ok(Value::Object(payload))
}

fn build_patch_payload(args: &PatchArgs) -> Result<Value, CliError> {
    let mut payload = match args.json.as_deref() {
        Some(raw_json) => parse_json_object_argument(raw_json, "--json")?,
        None => Map::new(),
    };

    let updates = parse_set_entries(&args.set)?;
    merge_object_fields(&mut payload, updates);

    if payload.is_empty() {
        return Err(CliError::InvalidInput {
            message: "PATCH requires at least one field update.".to_string(),
            hint: Some(
                "Pass --json '{\"priority\":\"P4\"}' or one/more --set key=value entries."
                    .to_string(),
            ),
        });
    }

    Ok(Value::Object(payload))
}

fn parse_json_object_argument(
    raw_json: &str,
    flag_name: &str,
) -> Result<Map<String, Value>, CliError> {
    let raw_json = raw_json.trim();
    if raw_json.is_empty() {
        return Err(CliError::InvalidInput {
            message: format!("Invalid {flag_name} value: it cannot be empty."),
            hint: Some(format!(
                "Pass {flag_name} with a JSON object, e.g. {flag_name} '{{\"priority\":\"P4\"}}'."
            )),
        });
    }

    let parsed: Value = serde_json::from_str(raw_json).map_err(|error| CliError::InvalidInput {
        message: format!("Invalid {flag_name} JSON: {error}"),
        hint: Some(format!(
            "Pass {flag_name} with a JSON object, e.g. {flag_name} '{{\"priority\":\"P4\"}}'."
        )),
    })?;

    parsed
        .as_object()
        .cloned()
        .ok_or_else(|| CliError::InvalidInput {
            message: format!("Invalid {flag_name} value: expected a JSON object."),
            hint: Some(format!(
                "Pass {flag_name} with a JSON object, e.g. {flag_name} '{{\"priority\":\"P4\"}}'."
            )),
        })
}

fn parse_set_entries(entries: &[String]) -> Result<Map<String, Value>, CliError> {
    let mut updates = Map::new();
    for entry in entries {
        let (key, value) = parse_set_entry(entry)?;
        updates.insert(key, value);
    }
    Ok(updates)
}

fn parse_set_entry(entry: &str) -> Result<(String, Value), CliError> {
    let (raw_key, raw_value) = entry
        .split_once('=')
        .ok_or_else(|| CliError::InvalidInput {
            message: format!("Invalid --set value '{entry}'. Expected KEY=VALUE."),
            hint: Some(
                "Examples: --set priority=P4 --set snoozeUntil=2026-02-25T17:00:00Z".to_string(),
            ),
        })?;

    let key = raw_key.trim();
    if key.is_empty() {
        return Err(CliError::InvalidInput {
            message: format!("Invalid --set value '{entry}': key cannot be empty."),
            hint: Some("Use a non-empty key, e.g. --set priority=P4".to_string()),
        });
    }

    Ok((key.to_string(), parse_set_value(raw_value.trim())))
}

fn parse_set_value(raw_value: &str) -> Value {
    serde_json::from_str(raw_value).unwrap_or_else(|_| Value::String(raw_value.to_string()))
}

fn merge_object_fields(target: &mut Map<String, Value>, updates: Map<String, Value>) {
    for (key, value) in updates {
        target.insert(key, value);
    }
}

fn print_json<T: serde::Serialize>(value: &T) -> Result<(), CliError> {
    let rendered = serde_json::to_string_pretty(value)
        .map_err(|error| CliError::Output(format!("Could not render JSON output: {error}")))?;
    println!("{rendered}");
    Ok(())
}

fn apply_task_completion_filter(tasks: &mut Vec<Task>, filter: Option<TaskCompletionFilter>) {
    let Some(filter) = filter else {
        return;
    };

    tasks.retain(|task| match filter {
        TaskCompletionFilter::Open => !task_is_completed(task),
        TaskCompletionFilter::Completed => task_is_completed(task),
    });
}

fn task_is_completed(task: &Task) -> bool {
    if status_indicates_completed(task.status.as_deref()) {
        return true;
    }

    if task
        .extra
        .get("completionStatus")
        .and_then(Value::as_str)
        .is_some_and(|status| status_indicates_completed(Some(status)))
    {
        return true;
    }

    task.extra
        .get("completed")
        .and_then(Value::as_bool)
        .or_else(|| task.extra.get("isComplete").and_then(Value::as_bool))
        .unwrap_or(false)
}

fn status_indicates_completed(status: Option<&str>) -> bool {
    matches!(
        status.map(|status| status.to_ascii_uppercase()),
        Some(status)
            if matches!(status.as_str(), "COMPLETED" | "COMPLETE" | "DONE" | "FINISHED")
    )
}

fn print_task_list_human(
    includes_all: bool,
    completion_filter: Option<TaskCompletionFilter>,
    tasks: &[Task],
) {
    if tasks.is_empty() {
        let filter_text = completion_filter.map(|filter| match filter {
            TaskCompletionFilter::Open => "open",
            TaskCompletionFilter::Completed => "completed",
        });

        if includes_all {
            if let Some(filter_text) = filter_text {
                println!("No tasks found with completion status '{filter_text}'.");
            } else {
                println!("No tasks found.");
            }
        } else {
            if let Some(filter_text) = filter_text {
                println!("No active tasks found with completion status '{filter_text}'.");
            } else {
                println!("No active tasks found.");
            }
        }
        return;
    }

    for task in tasks {
        let status = task.status.as_deref().unwrap_or("UNKNOWN");
        let due = task.due.as_deref().unwrap_or("-");
        println!(
            "#{: <6} [{: <11}] {} (due: {due})",
            task.id, status, task.title
        );
    }

    println!("\nTip: use --format json for machine-readable output.");
}

fn print_task_human(task: &Task) {
    println!("#{} {}", task.id, task.title);
    if let Some(status) = task.status.as_deref() {
        println!("status: {status}");
    }
    if let Some(priority) = task.priority.as_deref() {
        println!("priority: {priority}");
    }
    if let Some(due) = task.due.as_deref() {
        println!("due: {due}");
    }
    if let Some(notes) = task.notes.as_deref() {
        println!("notes: {notes}");
    }
}

fn print_events_list_human(events: &[Value]) {
    if events.is_empty() {
        println!("No events found.");
        return;
    }

    for event in events {
        let title =
            json_text_by_pointers(event, &["/title"]).unwrap_or_else(|| "<untitled>".to_string());
        let key =
            json_text_by_pointers(event, &["/key", "/eventKey"]).unwrap_or_else(|| "-".to_string());
        let start = json_text_by_pointers(
            event,
            &["/eventDate/start", "/dateRange/start", "/originalStart"],
        )
        .unwrap_or_else(|| "-".to_string());
        let end =
            json_text_by_pointers(event, &["/eventDate/end", "/dateRange/end", "/originalEnd"])
                .unwrap_or_else(|| "-".to_string());

        println!("- {title} [{key}] ({start} -> {end})");
    }

    println!("\nTip: use --format json for machine-readable output.");
}

fn print_event_human(event: &Value) -> Result<(), CliError> {
    let title =
        json_text_by_pointers(event, &["/title"]).unwrap_or_else(|| "<untitled>".to_string());
    let key =
        json_text_by_pointers(event, &["/key", "/eventKey"]).unwrap_or_else(|| "-".to_string());
    let start = json_text_by_pointers(
        event,
        &["/eventDate/start", "/dateRange/start", "/originalStart"],
    )
    .unwrap_or_else(|| "-".to_string());
    let end = json_text_by_pointers(event, &["/eventDate/end", "/dateRange/end", "/originalEnd"])
        .unwrap_or_else(|| "-".to_string());

    println!("title: {title}");
    println!("key: {key}");
    println!("start: {start}");
    println!("end: {end}");
    println!("\nRaw event JSON:");
    println!("{}", render_pretty_json(event)?);

    Ok(())
}

fn print_events_mutation_human(output: &EventsMutationOutput) -> Result<(), CliError> {
    if let Some(event_id) = output.event_id.as_deref() {
        println!(
            "Applied {} event action for {}/{}.",
            output.operation, output.calendar_id, event_id
        );
    } else {
        println!(
            "Applied {} event action for calendar {}.",
            output.operation, output.calendar_id
        );
    }

    print_event_apply_human(&output.response)
}

fn print_event_apply_human(response: &Value) -> Result<(), CliError> {
    if let Some(results) = response.get("results").and_then(|value| value.as_array()) {
        if results.is_empty() {
            println!("No action results returned.");
            return Ok(());
        }

        for (index, result_item) in results.iter().enumerate() {
            let result = json_text_by_pointers(result_item, &["/result"])
                .unwrap_or_else(|| "UNKNOWN".to_string());
            let action_type = json_text_by_pointers(
                result_item,
                &["/action/action/type", "/action/type", "/type"],
            )
            .unwrap_or_else(|| "UnknownAction".to_string());
            let event_key = json_text_by_pointers(
                result_item,
                &[
                    "/action/action/eventKey",
                    "/action/eventKey",
                    "/action/action/key",
                    "/action/key",
                ],
            )
            .unwrap_or_else(|| "-".to_string());

            println!(
                "{}. {} | {} | {}",
                index + 1,
                result,
                action_type,
                event_key
            );
        }

        println!("\nTip: use --format json for full mutation response.");
        return Ok(());
    }

    println!("{}", render_pretty_json(response)?);
    Ok(())
}

fn json_text_by_pointers(value: &Value, pointers: &[&str]) -> Option<String> {
    for pointer in pointers {
        if let Some(candidate) = value
            .pointer(pointer)
            .filter(|candidate| !candidate.is_null())
        {
            return match candidate {
                Value::String(text) => Some(text.clone()),
                Value::Bool(flag) => Some(flag.to_string()),
                Value::Number(number) => Some(number.to_string()),
                Value::Array(_) | Value::Object(_) => Some(candidate.to_string()),
                Value::Null => None,
            };
        }
    }

    None
}

fn render_pretty_json(value: &Value) -> Result<String, CliError> {
    serde_json::to_string_pretty(value)
        .map_err(|error| CliError::Output(format!("Could not render JSON output: {error}")))
}

fn print_mutation_human(prefix: &str, task: &Task) {
    println!("{prefix} task #{}: {}", task.id, task.title);
    if let Some(status) = task.status.as_deref() {
        println!("Status: {status}");
    }
    if let Some(priority) = task.priority.as_deref() {
        println!("Priority: {priority}");
    }
    if let Some(due) = task.due.as_deref() {
        println!("Due: {due}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_set_value_supports_json_literals() {
        assert_eq!(parse_set_value("true"), json!(true));
        assert_eq!(parse_set_value("42"), json!(42));
        assert_eq!(parse_set_value("{\"nested\":1}"), json!({"nested": 1}));
        assert_eq!(parse_set_value("P4"), json!("P4"));
    }

    #[test]
    fn parse_json_object_argument_requires_object() {
        let error = parse_json_object_argument("[]", "--json").unwrap_err();
        assert!(error.to_string().contains("expected a JSON object"));
    }

    #[test]
    fn parse_set_entry_requires_equals_sign() {
        let error = parse_set_entry("priority").unwrap_err();
        assert!(error.to_string().contains("Expected KEY=VALUE"));
    }

    #[test]
    fn build_event_create_request_wraps_add_event_action() {
        let args = EventsCreateArgs {
            calendar_id: 829105,
            title: "Team sync".to_string(),
            start: "2026-02-21T18:30:00Z".to_string(),
            end: "2026-02-21T19:00:00Z".to_string(),
            policy_id: "00000000-0000-0000-0000-000000000000".to_string(),
            attendees: vec!["person@example.com".to_string()],
            description: None,
            location: None,
            priority: Some(crate::cli::Priority::P2),
            visibility: None,
            transparency: None,
            guests_can_modify: false,
            guests_can_invite_others: true,
            guests_can_see_other_guests: true,
            json: None,
            set: vec![],
        };

        let request = build_event_create_request(&args).unwrap();
        let action = request
            .pointer("/actionsTaken/0")
            .and_then(|value| value.as_object())
            .expect("action should exist");

        assert_eq!(
            action.get("type").and_then(|value| value.as_str()),
            Some("AddEventAction")
        );
        assert_eq!(
            action.get("calendarId").and_then(|value| value.as_u64()),
            Some(829105)
        );
    }

    #[test]
    fn build_event_update_request_requires_mutation_fields() {
        let args = EventsUpdateArgs {
            calendar_id: 829105,
            event_id: "abc123".to_string(),
            policy_id: "00000000-0000-0000-0000-000000000000".to_string(),
            title: None,
            description: None,
            location: None,
            priority: None,
            visibility: None,
            transparency: None,
            start: None,
            end: None,
            json: None,
            set: vec![],
        };

        let error = build_event_update_request(&args).unwrap_err();
        assert!(error
            .to_string()
            .contains("requires at least one field change"));
    }

    #[test]
    fn build_events_apply_request_requires_actions_taken() {
        let args = EventsApplyArgs {
            json: "{}".to_string(),
        };

        let error = build_events_apply_request(&args).unwrap_err();
        assert!(error.to_string().contains("actionsTaken is required"));
    }

    #[test]
    fn apply_task_completion_filter_matches_status_field() {
        let mut tasks = vec![
            Task {
                id: 1,
                title: "Plan roadmap".to_string(),
                status: Some("OPEN".to_string()),
                due: None,
                priority: None,
                notes: None,
                deleted: false,
                extra: std::collections::HashMap::new(),
            },
            Task {
                id: 2,
                title: "Archive docs".to_string(),
                status: Some("COMPLETED".to_string()),
                due: None,
                priority: None,
                notes: None,
                deleted: false,
                extra: std::collections::HashMap::new(),
            },
        ];

        apply_task_completion_filter(&mut tasks, Some(TaskCompletionFilter::Completed));
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, 2);
    }

    #[test]
    fn apply_task_completion_filter_matches_completion_status_extra_field() {
        let mut completed_extra = std::collections::HashMap::new();
        completed_extra.insert("completionStatus".to_string(), json!("COMPLETED"));

        let mut tasks = vec![
            Task {
                id: 123,
                title: "Prepare launch checklist".to_string(),
                status: Some("OPEN".to_string()),
                due: None,
                priority: None,
                notes: None,
                deleted: false,
                extra: completed_extra,
            },
            Task {
                id: 999,
                title: "Other item".to_string(),
                status: Some("OPEN".to_string()),
                due: None,
                priority: None,
                notes: None,
                deleted: false,
                extra: std::collections::HashMap::new(),
            },
        ];

        apply_task_completion_filter(&mut tasks, Some(TaskCompletionFilter::Completed));
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, 123);
    }

    #[test]
    fn apply_task_completion_filter_matches_boolean_extra_fields() {
        let mut completed_extra = std::collections::HashMap::new();
        completed_extra.insert("completed".to_string(), json!(true));

        let mut tasks = vec![
            Task {
                id: 123,
                title: "Plan".to_string(),
                status: Some("OPEN".to_string()),
                due: None,
                priority: None,
                notes: None,
                deleted: false,
                extra: completed_extra,
            },
            Task {
                id: 456,
                title: "Build".to_string(),
                status: Some("OPEN".to_string()),
                due: None,
                priority: None,
                notes: None,
                deleted: false,
                extra: std::collections::HashMap::new(),
            },
        ];

        apply_task_completion_filter(&mut tasks, Some(TaskCompletionFilter::Completed));
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, 123);
    }
}
