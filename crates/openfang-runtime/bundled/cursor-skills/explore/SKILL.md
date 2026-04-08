---
name: explore
version: "1.0.0"
description: >-
  Read-only codebase exploration: map layout, key modules, and architecture
  for pipeline “explore” steps. Use with Cursor `--mode ask`.
---

# Explore (read-only)

## Expected Cursor CLI (via OpenFang `trigger_cursor_worker`)

- **Command shape**: `cursor agent -d <workspace> -p '<prompt>' --mode ask -o json`
- **Flags**: OpenFang always passes `-o json`. Do not rely on other flags unless the orchestrator adds allowlisted `flags`.
- **Workspace**: Spoke root (`-d`); all paths are relative to this root unless absolute inside the repo.

## Inputs (orchestrator puts these in `prompt` / `behavior`)

- **Goal**: what to map (e.g. “where is backlog API handled?”, “list crates touching workflows”).
- **Scope**: optional subpaths or exclusions.
- **`behavior`**: should include a line such as:  
  `Follow the contract in .cursor/skills/explore/SKILL.md in this workspace.`

## Rules

- **Read-only**: do not edit, create, or delete files; do not run destructive commands.
- Prefer **listing, search, and reading** to build a structured picture (trees, module boundaries, entrypoints).
- Respect `.cursorignore` and `.cursor/rules`.

## Output

- Concise **map** of relevant directories and files (markdown or bullet list).
- **Entrypoints** and **data flow** notes where applicable.
- Explicit **unknowns** if something cannot be confirmed without writes or runtime.
