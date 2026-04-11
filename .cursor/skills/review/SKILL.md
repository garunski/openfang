---
name: review
version: "1.0.0"
description: >-
  Read-only code review: diffs, logic, style, tests, and security notes.
  Use with Cursor `--mode ask`.
---

# Review (read-only)

## Expected Cursor CLI (via OpenFang `trigger_cursor_worker`)

- **Command shape**: `cursor agent --print --output-format json --trust --workspace <workspace> --mode ask -- '<prompt>'`
- **Flags**: Optional allowlisted extras from the orchestrator (`--yolo`, `--force`).
- **Workspace**: Spoke root (`--workspace`).

## Inputs

- **Diff or scope**: patch, branch comparison, file list, or “review commits X–Y” as given in `prompt`.
- **Focus**: e.g. correctness, edge cases, naming, test gaps, security-sensitive paths.
- **`behavior`**: include:  
  `Follow the contract in .cursor/skills/review/SKILL.md in this workspace.`

## Rules

- **Read-only**: no code edits, no formatting fixes, no running fixers that change files.
- Call out **bugs**, **regressions**, **missing tests**, **style** issues, and **security** concerns with severity (blocker / major / minor / nit).
- If diff is missing, say what you need; do not invent changes.

## Output

- **Summary** (what changed and overall risk).
- **Findings** grouped by severity with file/line references when possible.
- **Suggested tests** (descriptions only; do not implement unless a separate `test-write` run is requested).
