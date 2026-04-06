//! Multi-repo project model (backlog root + spokes + pipeline overrides).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

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

/// Optional pipeline settings for a project.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectPipelineOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_routing: Option<HashMap<String, String>>,
}

/// One spoke workspace under a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectPatch {
    pub name: Option<String>,
    pub spokes: Option<Vec<SpokeDescriptor>>,
    pub pipeline_overrides: Option<ProjectPipelineOverrides>,
}

/// Registered OpenFang project (backlog root + spokes).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Project {
    pub id: ProjectId,
    pub name: String,
    pub path: PathBuf,
    pub spokes: Vec<SpokeDescriptor>,
    pub pipeline_overrides: ProjectPipelineOverrides,
    /// Explicitly bound agent UUID strings (dashboard / API); persisted in `projects.json`.
    #[serde(default)]
    pub bound_agents: Vec<String>,
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
            pipeline_overrides: ProjectPipelineOverrides::default(),
            bound_agents: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }
}

impl Project {
    /// `path/backlog`
    pub fn backlog_root(&self) -> PathBuf {
        self.path.join("backlog")
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backlog_root_joins_backlog() {
        let p = Project {
            path: PathBuf::from("/repo/root"),
            ..Default::default()
        };
        assert_eq!(p.backlog_root(), PathBuf::from("/repo/root/backlog"));
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
        let j = r#"{"id":"550e8400-e29b-41d4-a716-446655440000","name":"n","path":"/p","spokes":[],"pipeline_overrides":{},"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"}"#;
        let p: Project = serde_json::from_str(j).unwrap();
        assert!(p.bound_agents.is_empty());
    }

    #[test]
    fn project_pipeline_overrides_json_omits_none() {
        let empty = serde_json::to_string(&ProjectPipelineOverrides::default()).unwrap();
        assert_eq!(empty, "{}");

        let partial = ProjectPipelineOverrides {
            max_retries: Some(3),
            model_routing: None,
        };
        let j = serde_json::to_string(&partial).unwrap();
        assert!(j.contains("max_retries"));
        assert!(!j.contains("model_routing"));
    }
}
