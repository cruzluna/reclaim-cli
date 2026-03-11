# reclaim-cli

CLI for interacting with Reclaim.ai. Manage tasks, events, and calendars from your terminal.

## Installation

### Install script

```bash
curl -fsSL https://raw.githubusercontent.com/cruzluna/reclaim-cli/main/install.sh | bash
```

Options:
- `RECLAIM_INSTALL_TAG` (default: `latest`)
- `RECLAIM_INSTALL_DIR` (default: `$HOME/.local/bin`)
- `RECLAIM_INSTALL_TARGET` (for manual target selection)

### Download release binary

1. Download the archive for your platform from [GitHub Releases](https://github.com/cruzluna/reclaim-cli/releases)
2. Extract and install:

```bash
tar -xzf reclaim-cli-*.tar.gz
install -m 0755 reclaim-cli-*/reclaim /usr/local/bin/reclaim
```

### Install from source

```bash
cargo install --path .
```

## Setup

```bash
export RECLAIM_API_KEY=your_api_key_here
```

## Quick start

```bash
reclaim list
reclaim list --filter open
reclaim list --filter completed
reclaim dashboard
reclaim get 123
reclaim create --title "Plan sprint"
reclaim patch 123 --set priority=P4
reclaim put 123 --set priority=P2
reclaim delete 123

# Events
reclaim events list --start 2026-02-01 --end 2026-02-28
reclaim events create --calendar-id 829105 --title "Team sync" \
  --start 2026-02-21T18:30:00Z --end 2026-02-21T19:00:00Z
```

## Commands

| Command | Description |
|---------|-------------|
| `reclaim list` | List tasks (filter: `open`, `completed`, `IN_PROGRESS`, etc.) |
| `reclaim get <ID>` | Get a task by ID |
| `reclaim create` | Create a new task |
| `reclaim patch <ID>` | Partially update a task |
| `reclaim put <ID>` | Fully replace a task |
| `reclaim delete <ID>` | Delete a task |
| `reclaim dashboard` | Interactive TUI |
| `reclaim events list` | List calendar events |
| `reclaim events get` | Get an event |
| `reclaim events create` | Create an event |
| `reclaim events update` | Update an event |
| `reclaim events delete` | Delete an event |

## Interactive dashboard

```bash
reclaim dashboard
```

Keyboard shortcuts (Vim-friendly):
- `j`/`k` or arrows: move selection
- `g`/`G`: jump to first/last task
- `?`: toggle help
- `r`: refresh
- `q`, `Esc`, `Ctrl+C`: quit

## JSON output

Use `--format json` for machine-readable output:

```bash
reclaim list --format json
reclaim get 123 --format json
```

## Field updates

Use `--set key=value` for partial updates:

```bash
reclaim patch 123 --set priority=P4 --set snoozeUntil=2026-02-25T17:00:00Z
```

Or `--json` for full objects:

```bash
reclaim put 123 --json '{"title":"New title","priority":"P2"}'
```

## Man page

Generate the man page:

```bash
cargo run --bin reclaim-man
```

This writes to `man/reclaim.1` by default. Release archives include it next to the binary.
