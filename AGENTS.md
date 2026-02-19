# AGENTS.md

Guidance for humans and agents using this CLI.

## Principles

1. **Keep commands simple**
   - Prefer short, obvious flows (`list`, `get`, `create`).
2. **Default to clear help**
   - `reclaim --help` should show practical examples and required setup.
3. **Return fixable errors**
   - Errors should include a plain-language cause and a direct hint to fix it.
4. **Support machine-readable output**
   - Use `--format json` when another tool/agent is consuming output.
5. **Avoid hidden magic**
   - Keep behavior explicit and predictable over cleverness.

## Recommended agent usage

- Set credentials via environment variable:
  - `RECLAIM_API_KEY=...`
- Use JSON output:
  - `reclaim list --format json`
  - `reclaim get 123 --format json`
