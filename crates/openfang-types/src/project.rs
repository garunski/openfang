//! Multi-repo project model (backlog root + spokes + workflow overrides).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Returned by backlog APIs when a project has no admin spoke configured.
pub const ADMIN_SPOKE_REQUIRED_MSG: &str =
    "Admin spoke is required. Set an admin spoke for this project first.";

/// Unique identifier for a registered project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProjectId(pub Uuid);

impl ProjectId {
    /// Generate a new random project id.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ProjectId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ProjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for ProjectId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

/// Optional workflow settings for a project.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectConduitOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_routing: Option<HashMap<String, String>>,
    /// Env var name holding a GitHub PAT for `git_create_pr` (per-project override).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_token_env: Option<String>,
    /// `{task_id}` placeholder, e.g. `openfang/task-{task_id}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_branch_name_template: Option<String>,
}

/// One spoke workspace under a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SpokeDescriptor {
    pub name: String,
    pub path: PathBuf,
    pub labels: Vec<String>,
}

impl Default for SpokeDescriptor {
    fn default() -> Self {
        Self {
            name: String::new(),
            path: PathBuf::new(),
            labels: Vec::new(),
        }
    }
}

/// Partial update for [`Project`].
#[derive(Debug, Clone, Default)]
pub struct ProjectPatch {
    pub name: Option<String>,
    pub spokes: Option<Vec<SpokeDescriptor>>,
    pub conduit_overrides: Option<ProjectConduitOverrides>,
    /// `None` = no change; `Some(None)` = clear; `Some(Some(name))` = set.
    pub admin_spoke: Option<Option<String>>,
    /// Mattermost channel id: same semantics as [`Self::admin_spoke`].
    pub mattermost_channel_id: Option<Option<String>>,
    /// Team URL slug when disambiguation is required (`fang` in `…/fang/channels/…`).
    pub mattermost_team_name: Option<Option<String>>,
    pub mattermost_channel_name: Option<Option<String>>,
    /// `None` = no change; `Some(None)` = clear; `Some(Some(id))` = set.
    pub orchestrator_agent_id: Option<Option<String>>,
}

/// Registered OpenFang project (backlog root + spokes).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Project {
    pub id: ProjectId,
    pub name: String,
    pub path: PathBuf,
    pub spokes: Vec<SpokeDescriptor>,
    pub conduit_overrides: ProjectConduitOverrides,
    /// Explicitly bound agent UUID strings (dashboard / API); persisted in `projects.json`.
    #[serde(default)]
    pub bound_agents: Vec<String>,
    /// Spoke name whose `<spoke>/backlog` holds tasks, docs, and knowledge.
    #[serde(default)]
    pub admin_spoke: Option<String>,
    /// Mattermost channel id for project-scoped orchestrator traffic.
    #[serde(default)]
    pub mattermost_channel_id: Option<String>,
    /// Mattermost team URL slug (e.g. `fang`); optional when channel slug alone is enough.
    #[serde(default)]
    pub mattermost_team_name: Option<String>,
    #[serde(default)]
    pub mattermost_channel_name: Option<String>,
    /// Agent UUID for the per-project `conduit-coordinator` hand (Mattermost orchestrator).
    #[serde(default)]
    pub orchestrator_agent_id: Option<String>,
    /// Agent ids that were explicitly unbound or were a previous project orchestrator (capped).
    #[serde(default)]
    pub former_agent_ids: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Default for Project {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            id: ProjectId::new(),
            name: String::new(),
            path: PathBuf::new(),
            spokes: Vec::new(),
            conduit_overrides: ProjectConduitOverrides::default(),
            bound_agents: Vec::new(),
            admin_spoke: None,
            mattermost_channel_id: None,
            mattermost_team_name: None,
            mattermost_channel_name: None,
            orchestrator_agent_id: None,
            former_agent_ids: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }
}

impl Project {
    /// Record an agent id for dashboard history (deduped, FIFO-capped).
    pub fn record_former_agent(&mut self, agent_id: &str) {
        let t = agent_id.trim();
        if t.is_empty() {
            return;
        }
        let t = t.to_string();
        self.former_agent_ids.retain(|x| x != &t);
        self.former_agent_ids.push(t);
        const MAX: usize = 100;
        if self.former_agent_ids.len() > MAX {
            let drop = self.former_agent_ids.len() - MAX;
            self.former_agent_ids.drain(0..drop);
        }
    }

