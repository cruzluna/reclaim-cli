use crate::{
    error::CliError,
    reclaim_api::{ReclaimApi, Task, TaskFilter},
};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame, Terminal,
};
use std::{
    cmp,
    io::{self, Stdout},
    time::Duration,
};

const POLL_INTERVAL: Duration = Duration::from_millis(200);
const DASHBOARD_HINT: &str = "j/k move  g/G jump  r refresh  ? help  :q/Esc/Ctrl+C quit";

type DashboardTerminal = Terminal<CrosstermBackend<Stdout>>;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum AppAction {
    None,
    Quit,
    Refresh,
}

#[derive(Debug)]
struct DashboardApp {
    tasks: Vec<Task>,
    filter: TaskFilter,
    list_state: ListState,
    show_help: bool,
    command_buffer: String,
    status_message: Option<String>,
}

impl DashboardApp {
    fn new(tasks: Vec<Task>, filter: TaskFilter) -> Self {
        let mut list_state = ListState::default();
        if !tasks.is_empty() {
            list_state.select(Some(0));
        }

        Self {
            tasks,
            filter,
            list_state,
            show_help: false,
            command_buffer: String::new(),
            status_message: None,
        }
    }

    fn selected_index(&self) -> Option<usize> {
        self.list_state.selected()
    }

    fn selected_task(&self) -> Option<&Task> {
        self.selected_index().and_then(|idx| self.tasks.get(idx))
    }

    fn replace_tasks(&mut self, tasks: Vec<Task>) {
        self.tasks = tasks;
        if self.tasks.is_empty() {
            self.list_state.select(None);
        } else {
            let selected = self.selected_index().unwrap_or(0);
            let new_index = cmp::min(selected, self.tasks.len() - 1);
            self.list_state.select(Some(new_index));
        }

        let count = self.tasks.len();
        self.status_message = Some(format!(
            "Refreshed: {count} task{} loaded.",
            if count == 1 { "" } else { "s" }
        ));
    }

    fn set_status(&mut self, message: impl Into<String>) {
        self.status_message = Some(message.into());
    }

    fn select_next(&mut self) {
        if self.tasks.is_empty() {
            self.list_state.select(None);
            return;
        }

        let next = match self.selected_index() {
            Some(index) if index + 1 < self.tasks.len() => index + 1,
            _ => 0,
        };
        self.list_state.select(Some(next));
    }

    fn select_previous(&mut self) {
        if self.tasks.is_empty() {
            self.list_state.select(None);
            return;
        }

        let previous = match self.selected_index() {
            Some(0) | None => self.tasks.len() - 1,
            Some(index) => index - 1,
        };
        self.list_state.select(Some(previous));
    }

    fn select_first(&mut self) {
        if self.tasks.is_empty() {
            self.list_state.select(None);
        } else {
            self.list_state.select(Some(0));
        }
    }

    fn select_last(&mut self) {
        if self.tasks.is_empty() {
            self.list_state.select(None);
        } else {
            self.list_state.select(Some(self.tasks.len() - 1));
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        if is_quit_key(key) {
            return AppAction::Quit;
        }

        if !self.command_buffer.is_empty() {
            return self.handle_command_key(key);
        }

        if self.show_help {
            if matches!(key.code, KeyCode::Char('?') | KeyCode::Enter) {
                self.show_help = false;
            }
            return AppAction::None;
        }

        match key.code {
            KeyCode::Char('?') => {
                self.show_help = true;
                self.status_message = Some("Help opened. Press ? or Enter to close.".to_string());
                AppAction::None
            }
            KeyCode::Char(':') => {
                self.command_buffer = ":".to_string();
                self.status_message = Some("Command mode: type :q to quit.".to_string());
                AppAction::None
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.select_next();
                AppAction::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.select_previous();
                AppAction::None
            }
            KeyCode::Char('g') | KeyCode::Home => {
                self.select_first();
                AppAction::None
            }
            KeyCode::Char('G') | KeyCode::End => {
                self.select_last();
                AppAction::None
            }
            KeyCode::Char('r') => AppAction::Refresh,
            _ => AppAction::None,
        }
    }

    fn handle_command_key(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Enter => {
                let command = self.command_buffer.clone();
                self.command_buffer.clear();
                if command == ":q" {
                    AppAction::Quit
                } else if command == ":" {
                    self.status_message = Some("Command cancelled.".to_string());
                    AppAction::None
                } else {
                    self.status_message = Some(format!("Unknown command: {command}"));
                    AppAction::None
                }
            }
            KeyCode::Backspace => {
                self.command_buffer.pop();
                if self.command_buffer.is_empty() {
                    self.status_message = Some(DASHBOARD_HINT.to_string());
                }
                AppAction::None
            }
            KeyCode::Char(ch)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.command_buffer.push(ch);
                if self.command_buffer == ":q" {
                    return AppAction::Quit;
                }
                AppAction::None
            }
            _ => AppAction::None,
        }
    }
}

