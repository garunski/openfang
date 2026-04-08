//! Structured JSON audit lines for automation pipeline runs.
//!
//! Emits `tracing` events (target `openfang_pipeline_audit`, field `line` = one JSON object).
//! Append the same lines to a file when **`OPENFANG_PIPELINE_AUDIT_LOG`** is set to a path.

use chrono::Utc;
use serde::Serialize;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use tracing::info;

/// Cap for in-memory pipeline audit ring (API reads this; tracing/file remain primary sinks).
const PIPELINE_AUDIT_RING_CAP: usize = 8000;

static PIPELINE_AUDIT_RING: OnceLock<Mutex<VecDeque<serde_json::Value>>> = OnceLock::new();

fn pipeline_audit_ring() -> &'static Mutex<VecDeque<serde_json::Value>> {
    PIPELINE_AUDIT_RING.get_or_init(|| Mutex::new(VecDeque::with_capacity(PIPELINE_AUDIT_RING_CAP)))
}

fn ring_push(v: &serde_json::Value) {
    let mut g = pipeline_audit_ring()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if g.len() >= PIPELINE_AUDIT_RING_CAP {
        g.pop_front();
    }
    g.push_back(v.clone());
}

/// Recent pipeline audit JSON records, **newest first** (up to `limit`).
pub fn recent_pipeline_audit_records(limit: usize) -> Vec<serde_json::Value> {
    let g = pipeline_audit_ring()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    g.iter().rev().take(limit).cloned().collect()
}

/// Clear the in-memory ring (e.g. integration tests).
pub fn clear_pipeline_audit_ring() {
    if let Ok(mut g) = pipeline_audit_ring().lock() {
        g.clear();
    }
}

#[cfg(test)]
static PIPELINE_AUDIT_TEST_RECORDS: Mutex<Vec<serde_json::Value>> = Mutex::new(Vec::new());

/// Clear recorded pipeline audit events (unit tests only).
#[cfg(test)]
pub fn clear_pipeline_audit_test_buffer() {
    PIPELINE_AUDIT_TEST_RECORDS.lock().unwrap().clear();
    clear_pipeline_audit_ring();
}