    /// Trim and clear empty `admin_spoke` values.
    pub fn normalize_admin_spoke_field(&mut self) {
        self.admin_spoke = self.admin_spoke.as_ref().and_then(|s| {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        });
    }

    /// `<admin_spoke_root>/backlog` when [`Self::admin_spoke`] is set and names a known spoke.
    pub fn admin_backlog_root(&self) -> Option<PathBuf> {
        let name = self.admin_spoke.as_ref()?.trim();
        if name.is_empty() {
            return None;
        }
        Some(self.resolve_spoke(name)?.join("backlog"))
    }

    /// Resolve a spoke by name to an absolute path.
    pub fn resolve_spoke(&self, name: &str) -> Option<PathBuf> {
        let spoke = self.spokes.iter().find(|s| s.name == name)?;
        let p = &spoke.path;
        if p.is_absolute() {
            Some(p.clone())
        } else {
            Some(self.path.join(p))
        }
    }

    /// True when `workspace` lies under at least one spoke root (normalized paths).
    /// Used to treat an agent whose manifest workspace matches a spoke as project-scoped.
    pub fn workspace_in_spoke_scope(&self, workspace: &Path) -> bool {
        let w = workspace
            .canonicalize()
            .unwrap_or_else(|_| workspace.to_path_buf());
        for s in &self.spokes {
            let root = if s.path.is_absolute() {
                s.path.clone()
            } else {
                self.path.join(&s.path)
            };
            let r = root.canonicalize().unwrap_or(root);
            if w.starts_with(&r) {
                return true;
            }
        }
        false
    }
}

/// One quality-gate or tool failure recorded for project-scoped context (stderr excerpt).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PastFailure {
    pub at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    pub stderr_snippet: String,
}

/// A decision or outcome note from a prior workflow run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DecisionLogEntry {
    pub at: DateTime<Utc>,
    pub summary: String,
}

/// File-backed per-project context for orchestrator prompts and Cursor enrichment (`context.json`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectContext {
    #[serde(default)]
    pub repo_structure_summary: String,
    #[serde(default)]
    pub coding_conventions: String,
    #[serde(default)]
    pub past_failures: Vec<PastFailure>,
    #[serde(default)]
    pub decision_log: Vec<DecisionLogEntry>,
    pub updated_at: DateTime<Utc>,
}

impl Default for ProjectContext {
    fn default() -> Self {
        Self {
            repo_structure_summary: String::new(),
            coding_conventions: String::new(),
            past_failures: Vec::new(),
            decision_log: Vec::new(),
            updated_at: Utc::now(),
        }
    }
}

impl ProjectContext {
    /// Drop old decisions and cap list sizes (called after load and before save).
    pub fn prune(&mut self, max_failures: usize, decision_max_age_days: u32) {
        if decision_max_age_days > 0 {
            let cutoff = Utc::now() - chrono::Duration::days(decision_max_age_days as i64);
            self.decision_log.retain(|d| d.at >= cutoff);
        }
        const MAX_DECISION_ENTRIES: usize = 200;
        if self.decision_log.len() > MAX_DECISION_ENTRIES {
            let drop = self.decision_log.len() - MAX_DECISION_ENTRIES;
            self.decision_log.drain(..drop);
        }
        let cap = max_failures.max(1);
        if self.past_failures.len() > cap {
            let drop = self.past_failures.len() - cap;
            self.past_failures.drain(..drop);
        }
        self.updated_at = Utc::now();
    }

