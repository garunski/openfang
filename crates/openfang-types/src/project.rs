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
}
