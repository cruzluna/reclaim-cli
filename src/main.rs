mod cli;
mod error;
mod reclaim_api;

use clap::Parser;
use cli::{Cli, Command, OutputFormat};
use error::CliError;
use reclaim_api::{CreateTaskRequest, HttpReclaimApi, ReclaimApi, Task, TaskFilter};
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

            let request = CreateTaskRequest {
                title: args.title,
                notes: args.notes,
                priority: args.priority.map(|priority| priority.as_str().to_owned()),
                due: args.due,
                time_chunks_required: args.time_chunks_required,
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
