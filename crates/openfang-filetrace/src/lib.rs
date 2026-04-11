//! Rolling scoped trace logs under `<home>/logs/trace/`.
//!
//! - **system** — `system.log` (hourly roll to `archive/`)
//! - **project** — `projects/<uuid>.log`
//! - **agent** — `agents/<uuid>.log`
//!
//! Rotated segments live in `logs/trace/archive/` as `{prefix}_{YYYYmmddHH}.log`.
//! Files in `archive/` older than [`RETENTION`] are deleted (best-effort).

mod paths;
mod prune;
mod writer;

pub use paths::{
    logs_dir, trace_agents_dir, trace_archive_dir, trace_projects_dir, trace_readme_path,
    trace_root, trace_system_log_path, TRACE_SUBDIR,
};

use chrono::{NaiveDate, Utc};
use dashmap::DashMap;
use std::sync::Mutex;
use prune::prune_archive_dir;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;
pub use writer::RotatingWriter;

/// Wall-clock retention for archived hourly segments (48 hours).
pub const RETENTION: std::time::Duration = std::time::Duration::from_secs(48 * 3600);

const SHIP_README: &str = include_str!("../assets/TRACE_LOGS_README.md");

/// Root layout: `<home>/logs/trace/`.
#[derive(Debug)]
pub struct ScopedFileTrace {
    root: PathBuf,
    archive_dir: PathBuf,
    projects_dir: PathBuf,
    agents_dir: PathBuf,
    system: Mutex<RotatingWriter>,
    projects: DashMap<String, Mutex<RotatingWriter>>,
    agents: DashMap<String, Mutex<RotatingWriter>>,
}

impl ScopedFileTrace {
    /// Create directories, write shipped README once, prune expired archives.
    pub fn new(home_dir: &Path) -> std::io::Result<Self> {
        let root = paths::trace_root(home_dir);
        let archive_dir = paths::trace_archive_dir(home_dir);
        let projects_dir = paths::trace_projects_dir(home_dir);
        let agents_dir = paths::trace_agents_dir(home_dir);
        std::fs::create_dir_all(&archive_dir)?;
        std::fs::create_dir_all(&projects_dir)?;
        std::fs::create_dir_all(&agents_dir)?;

        let readme = paths::trace_readme_path(home_dir);
        if !readme.exists() {
            let _ = std::fs::write(&readme, SHIP_README);
        }

        let system_path = paths::trace_system_log_path(home_dir);
        let system = Mutex::new(RotatingWriter::new(
            system_path,
            archive_dir.clone(),
            "sys".to_string(),
        ));

        let t = Self {
            root,
            archive_dir,
            projects_dir,
            agents_dir,
            system,
            projects: DashMap::new(),
            agents: DashMap::new(),
        };
        let _ = t.prune_expired();
        Ok(t)
    }

    #[inline]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// System-wide trace lines (daemon lifecycle, cross-cutting events).
    pub fn system(&self) -> TraceSink<'_> {
        TraceSink::System(&self.system)
    }

    /// Per-project trace. `project_id` must be a valid UUID string.
    pub fn project(&self, project_id: &str) -> Option<TraceSink<'_>> {
        let id = parse_uuid_strict(project_id)?;
        let key = id.to_string();
        self.projects
            .entry(key.clone())
            .or_insert_with(|| {
                Mutex::new(RotatingWriter::new(
                    self.projects_dir.join(format!("{key}.log")),
                    self.archive_dir.clone(),
                    format!("prj_{}", id.as_simple()),
                ))
            });
        Some(TraceSink::Project {
            map: &self.projects,
            key,
        })
    }

    /// Per-agent trace. `agent_id` must be a valid UUID string.
    pub fn agent(&self, agent_id: &str) -> Option<TraceSink<'_>> {
        let id = parse_uuid_strict(agent_id)?;
        let key = id.to_string();
        self.agents
            .entry(key.clone())
            .or_insert_with(|| {
                Mutex::new(RotatingWriter::new(
                    self.agents_dir.join(format!("{key}.log")),
                    self.archive_dir.clone(),
                    format!("agt_{}", id.as_simple()),
                ))
            });
        Some(TraceSink::Agent {
            map: &self.agents,
            key,
        })
    }

    /// Remove archive segments older than [`RETENTION`]. Returns deleted byte count (approx).
    pub fn prune_expired(&self) -> std::io::Result<u64> {
        prune_archive_dir(&self.archive_dir, RETENTION)
    }
}

fn parse_uuid_strict(s: &str) -> Option<Uuid> {
    let t = s.trim();
    if t.is_empty() || t.len() > 64 {
        return None;
    }
    Uuid::parse_str(t).ok()
}

/// Ergonomic writer for one scope.
pub enum TraceSink<'a> {
    System(&'a Mutex<RotatingWriter>),
    Project {
        map: &'a DashMap<String, Mutex<RotatingWriter>>,
        key: String,
    },
    Agent {
        map: &'a DashMap<String, Mutex<RotatingWriter>>,
        key: String,
    },
}