pub async fn run_dashboard(api: &impl ReclaimApi, include_all: bool) -> Result<(), CliError> {
    let filter = if include_all {
        TaskFilter::All
    } else {
        TaskFilter::Active
    };
    let tasks = api.list_tasks(filter).await?;
    let mut app = DashboardApp::new(tasks, filter);

    let mut terminal = setup_terminal()?;
    let loop_result = run_event_loop(&mut terminal, api, &mut app).await;
    let restore_result = restore_terminal(&mut terminal);

    match (loop_result, restore_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(loop_error), Ok(())) => Err(loop_error),
        (Ok(()), Err(restore_error)) => Err(restore_error),
        (Err(loop_error), Err(restore_error)) => Err(CliError::Output(format!(
            "{loop_error}\nAlso failed to restore terminal state: {restore_error}"
        ))),
    }
}

async fn run_event_loop(
    terminal: &mut DashboardTerminal,
    api: &impl ReclaimApi,
    app: &mut DashboardApp,
) -> Result<(), CliError> {
    loop {
        terminal
            .draw(|frame| draw_dashboard(frame, app))
            .map_err(|error| map_tui_error("Failed to draw dashboard frame", error))?;

        if !event::poll(POLL_INTERVAL).map_err(|error| map_tui_error("TUI poll failed", error))? {
            continue;
        }

        let event = event::read().map_err(|error| map_tui_error("TUI event read failed", error))?;
        if let Event::Key(key) = event {
            if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                continue;
            }

            match app.handle_key(key) {
                AppAction::None => {}
                AppAction::Quit => return Ok(()),
                AppAction::Refresh => match api.list_tasks(app.filter).await {
                    Ok(tasks) => app.replace_tasks(tasks),
                    Err(error) => {
                        let summary = error.to_string();
                        let first_line = summary.lines().next().unwrap_or("Refresh failed.");
                        app.set_status(format!("Refresh failed: {first_line}"));
                    }
                },
            }
        }
    }
}

fn draw_dashboard(frame: &mut Frame<'_>, app: &mut DashboardApp) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(8),
            Constraint::Length(2),
        ])
        .split(frame.area());

    draw_header(frame, app, layout[0]);
    draw_body(frame, app, layout[1]);
    draw_footer(frame, app, layout[2]);

    if app.show_help {
        draw_help_popup(frame);
    }
}

fn draw_header(frame: &mut Frame<'_>, app: &DashboardApp, area: Rect) {
    let filter_label = match app.filter {
        TaskFilter::Active => "active",
        TaskFilter::All => "all",
    };

    let header = Paragraph::new(Line::from(vec![
        Span::styled(
            "Reclaim Task Dashboard",
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(
            "  |  {} task{} ({filter_label})",
            app.tasks.len(),
            if app.tasks.len() == 1 { "" } else { "s" }
        )),
    ]));
    frame.render_widget(header, area);
}

fn draw_body(frame: &mut Frame<'_>, app: &mut DashboardApp, area: Rect) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);

    let items: Vec<ListItem<'_>> = if app.tasks.is_empty() {
        vec![ListItem::new("No tasks found for this filter.")]
    } else {
        app.tasks
            .iter()
            .map(|task| {
                let status = task.status.as_deref().unwrap_or("UNKNOWN");
                let due = task.due.as_deref().unwrap_or("-");
                ListItem::new(format!(
                    "#{:<6} [{:<10}] {} (due: {due})",
                    task.id, status, task.title
                ))
            })
            .collect()
    };

    let tasks_list = List::new(items)
        .block(Block::default().title("Tasks").borders(Borders::ALL))
        .highlight_symbol(">> ")
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_stateful_widget(tasks_list, columns[0], &mut app.list_state);

    let details = Paragraph::new(selected_task_lines(app))
        .block(Block::default().title("Details").borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    frame.render_widget(details, columns[1]);
}

fn draw_footer(frame: &mut Frame<'_>, app: &DashboardApp, area: Rect) {
    let text = if !app.command_buffer.is_empty() {
        format!("Command: {}", app.command_buffer)
    } else if let Some(status) = app.status_message.as_deref() {
        status.to_string()
    } else {
        DASHBOARD_HINT.to_string()
    };

    let footer = Paragraph::new(text).block(Block::default().borders(Borders::TOP));
    frame.render_widget(footer, area);
}

