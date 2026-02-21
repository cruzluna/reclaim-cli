mod cli;
mod error;
mod reclaim_api;

use clap::Parser;
use cli::{Cli, Command, OutputFormat, PatchArgs, PutArgs};
use error::CliError;
use reclaim_api::{CreateTaskRequest, HttpReclaimApi, ReclaimApi, Task, TaskFilter};
use serde_json::{Map, Value};
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
            let tasks = api.list_tasks(filter).await?;

            match format {
                OutputFormat::Json => print_json(&tasks)?,
                OutputFormat::Human => print_task_list_human(args.all, &tasks),
            }
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

fn print_task_list_human(includes_all: bool, tasks: &[Task]) {
    if tasks.is_empty() {
        if includes_all {
            println!("No tasks found.");
        } else {
            println!("No active tasks found.");
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
}