    /// Merge fields from `update_project_context` tool JSON.
    pub fn apply_tool_update(&mut self, input: &serde_json::Value) -> Result<(), String> {
        if let Some(s) = input
            .get("set_repo_structure_summary")
            .and_then(|v| v.as_str())
        {
            self.repo_structure_summary = s.to_string();
        }
        if let Some(s) = input.get("set_coding_conventions").and_then(|v| v.as_str()) {
            self.coding_conventions = s.to_string();
        }
        if let Some(f) = input.get("append_past_failure") {
            let stderr_snippet = f
                .get("stderr_snippet")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let task_id = f
                .get("task_id")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from);
            self.past_failures.push(PastFailure {
                at: Utc::now(),
                task_id,
                stderr_snippet,
            });
        }
        if let Some(summary) = input
            .get("append_decision")
            .and_then(|v| v.get("summary"))
            .and_then(|v| v.as_str())
        {
            let summary = summary.trim();
            if !summary.is_empty() {
                self.decision_log.push(DecisionLogEntry {
                    at: Utc::now(),
                    summary: summary.to_string(),
                });
            }
        }
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Human-readable block prepended to workflow input (truncated to `max_chars`).
    pub fn conduit_prompt_section(&self, max_chars: usize) -> String {
        let mut parts: Vec<String> = Vec::new();
        if !self.repo_structure_summary.trim().is_empty() {
            parts.push(format!(
                "### Repo structure\n{}",
                self.repo_structure_summary.trim()
            ));
        }
        if !self.coding_conventions.trim().is_empty() {
            parts.push(format!(
                "### Coding conventions\n{}",
                self.coding_conventions.trim()
            ));
        }
        if !self.past_failures.is_empty() {
            let mut lines = vec!["### Recent failures (stderr excerpts)".to_string()];
            for f in &self.past_failures {
                let tid = f
                    .task_id
                    .as_deref()
                    .map(|s| format!(" ({s})"))
                    .unwrap_or_default();
                let snip = truncate_chars(&f.stderr_snippet, 800);
                lines.push(format!("- {}{}: {}", f.at.to_rfc3339(), tid, snip));
            }
            parts.push(lines.join("\n"));
        }
        if !self.decision_log.is_empty() {
            let mut lines = vec!["### Decision log".to_string()];
            for d in &self.decision_log {
                lines.push(format!(
                    "- {}: {}",
                    d.at.to_rfc3339(),
                    truncate_chars(&d.summary, 500)
                ));
            }
            parts.push(lines.join("\n"));
        }
        if parts.is_empty() {
            return String::new();
        }
        let body = parts.join("\n\n");
        let header = "## Accumulated project context\n\n";
        let full = format!("{header}{body}");
        truncate_chars(&full, max_chars.max(256))
    }
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut t = s.chars().take(max.saturating_sub(20)).collect::<String>();
    t.push_str("\n…[truncated]");
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_backlog_root_requires_spoke() {
        let p = Project {
            path: PathBuf::from("/repo/root"),
            admin_spoke: Some("admin".into()),
            ..Default::default()
        };
        assert!(p.admin_backlog_root().is_none());

        let p2 = Project {
            path: PathBuf::from("/proj"),
            admin_spoke: Some("admin".into()),
            spokes: vec![SpokeDescriptor {
                name: "admin".into(),
                path: PathBuf::from("spokes/admin"),
                labels: vec![],
            }],
            ..Default::default()
        };
        assert_eq!(
            p2.admin_backlog_root(),
            Some(PathBuf::from("/proj/spokes/admin/backlog"))
        );
    }

    #[test]
    fn resolve_spoke_relative() {
        let p = Project {
            path: PathBuf::from("/proj"),
            spokes: vec![SpokeDescriptor {
                name: "core".into(),
                path: PathBuf::from("spokes/core"),
                labels: vec![],
            }],
            ..Default::default()
        };
        assert_eq!(
            p.resolve_spoke("core"),
            Some(PathBuf::from("/proj/spokes/core"))
        );
    }

    #[test]
    fn resolve_spoke_absolute() {
        let p = Project {
            path: PathBuf::from("/proj"),
            spokes: vec![SpokeDescriptor {
                name: "x".into(),
                path: PathBuf::from("/abs/x"),
                labels: vec![],
            }],
            ..Default::default()
        };
        assert_eq!(p.resolve_spoke("x"), Some(PathBuf::from("/abs/x")));
    }

    #[test]
    fn project_id_display_from_str_roundtrip() {
        let id = ProjectId::new();
        let s = id.to_string();
        assert_eq!(id, s.parse::<ProjectId>().unwrap());
    }

