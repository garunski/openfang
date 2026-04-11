---
name: test-write
version: "1.0.0"
description: >-
  Author or extend automated tests for described code changes. Use with
  Cursor `--mode agent` and OpenFang `trigger_cursor_worker` when the
  prompt references this skill path.
---

# Test write (`--mode agent`)

## Expected Cursor CLI (via OpenFang `trigger_cursor_worker`)

- **Command shape**: `cursor agent --print --output-format json --trust --workspace <workspace> -- '<prompt>'` (no `--mode` for full read/write agent runs)
- **Flags**: Optional allowlisted extras from the orchestrator: `--yolo` / `--force`.
- **Workspace**: Spoke root (`--workspace`).

## Inputs

- **What to cover**: functions, modules, or scenarios from the orchestrator `prompt` (often tied to a backlog task or diff).
- **`behavior`**: must reference this file so OpenFang swaps in the test contract, e.g.:  
  `Follow .cursor/skills/test-write/SKILL.md in this workspace.`

## Rules

- **Tests only** by default: add/update test files and minimal production hooks **only** if required for testability (e.g. `pub(crate)` visibility). Do not implement unrelated features.
- Match existing **test framework** and patterns in the repo (Rust `cargo test`, JS test runner, etc.).
- Before finishing, run the **project’s standard test/lint path** from the workspace root (e.g. `mise run 001-qa` when present, or `cargo test` / `npm test` as documented in the repo).
- Honor `.cursorignore` and `.cursor/rules`.

## Output

- List of **files changed** (tests + any tiny prod edits).
- How to **run** the new/updated tests.
- Any **gaps** left uncovered.
