---
name: implement
version: "1.0.0"
description: >-
  Headless Cursor agent implementation contract: read task from disk, satisfy AC,
  run mise quality gate locally, respect Cursor rules and ignore files.
---

# Implement (Cursor agent contract)

Use this skill on **every** headless `cursor agent` run for full read/write implementation (OpenFang `mode` `agent` omits `--mode` on the CLI; Cursor runs with write-capable tools). The task summary in the user prompt is not enough; follow this process end-to-end.

## Objective

Turn the described backlog task into merged code (or repo artifacts) that **passes** the spoke quality gate.

## Inputs

- **Task source**: A filesystem path to the task markdown (preferred) and/or task id plus repo context from the orchestrator prompt.
- **Workspace**: The spoke root (`--workspace` / OpenFang `workspace` parameter). All commands run from here unless stated otherwise.

## Process

1. **Read the full task** from disk using the path given in the prompt (or resolve the path from task id + backlog layout). Parse description, acceptance criteria, and plan sections.
2. **Implement** only what the task requires. Match existing project patterns; keep diffs focused.
3. **Before claiming done**, from the workspace root run:
   ```bash
   mise run 001-qa
   ```
   Fix failures until it exits 0 (or document blockers if the task explicitly allows partial delivery — otherwise keep iterating).
4. **Respect**:
   - `.cursor/rules` — treat as hard constraints for style, safety, and workflow.
   - `.cursorignore` — do not read or rely on ignored paths unless the task explicitly requires it and documents why.

## Output expectations

- Concrete file changes (no hand-wavy “should work”).
- All acceptance criteria addressed or explicitly called out with evidence.
- Quality gate green locally (`mise run 001-qa`).
