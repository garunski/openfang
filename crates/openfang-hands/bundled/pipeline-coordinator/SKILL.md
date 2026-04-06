---
name: pipeline-coordinator-skill
version: "1.0.0"
description: "Automation pipeline — backlog lifecycle, Cursor worker, quality gate, retries, rollback"
runtime: prompt_only
---

# Pipeline coordinator

## Flow (doc-driven)

1. Resolve **task id** and **spoke** vs **backlog** roots (`repo:<spoke>` routing when applicable).
2. `backlog_task_view` → plan from description and AC.
3. `backlog_task_edit` → **In Progress**.
4. Prefer **`run_pipeline`** (`task_id`, `prompt`, `workspace` or `task_labels` with `repo:<spoke>`) so retries are enforced in Rust; or manually: `trigger_cursor_worker` then `enforce_quality_gate` on the same allowlisted spoke.
5. Gate **OK** → `backlog_task_edit` → **Done**. If not using `run_pipeline`: gate **fail** → retry Cursor with gate **stderr** in the prompt, up to **`max_retries`** from `[automation].max_retries` or project `pipeline_overrides` (default **2**).
6. Retries exhausted → `backlog_task_edit` → **Ready for Dev** or **New** (never Done without exit_code 0 from the gate).

## Forbidden

- Setting status **Done** without a passing gate (`run_pipeline` result or latest `enforce_quality_gate` with `exit_code` 0).

## Tools (reference)

- `backlog_task_create`, `backlog_task_list`, `backlog_task_view`, `backlog_task_edit`
- `run_pipeline` — `task_id`, `prompt`, `workspace` or `task_labels`, optional `max_retries`, `project_id`, `rollback_to_status`
- `trigger_cursor_worker` — `workspace`, `prompt`, optional `mode`, `behavior`, `flags`
- `enforce_quality_gate` — `spoke_root`

Optional on backlog tools: `backlog_root` when multiple backlog roots are configured.
