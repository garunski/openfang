# Conduit coordinator

## Mattermost → conduit

- **Queries**: answer from context; use `query_project_status`, `backlog_task_view` (with **`project_id`**), or `resolve_conduit_context` for a single JSON bundle.
- **Triggers**: `start_project_conduit` with `project_id`, `task_id`, **`conduit_name`** (e.g. `conduit-doc-to-tasks`, `conduit-full-cycle`), optional `conduit_id`, and `post_mattermost_confirmation=true` on Mattermost.

## Conduit run payload (dashboard / `start_project_conduit`)

The first user message includes:

- **`[OpenFang conduit binding]`** — `project_id`, `trigger_task_id`, optional Mattermost id, `admin_spoke`, `configured_spokes` (configured paths may be relative to the project record).
- **`[OpenFang resolved paths]`** — authoritative **absolute** `admin_backlog_root` and `spoke.<name>=…` (do not guess or ask the user).
- **`[OpenFang trigger task snapshot]`** — plaintext from **`backlog task <trigger_task_id> --plain`** when the run was prepared (parse description, doc refs, labels here).
- Optional **accumulated project context** from `context.json` (trimmed).

Treat binding + resolved paths + snapshot as authoritative. Call `read_project_context` / `update_project_context` for **persisted** notes (repo layout, conventions, failures, decisions). Call `resolve_conduit_context` to **refresh** the same JSON shape plus an optional live **`trigger_task_snapshot`**.

## `trigger_cursor_worker` ↔ spoke skills

| Stage | `mode` | `behavior` (include path in spoke workspace) |
|-------|--------|-----------------------------------------------|
| Explore | `ask` | `Follow .cursor/skills/explore/SKILL.md in this workspace.` |
| Review | `ask` | `Follow .cursor/skills/review/SKILL.md in this workspace.` |
| Test write | `agent` | String containing `.cursor/skills/test-write/SKILL.md` |
| Implement | `agent` | Omit or add only extra notes (default contract → `implement/SKILL.md`) |