impl TraceSink<'_> {
    /// One human-readable line: RFC3339 ms + level + message.
    pub fn line(&self, level: &str, message: &str) {
        let ts = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let level = level.to_ascii_uppercase();
        let mut buf = String::with_capacity(ts.len() + level.len() + message.len() + 8);
        buf.push_str(&ts);
        buf.push_str(" [");
        buf.push_str(&level);
        buf.push_str("] ");
        buf.push_str(message);
        let _ = self.write_raw_line(&buf);
    }

    pub fn debug(&self, message: &str) {
        self.line("DEBUG", message);
    }
    pub fn info(&self, message: &str) {
        self.line("INFO", message);
    }
    pub fn warn(&self, message: &str) {
        self.line("WARN", message);
    }
    pub fn error(&self, message: &str) {
        self.line("ERROR", message);
    }

    /// Single JSON line (newline appended). Prefer for structured tracing.
    pub fn json_line(&self, value: &serde_json::Value) {
        if let Ok(s) = serde_json::to_string(value) {
            let _ = self.write_raw_line(&s);
        }
    }

    fn write_raw_line(&self, line: &str) -> std::io::Result<()> {
        match self {
            TraceSink::System(m) => m
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .append_line(line),
            TraceSink::Project { map, key } => {
                if let Some(entry) = map.get(key) {
                    entry
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .append_line(line)?;
                }
                Ok(())
            }
            TraceSink::Agent { map, key } => {
                if let Some(entry) = map.get(key) {
                    entry
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .append_line(line)?;
                }
                Ok(())
            }
        }
    }
}

/// Background thread: prune archive hourly (daemon may boot outside a Tokio runtime).
pub fn spawn_retention_task(trace: Arc<ScopedFileTrace>) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(3600));
            if let Err(e) = trace.prune_expired() {
                tracing::warn!(error = %e, "scoped trace archive prune failed");
            }
        }
    });
}

// --- API / dashboard helpers (allowlisted keys, no path traversal) ---

/// `trace_system` or `trace_prj_<uuid_underscored>` or `trace_agt_<uuid_underscored>`.
pub fn resolve_trace_log_key(home: &Path, key: &str) -> Option<PathBuf> {
    if !key
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return None;
    }
    if key == "trace_system" {
        return Some(paths::trace_system_log_path(home));
    }
    if let Some(rest) = key.strip_prefix("trace_prj_") {
        let uuid_str = rest.replace('_', "-");
        let id = Uuid::parse_str(&uuid_str).ok()?;
        return Some(paths::trace_projects_dir(home).join(format!("{id}.log")));
    }
    if let Some(rest) = key.strip_prefix("trace_agt_") {
        let uuid_str = rest.replace('_', "-");
        let id = Uuid::parse_str(&uuid_str).ok()?;
        return Some(paths::trace_agents_dir(home).join(format!("{id}.log")));
    }
    None
}

fn uuid_to_api_key_segment(id: &Uuid) -> String {
    id.to_string().replace('-', "_")
}

/// Entries for `GET /api/logs/files` — `(key, label)`.
pub fn trace_log_catalog_entries(home: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    out.push((
        "trace_system".to_string(),
        "Trace: system (rolling, 48h archive)".to_string(),
    ));

    if let Ok(rd) = std::fs::read_dir(paths::trace_projects_dir(home)) {
        for ent in rd.flatten() {
            let path = ent.path();
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            let Some(stem) = name.strip_suffix(".log") else {
                continue;
            };
            let Ok(id) = Uuid::parse_str(stem) else {
                continue;
            };
            let key = format!("trace_prj_{}", uuid_to_api_key_segment(&id));
            out.push((
                key,
                format!("Trace: project {} (rolling)", id.as_simple()),
            ));
        }
    }

    if let Ok(rd) = std::fs::read_dir(paths::trace_agents_dir(home)) {
        for ent in rd.flatten() {
            let path = ent.path();
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            let Some(stem) = name.strip_suffix(".log") else {
                continue;
            };
            let Ok(id) = Uuid::parse_str(stem) else {
                continue;
            };
            let key = format!("trace_agt_{}", uuid_to_api_key_segment(&id));
            out.push((key, format!("Trace: agent {} (rolling)", id.as_simple())));
        }
    }

    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Format `YYYYmmddHH` hour bucket start as `DateTime<Utc>`.
pub fn hour_bucket_start_utc(ymd_h: &str) -> Option<chrono::DateTime<Utc>> {
    if ymd_h.len() != 10 {
        return None;
    }
    let y = ymd_h.get(0..4)?.parse().ok()?;
    let mo = ymd_h.get(4..6)?.parse().ok()?;
    let d = ymd_h.get(6..8)?.parse().ok()?;
    let h = ymd_h.get(8..10)?.parse().ok()?;
    let nd = NaiveDate::from_ymd_opt(y, mo, d)?;
    let naive = nd.and_hms_opt(h, 0, 0)?;
    Some(chrono::DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn rotate_and_prune_smoke() {
        let dir = tempdir().unwrap();
        let t = ScopedFileTrace::new(dir.path()).unwrap();
        t.system().info("hello");
        let p = Uuid::new_v4();
        t.project(&p.to_string()).unwrap().info("proj line");
        let a = Uuid::new_v4();
        t.agent(&a.to_string()).unwrap().warn("agent warn");
        assert!(t.root().join("system.log").exists());
        let n = t.prune_expired().unwrap();
        assert!(n < u64::MAX);
        let entries = trace_log_catalog_entries(dir.path());
        assert!(entries.iter().any(|(k, _)| k == "trace_system"));
    }

    #[test]
    fn resolve_round_trip_key() {
        let dir = tempdir().unwrap();
        let id = Uuid::new_v4();
        let path = paths::trace_projects_dir(dir.path()).join(format!("{id}.log"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"x").unwrap();
        let key = format!("trace_prj_{}", uuid_to_api_key_segment(&id));
        let resolved = resolve_trace_log_key(dir.path(), &key).unwrap();
        assert_eq!(resolved, path);
    }
}