fn draw_help_popup(frame: &mut Frame<'_>) {
    let area = centered_rect(72, 70, frame.area());
    frame.render_widget(Clear, area);

    let help_lines = vec![
        Line::from(Span::styled(
            "Dashboard key bindings",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("Navigation"),
        Line::from("  j / Down        Move down"),
        Line::from("  k / Up          Move up"),
        Line::from("  g / Home        Jump to first task"),
        Line::from("  G / End         Jump to last task"),
        Line::from(""),
        Line::from("Actions"),
        Line::from("  r               Refresh tasks from API"),
        Line::from("  ?               Toggle this help"),
        Line::from(""),
        Line::from("Exit"),
        Line::from("  :q              Vim-style quit command"),
        Line::from("  Esc             Quit immediately"),
        Line::from("  Ctrl+C          Quit immediately"),
        Line::from(""),
        Line::from("Press ? or Enter to close this panel."),
    ];

    let help = Paragraph::new(help_lines)
        .block(Block::default().title("Help").borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    frame.render_widget(help, area);
}

fn selected_task_lines(app: &DashboardApp) -> Vec<Line<'static>> {
    let Some(task) = app.selected_task() else {
        return vec![
            Line::from("No task selected."),
            Line::from("Try pressing r to refresh from the API."),
        ];
    };

    let mut lines = vec![
        Line::from(format!("#{} {}", task.id, task.title)),
        Line::from(format!(
            "status: {}",
            task.status.as_deref().unwrap_or("UNKNOWN")
        )),
        Line::from(format!(
            "priority: {}",
            task.priority.as_deref().unwrap_or("-")
        )),
        Line::from(format!("due: {}", task.due.as_deref().unwrap_or("-"))),
    ];

    if let Some(notes) = task
        .notes
        .as_deref()
        .filter(|notes| !notes.trim().is_empty())
    {
        lines.push(Line::from(""));
        lines.push(Line::from("notes:"));
        for line in notes.lines() {
            lines.push(Line::from(format!("  {line}")));
        }
    }

    lines
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

fn setup_terminal() -> Result<DashboardTerminal, CliError> {
    enable_raw_mode()
        .map_err(|error| map_tui_error("Failed to enable raw terminal mode", error))?;

    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)
        .map_err(|error| map_tui_error("Failed to enter alternate screen", error))?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)
        .map_err(|error| map_tui_error("Failed to create terminal", error))?;
    terminal
        .clear()
        .map_err(|error| map_tui_error("Failed to clear terminal", error))?;
    Ok(terminal)
}

fn restore_terminal(terminal: &mut DashboardTerminal) -> Result<(), CliError> {
    disable_raw_mode().map_err(|error| map_tui_error("Failed to disable raw mode", error))?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .map_err(|error| map_tui_error("Failed to leave alternate screen", error))?;
    terminal
        .show_cursor()
        .map_err(|error| map_tui_error("Failed to show cursor", error))?;
    Ok(())
}

fn is_quit_key(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Esc)
        || (key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C')))
}

fn map_tui_error(context: &str, error: io::Error) -> CliError {
    CliError::Output(format!("{context}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn test_task(id: u64, title: &str) -> Task {
        Task {
            id,
            title: title.to_string(),
            status: Some("NEW".to_string()),
            due: Some("2026-02-23T17:00:00Z".to_string()),
            priority: Some("P3".to_string()),
            notes: Some("note".to_string()),
            deleted: false,
            extra: HashMap::new(),
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn esc_key_quits_dashboard() {
        let mut app = DashboardApp::new(vec![test_task(1, "One")], TaskFilter::Active);
        assert_eq!(app.handle_key(key(KeyCode::Esc)), AppAction::Quit);
    }

    #[test]
    fn ctrl_c_quits_dashboard() {
        let mut app = DashboardApp::new(vec![test_task(1, "One")], TaskFilter::Active);
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(app.handle_key(ctrl_c), AppAction::Quit);
    }

    #[test]
    fn colon_q_quits_dashboard() {
        let mut app = DashboardApp::new(vec![test_task(1, "One")], TaskFilter::Active);
        assert_eq!(app.handle_key(key(KeyCode::Char(':'))), AppAction::None);
        assert_eq!(app.handle_key(key(KeyCode::Char('q'))), AppAction::Quit);
    }

    #[test]
    fn question_mark_opens_help() {
        let mut app = DashboardApp::new(vec![test_task(1, "One")], TaskFilter::Active);
        assert_eq!(app.handle_key(key(KeyCode::Char('?'))), AppAction::None);
        assert!(app.show_help);
    }

    #[test]
    fn j_and_k_navigate_task_list() {
        let mut app = DashboardApp::new(
            vec![
                test_task(1, "One"),
                test_task(2, "Two"),
                test_task(3, "Three"),
            ],
            TaskFilter::Active,
        );

        app.handle_key(key(KeyCode::Char('j')));
        assert_eq!(app.selected_index(), Some(1));
        app.handle_key(key(KeyCode::Char('j')));
        assert_eq!(app.selected_index(), Some(2));
        app.handle_key(key(KeyCode::Char('k')));
        assert_eq!(app.selected_index(), Some(1));
    }
}