    #[test]
    fn project_json_default_bound_agents_when_omitted() {
        let j = r#"{"id":"550e8400-e29b-41d4-a716-446655440000","name":"n","path":"/p","spokes":[],"conduit_overrides":{},"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"}"#;
        let p: Project = serde_json::from_str(j).unwrap();
        assert!(p.bound_agents.is_empty());
        assert!(p.admin_spoke.is_none());
        assert!(p.mattermost_channel_id.is_none());
        assert!(p.mattermost_team_name.is_none());
        assert!(p.mattermost_channel_name.is_none());
        assert!(p.orchestrator_agent_id.is_none());
        assert!(p.former_agent_ids.is_empty());
    }

    #[test]
    fn project_json_roundtrip_mattermost_fields() {
        let mut p = Project {
            mattermost_channel_id: Some("abc123".into()),
            mattermost_team_name: Some("acme".into()),
            mattermost_channel_name: Some("town-square".into()),
            ..Default::default()
        };
        p.normalize_admin_spoke_field();
        let j = serde_json::to_string(&p).unwrap();
        let q: Project = serde_json::from_str(&j).unwrap();
        assert_eq!(q.mattermost_channel_id.as_deref(), Some("abc123"));
        assert_eq!(q.mattermost_team_name.as_deref(), Some("acme"));
        assert_eq!(q.mattermost_channel_name.as_deref(), Some("town-square"));
    }

    #[test]
    fn project_conduit_overrides_json_omits_none() {
        let empty = serde_json::to_string(&ProjectConduitOverrides::default()).unwrap();
        assert_eq!(empty, "{}");

        let partial = ProjectConduitOverrides {
            max_retries: Some(3),
            model_routing: None,
            ..Default::default()
        };
        let j = serde_json::to_string(&partial).unwrap();
        assert!(j.contains("max_retries"));
        assert!(!j.contains("model_routing"));
    }

    #[test]
    fn project_context_prune_zero_age_keeps_old_decisions() {
        let old = Utc::now() - chrono::Duration::days(400);
        let mut ctx = ProjectContext {
            decision_log: vec![DecisionLogEntry {
                at: old,
                summary: "legacy".into(),
            }],
            ..Default::default()
        };
        ctx.prune(10, 0);
        assert_eq!(ctx.decision_log.len(), 1);
    }

    #[test]
    fn project_context_prune_failures_and_decisions() {
        let old = Utc::now() - chrono::Duration::days(100);
        let recent = Utc::now() - chrono::Duration::days(1);
        let mut ctx = ProjectContext {
            past_failures: vec![
                PastFailure {
                    at: Utc::now(),
                    task_id: None,
                    stderr_snippet: "a".into(),
                },
                PastFailure {
                    at: Utc::now(),
                    task_id: None,
                    stderr_snippet: "b".into(),
                },
                PastFailure {
                    at: Utc::now(),
                    task_id: None,
                    stderr_snippet: "c".into(),
                },
            ],
            decision_log: vec![
                DecisionLogEntry {
                    at: old,
                    summary: "old".into(),
                },
                DecisionLogEntry {
                    at: recent,
                    summary: "new".into(),
                },
            ],
            ..Default::default()
        };
        ctx.prune(2, 30);
        assert_eq!(ctx.past_failures.len(), 2);
        assert_eq!(ctx.decision_log.len(), 1);
        assert_eq!(ctx.decision_log[0].summary, "new");
    }

    #[test]
    fn project_context_apply_tool_update_and_prompt_section() {
        let mut ctx = ProjectContext::default();
        let v = serde_json::json!({
            "set_repo_structure_summary": "src/, crates/",
            "append_decision": { "summary": " use trait X " },
            "append_past_failure": { "task_id": "TASK-1", "stderr_snippet": "error: failed" }
        });
        ctx.apply_tool_update(&v).unwrap();
        assert!(ctx.repo_structure_summary.contains("crates"));
        assert_eq!(ctx.decision_log.len(), 1);
        assert_eq!(ctx.past_failures.len(), 1);
        let sec = ctx.conduit_prompt_section(10_000);
        assert!(sec.contains("Accumulated project"));
        assert!(sec.contains("TASK-1"));
    }
}
