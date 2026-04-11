//! Project-scoped agents, spokes, and workflow audit filtering.

use crate::project_backlog;
use openfang_types::project::Project;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// `(spoke_name, absolute path)`.
pub fn resolved_spoke_pairs(project: &Project) -> Vec<(String, PathBuf)> {
    project
        .spokes
        .iter()
        .map(|s| {
            let abs = if s.path.is_absolute() {
                s.path.clone()
            } else {
                project.path.join(&s.path)
            };
            (s.name.clone(), abs)
        })
        .collect()
}

pub fn normalize_existing(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

/// Longest matching spoke root wins.
pub fn workspace_spoke_name(ws: &Path, pairs: &[(String, PathBuf)]) -> Option<String> {
    let w = normalize_existing(ws);
    let mut best: Option<(usize, String)> = None;
    for (name, root) in pairs {
        let r = normalize_existing(root);
        if w.starts_with(&r) {
            let len = r.as_os_str().len();
            if best.as_ref().map(|(l, _)| len > *l).unwrap_or(true) {
                best = Some((len, name.clone()));
            }
        }
    }
    best.map(|(_, n)| n)
}

pub fn backlog_task_id_set(backlog_root: &Path) -> HashSet<String> {
    project_backlog::list_tasks(backlog_root, None)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| v.get("id").and_then(|x| x.as_str()).map(String::from))
        .collect()
}

fn path_matches_any_spoke(field: &str, spokes_norm: &[PathBuf]) -> bool {
    if spokes_norm.is_empty() {
        return false;
    }
    let w = normalize_existing(Path::new(field));
    spokes_norm.iter().any(|r| w.starts_with(r))
}

pub fn pipeline_event_in_scope(
    ev: &Value,
    spokes_norm: &[PathBuf],
    task_ids: &HashSet<String>,
) -> bool {
    if let Some(tid) = ev.get("task_id").and_then(|x| x.as_str()) {
        if task_ids.contains(tid) {
            return true;
        }
    }
    if let Some(sr) = ev.get("spoke_root").and_then(|x| x.as_str()) {
        if path_matches_any_spoke(sr, spokes_norm) {
            return true;
        }
    }
    if let Some(ws) = ev.get("workspace").and_then(|x| x.as_str()) {
        if path_matches_any_spoke(ws, spokes_norm) {
            return true;
        }
    }
    false
}

pub fn flatten_pipeline_event(ev: &Value) -> Value {
    let event = ev
        .get("event")
        .and_then(|x| x.as_str())
        .unwrap_or("unknown");
    let timestamp = ev
        .get("timestamp")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let task_id = ev
        .get("task_id")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let spoke = ev
        .get("spoke_root")
        .or_else(|| ev.get("workspace"))
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let (action, outcome) = match event {
        "quality_gate" => (
            "gate",
            format!(
                "exit_code={}",
                ev.get("exit_code").and_then(|x| x.as_i64()).unwrap_or(-1)
            ),
        ),
        "cursor_worker_start" => (
            "cursor",
            format!(
                "running model={} mode={} pid={}",
                ev.get("model").and_then(|x| x.as_str()).unwrap_or(""),
                ev.get("mode").and_then(|x| x.as_str()).unwrap_or(""),
                ev.get("child_pid")
                    .and_then(|x| x.as_u64())
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| "-".to_string())
            ),
        ),
        "cursor_worker" => (
            "cursor",
            format!(
                "exit_code={} model={}",
                ev.get("exit_code").and_then(|x| x.as_i64()).unwrap_or(-1),
                ev.get("model").and_then(|x| x.as_str()).unwrap_or("")
            ),
        ),
        "backlog_status_transition" => (
            "status_change",
            ev.get("to_status")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string(),
        ),
        "git_action" => (
            "git",
            ev.get("result")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string(),
        ),
        "conduit_run_outcome" => (
            "conduit_outcome",
            format!(
                "success={}",
                ev.get("success").and_then(|x| x.as_bool()).unwrap_or(false)
            ),
        ),
        _ => (event, ev.to_string()),
    };
    json!({
        "task_id": task_id,
        "spoke": spoke,
        "action": action,
        "outcome": outcome,
        "timestamp": timestamp,
        "raw_event": event,
    })
}

/// `events` must be **newest first** (same order as `recent_pipeline_audit_records`).
pub fn scoped_pipeline_rows(
    events: &[Value],
    spoke_paths: &[PathBuf],
    backlog_ids: &HashSet<String>,
    limit: usize,
) -> Vec<Value> {
    let spokes_norm: Vec<PathBuf> = spoke_paths.iter().map(|p| normalize_existing(p)).collect();
    let mut out = Vec::new();
    for ev in events {
        if pipeline_event_in_scope(ev, &spokes_norm, backlog_ids) {
            out.push(flatten_pipeline_event(ev));
            if out.len() >= limit {
                break;
            }
        }
    }
    out
}

/// `events` newest first; first matching quality_gate for this spoke wins.
pub fn last_quality_gate_for_spoke(spoke_abs: &Path, events: &[Value]) -> Option<Value> {
    let sn = normalize_existing(spoke_abs);
    for ev in events {
        if ev.get("event").and_then(|x| x.as_str()) != Some("quality_gate") {
            continue;
        }
        let sr = ev.get("spoke_root").and_then(|x| x.as_str())?;
        let pn = normalize_existing(Path::new(sr));
        if pn == sn {
            return Some(json!({
                "exit_code": ev.get("exit_code"),
                "timestamp": ev.get("timestamp"),
            }));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use openfang_runtime::pipeline_audit::{self, PipelineAuditEvent};
    use tempfile::tempdir;

    #[test]
    fn workspace_picks_longest_spoke_prefix() {
        let tmp = tempdir().unwrap();
        let outer = tmp.path().join("outer");
        let inner = outer.join("inner");
        std::fs::create_dir_all(&inner).unwrap();
        let pairs = vec![
            ("outer".into(), outer.clone()),
            ("inner".into(), inner.clone()),
        ];
        assert_eq!(
            workspace_spoke_name(&inner.join("ws"), &pairs).as_deref(),
            Some("inner")
        );
    }

    #[test]
    fn scoped_pipelines_filter() {
        pipeline_audit::clear_pipeline_audit_ring();
        let tmp = tempdir().unwrap();
        let spoke = tmp.path().join("spoke");
        std::fs::create_dir_all(&spoke).unwrap();
        let canon = spoke.canonicalize().unwrap();
        pipeline_audit::emit(PipelineAuditEvent::QualityGate {
            timestamp: "t1".into(),
            task_id: "TASK-X".into(),
            spoke_root: canon.to_string_lossy().into(),
            exit_code: 0,
            actor: "a".into(),
            tool: "enforce_quality_gate".into(),
        });
        let events = pipeline_audit::recent_pipeline_audit_records(100);
        let mut ids = HashSet::new();
        ids.insert("TASK-OTHER".into());
        let rows = scoped_pipeline_rows(&events, std::slice::from_ref(&canon), &ids, 10);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["action"], "gate");

        let rows2 = scoped_pipeline_rows(&events, &[], &ids, 10);
        assert!(rows2.is_empty());
        ids.insert("TASK-X".into());
        let rows3 = scoped_pipeline_rows(&events, &[], &ids, 10);
        assert_eq!(rows3.len(), 1);
    }
}
