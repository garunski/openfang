//! Backlog.md entity types (tasks, docs, decisions, milestones, search).
//!
//! Field names and shapes align with `MrLesk/Backlog.md` `src/types/index.ts`; serde uses
//! `camelCase` on the wire unless noted.

pub mod parser;
pub mod reader;
pub mod serializer;

use serde::{Deserialize, Serialize};

/// Global / project backlog configuration (`backlog.config.yml`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BacklogConfig {
    pub project_name: String,
    #[serde(default)]
    pub default_status: String,
    #[serde(default)]
    pub statuses: Vec<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub task_prefix: String,
    pub date_format: String,
    #[serde(default)]
    pub definition_of_done: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_assignee: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_reporter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_column_width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_editor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_open_browser: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_operations: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_commit: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bypass_git_hooks: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check_active_branches: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_branch_days: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_status_change: Option<String>,
}

/// Task priority (`high` | `medium` | `low` in JSON).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskPriority {
    High,
    Medium,
    Low,
}

/// Parsed acceptance / DoD checklist line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptanceCriterion {
    pub index: usize,
    pub text: String,
    pub checked: bool,
}

/// Full task record (frontmatter + parsed body sections).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BacklogTask {
    pub id: String,
    pub title: String,
    pub status: String,
    #[serde(default)]
    pub assignee: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reporter: Option<String>,
    pub created_date: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_date: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub milestone: Option<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub references: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub documentation: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtasks: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<TaskPriority>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ordinal: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_status_change: Option<String>,

    #[serde(default)]
    pub raw_content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "acceptanceCriteriaItems", default)]
    pub acceptance_criteria: Vec<AcceptanceCriterion>,
    #[serde(rename = "definitionOfDoneItems", default)]
    pub definition_of_done: Vec<AcceptanceCriterion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub implementation_plan: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub implementation_notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_summary: Option<String>,
    /// Relative path from backlog root (set by filesystem reader only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
}

/// ADR / decision record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BacklogDecision {
    pub id: String,
    pub title: String,
    pub date: String,
    pub status: DecisionStatus,
    pub context: String,
    pub decision: String,
    pub consequences: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alternatives: Option<String>,
    pub raw_content: String,
    /// Relative path from backlog root (set by filesystem reader only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
}

/// Decision lifecycle in `decisions/*.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DecisionStatus {
    Proposed,
    Accepted,
    Rejected,
    Superseded,
}

/// Milestone metadata file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BacklogMilestone {
    pub id: String,
    pub title: String,
    pub description: String,
    pub raw_content: String,
    /// Relative path from backlog root (set by filesystem reader only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
}

/// Non-task markdown doc under `backlog/docs`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BacklogDocument {
    pub id: String,
    pub title: String,
    #[serde(rename = "type")]
    pub doc_type: String,
    pub created_date: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    pub raw_content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Relative path from backlog root to the markdown file (set by filesystem reader only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
}

/// Single hit from backlog search (mirrors `SearchResult` in Backlog.md).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum BacklogSearchResult {
    Task {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        score: Option<f64>,
        task: Box<BacklogTask>,
    },
    Document {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        score: Option<f64>,
        document: Box<BacklogDocument>,
    },
    Decision {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        score: Option<f64>,
        decision: Box<BacklogDecision>,
    },
}

/// Full load of a `backlog/` directory tree.
#[derive(Debug, Clone, Default)]
pub struct BacklogSnapshot {
    pub config: BacklogConfig,
    pub tasks: Vec<BacklogTask>,
    pub documents: Vec<BacklogDocument>,
    pub decisions: Vec<BacklogDecision>,
    pub drafts: Vec<BacklogTask>,
    pub milestones: Vec<BacklogMilestone>,
    pub completed: Vec<BacklogTask>,
    pub archived_milestones: Vec<BacklogMilestone>,
}

/// One node in a nested docs tree (for UI).
#[derive(Debug, Clone)]
pub struct DocTreeNode {
    pub name: String,
    pub children: Vec<DocTreeNode>,
    pub docs: Vec<BacklogDocument>,
}

/// Nested document tree built from [`BacklogDocument::path`] categories.
#[derive(Debug, Clone)]
pub struct DocTree {
    pub root: DocTreeNode,
}

impl DocTreeNode {
    pub fn root() -> Self {
        Self {
            name: String::new(),
            children: Vec::new(),
            docs: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_priority_json_lowercase() {
        let p = TaskPriority::High;
        assert_eq!(serde_json::to_string(&p).unwrap(), "\"high\"");
        let q: TaskPriority = serde_json::from_str("\"medium\"").unwrap();
        assert_eq!(q, TaskPriority::Medium);
    }

    #[test]
    fn decision_status_json_lowercase() {
        let s = DecisionStatus::Superseded;
        assert_eq!(serde_json::to_string(&s).unwrap(), "\"superseded\"");
    }

    #[test]
    fn backlog_task_accepts_typescript_style_json() {
        let json = r#"{
            "id": "TASK-1",
            "title": "T",
            "status": "Open",
            "assignee": ["a"],
            "createdDate": "2026-01-01",
            "acceptanceCriteriaItems": [{"index": 1, "text": "x", "checked": false}],
            "definitionOfDoneItems": [{"index": 0, "text": "y", "checked": true}],
            "rawContent": "body"
        }"#;
        let t: BacklogTask = serde_json::from_str(json).unwrap();
        assert_eq!(t.id, "TASK-1");
        assert_eq!(t.acceptance_criteria.len(), 1);
        assert_eq!(t.definition_of_done[0].text, "y");
    }

    #[test]
    fn backlog_search_result_roundtrip() {
        let r = BacklogSearchResult::Task {
            score: Some(1.5),
            task: Box::new(BacklogTask {
                id: "TASK-1".into(),
                ..Default::default()
            }),
        };
        let v = serde_json::to_value(&r).unwrap();
        let back: BacklogSearchResult = serde_json::from_value(v).unwrap();
        match back {
            BacklogSearchResult::Task { score, task } => {
                assert_eq!(score, Some(1.5));
                assert_eq!(task.id, "TASK-1");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn backlog_config_optional_fields_omit() {
        let c = BacklogConfig {
            project_name: "p".into(),
            date_format: "%Y".into(),
            ..Default::default()
        };
        let s = serde_json::to_string(&c).unwrap();
        assert!(!s.contains("defaultAssignee"));
    }
}