/// Take and clear recorded pipeline audit events (unit tests only).
#[cfg(test)]
pub fn take_pipeline_audit_test_buffer() -> Vec<serde_json::Value> {
    std::mem::take(&mut *PIPELINE_AUDIT_TEST_RECORDS.lock().unwrap())
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum PipelineAuditEvent {
    BacklogStatusTransition {
        timestamp: String,
        task_id: String,
        from_status: Option<String>,
        to_status: String,
        actor: String,
        tool: String,
    },
    QualityGate {
        timestamp: String,
        task_id: String,
        spoke_root: String,
        exit_code: i32,
        actor: String,
        tool: String,
    },
    CursorWorker {
        timestamp: String,
        task_id: String,
        workspace: String,
        mode: String,
        exit_code: i32,
        actor: String,
        tool: String,
    },
    GitAction {
        timestamp: String,
        task_id: String,
        spoke_root: String,
        action: String,
        result: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        branch_name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        commit_sha: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pr_url: Option<String>,
        actor: String,
        tool: String,
    },
    PipelineRunOutcome {
        timestamp: String,
        task_id: String,
        success: bool,
        retry_count: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        rollback_to_status: Option<String>,
        actor: String,
        tool: String,
    },
}

pub fn emit(event: PipelineAuditEvent) {
    let v = serde_json::to_value(&event)
        .unwrap_or_else(|_| serde_json::json!({ "event": "pipeline_audit_serialize_error" }));
    ring_push(&v);
    #[cfg(test)]
    {
        PIPELINE_AUDIT_TEST_RECORDS.lock().unwrap().push(v.clone());
    }
    let line = serde_json::to_string(&v)
        .unwrap_or_else(|_| r#"{"event":"pipeline_audit_line_error"}"#.to_string());
    info!(target: "openfang_pipeline_audit", %line);
    if let Ok(path) = std::env::var("OPENFANG_PIPELINE_AUDIT_LOG") {
        let path = path.trim();
        if !path.is_empty() {
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(Path::new(path))
            {
                use std::io::Write;
                let _ = writeln!(f, "{line}");
            }
        }
    }
}

pub fn log_backlog_status_transition(
    task_id: impl Into<String>,
    from_status: Option<String>,
    to_status: impl Into<String>,
    actor: impl Into<String>,
) {
    emit(PipelineAuditEvent::BacklogStatusTransition {
        timestamp: Utc::now().to_rfc3339(),
        task_id: task_id.into(),
        from_status,
        to_status: to_status.into(),
        actor: actor.into(),
        tool: "backlog_task_edit".to_string(),
    });
}

pub fn log_quality_gate(
    task_id: impl Into<String>,
    spoke_root: impl Into<String>,
    exit_code: i32,
    actor: impl Into<String>,
) {
    emit(PipelineAuditEvent::QualityGate {
        timestamp: Utc::now().to_rfc3339(),
        task_id: task_id.into(),
        spoke_root: spoke_root.into(),
        exit_code,
        actor: actor.into(),
        tool: "enforce_quality_gate".to_string(),
    });
}

pub fn log_cursor_worker(
    task_id: impl Into<String>,
    workspace: impl Into<String>,
    mode: impl Into<String>,
    exit_code: i32,
    actor: impl Into<String>,
) {
    emit(PipelineAuditEvent::CursorWorker {
        timestamp: Utc::now().to_rfc3339(),
        task_id: task_id.into(),
        workspace: workspace.into(),
        mode: mode.into(),
        exit_code,
        actor: actor.into(),
        tool: "trigger_cursor_worker".to_string(),
    });
}

pub fn log_pipeline_run_outcome(
    task_id: impl Into<String>,
    success: bool,
    retry_count: u32,
    rollback_to_status: Option<String>,
    actor: impl Into<String>,
    tool: impl Into<String>,
) {
    emit(PipelineAuditEvent::PipelineRunOutcome {
        timestamp: Utc::now().to_rfc3339(),
        task_id: task_id.into(),
        success,
        retry_count,
        rollback_to_status,
        actor: actor.into(),
        tool: tool.into(),
    });
}

/// Parse `status:` from YAML front matter in `backlog task <id> --plain` output.
pub fn parse_backlog_plain_status(text: &str) -> Option<String> {
    let mut in_fm = false;
    for line in text.lines() {
        let t = line.trim();
        if t == "---" {
            if !in_fm {
                in_fm = true;
            } else {
                break;
            }
            continue;
        }
        if in_fm {
            if let Some(rest) = t.strip_prefix("status:") {
                let s = rest.trim().trim_matches('\'').trim_matches('"').to_string();
                if !s.is_empty() {
                    return Some(s);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_backlog_plain_status_finds_value() {
        let text = "---\nid: TASK-1\nstatus: In Progress\n---\nbody\n";
        assert_eq!(
            parse_backlog_plain_status(text).as_deref(),
            Some("In Progress")
        );
    }

    #[test]
    fn parse_backlog_plain_status_none_without_front_matter() {
        assert!(parse_backlog_plain_status("no front matter").is_none());
    }

    #[test]
    fn quality_gate_event_serializes_for_correlation() {
        let ev = PipelineAuditEvent::QualityGate {
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            task_id: "TASK-99".to_string(),
            spoke_root: "/tmp/spoke".to_string(),
            exit_code: 0,
            actor: "agent-a".to_string(),
            tool: "enforce_quality_gate".to_string(),
        };
        let line = serde_json::to_string(&ev).unwrap();
        assert!(!line.contains('\n'));
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["event"], "quality_gate");
        assert_eq!(v["task_id"], "TASK-99");
        assert_eq!(v["exit_code"], 0);
    }

    #[test]
    fn cursor_worker_event_line_is_single_row_json() {
        let ev = PipelineAuditEvent::CursorWorker {
            timestamp: "t".to_string(),
            task_id: "T1".to_string(),
            workspace: "/w".to_string(),
            mode: "agent".to_string(),
            exit_code: 1,
            actor: "act".to_string(),
            tool: "trigger_cursor_worker".to_string(),
        };
        let line = serde_json::to_string(&ev).unwrap();
        assert!(!line.contains('\n'));
    }
}
