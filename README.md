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
```

Use `--format json` when output should be machine-readable:

```bash
cargo run -- list --format json
```

## API notes

This foundation is based on observed usage from:
https://github.com/johnjhughes/reclaim-mcp-server

In particular:
- Base URL: `https://api.app.reclaim.ai/api`
- Task endpoints: `/tasks`, `/tasks/{id}`
