OpenFang scoped trace logs
============================

This directory is managed automatically by the daemon.

Layout
------
- `system.log` — process-wide trace (hourly rotation).
- `projects/<UUID>.log` — per registered project.
- `agents/<UUID>.log` — per agent id.
- `archive/` — closed hourly segments named `{prefix}_{YYYYmmddHH}.log`.

Retention
---------
Archive files older than **48 hours** (by hour bucket in the filename) are deleted on boot and every hour.

Viewing
-------
Use the dashboard **Logs → Files** tab, or `GET /api/logs/files` for keys `trace_system`, `trace_prj_*`, `trace_agt_*`.
