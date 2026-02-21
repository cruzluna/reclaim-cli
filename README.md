# reclaim-cli

Foundational Rust CLI for interacting with Reclaim.ai.

## What this includes

- Binary executable named **`reclaim`**
- Clear CLI/API separation in code:
  - `src/cli.rs` for command parsing and help text
  - `src/reclaim_api.rs` for Reclaim API abstraction + HTTP implementation
  - `src/error.rs` for actionable errors with fix hints
- Foundational commands:
  - `reclaim list`
  - `reclaim get <TASK_ID>`
  - `reclaim create --title "..." [options]`
  - `reclaim put <TASK_ID> --json '{...}'` or `--set key=value`
  - `reclaim patch <TASK_ID> --json '{...}'` and/or `--set key=value`
  - `reclaim delete <TASK_ID>`

## Installation

### Option 0: Install script

```bash
curl -fsSL https://raw.githubusercontent.com/cruzluna/reclaim-cli/main/install.sh | bash
```

Useful overrides:
- `RECLAIM_INSTALL_TAG` (default: `latest`)
- `RECLAIM_INSTALL_DIR` (default: `$HOME/.local/bin`)
- `RECLAIM_INSTALL_TARGET` (for manual target selection)

### Option 1: Download a release binary (recommended)

1. Go to GitHub Releases and download the archive for your platform:
   - `reclaim-cli-<target>.tar.gz`
2. Extract it:

```bash
tar -xzf reclaim-cli-<target>.tar.gz
```

3. Move the `reclaim` binary into your `PATH`:

```bash
install -m 0755 reclaim-cli-<target>/reclaim /usr/local/bin/reclaim
```

### Option 2: Install from source

```bash
git clone https://github.com/cruzluna/reclaim-cli.git
cd reclaim-cli
cargo install --path .
```

## Quick start

1. Set API key:

```bash
export RECLAIM_API_KEY=your_api_key_here
```

2. Run commands:

```bash
cargo run -- list
cargo run -- get 123
cargo run -- create --title "Plan sprint"
cargo run -- patch 123 --set priority=P4 --set snoozeUntil=2026-02-25T17:00:00Z
cargo run -- put 123 --set priority=P2
cargo run -- delete 123
```

Use `--format json` when output should be machine-readable:

```bash
cargo run -- list --format json
```

Use `--json` and `--set key=value` on `put`/`patch` for agent-friendly updates:

```bash
# Partial update (PATCH)
cargo run -- patch 123 \
  --set priority=P4 \
  --set snoozeUntil=2026-02-25T17:00:00Z \
  --format json

# Full replace (PUT) using a JSON object
cargo run -- put 123 --json '{"title":"Plan sprint","priority":"P2"}' --format json
```

## Man page

Generate `reclaim(1)` from the clap CLI definition:

```bash
cargo run --bin reclaim-man
```

By default this writes `man/reclaim.1`. To choose a different destination:

```bash
cargo run --bin reclaim-man -- --output /tmp/reclaim.1
```

Release archives also include `reclaim.1` next to the binary.

## API notes

This foundation is based on observed usage from:
https://github.com/johnjhughes/reclaim-mcp-server

In particular:
- Base URL: `https://api.app.reclaim.ai/api`
- Task endpoints: `/tasks`, `/tasks/{id}` (`GET`, `PUT`, `PATCH`, `DELETE`)
