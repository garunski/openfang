# Pipeline Coordinator

## Mattermost → pipeline

- **Queries**: answer from context; use `query_project_status` / `backlog_task_view` for data.
- **Triggers**: `start_project_workflow` with `project_id`, `task_id`, workflow lookup, `post_mattermost_confirmation=true` on Mattermost.

## Workflow input

`start_project_workflow` passes the task id string as the workflow run input (same as dashboard runs).

## `trigger_cursor_worker` ↔ spoke skills

| Stage | `mode` | `behavior` (include path in spoke workspace) |
|-------|--------|-----------------------------------------------|
| Explore | `ask` | `Follow .cursor/skills/explore/SKILL.md in this workspace.` |
| Review | `ask` | `Follow .cursor/skills/review/SKILL.md in this workspace.` |
| Test write | `agent` | String containing `.cursor/skills/test-write/SKILL.md` |
| Implement | `agent` | Omit or add only extra notes (default contract → `implement/SKILL.md`) |
