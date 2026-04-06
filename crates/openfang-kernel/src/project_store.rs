//! File-backed project registry (`<home>/projects.json`).

use openfang_types::error::{OpenFangError, OpenFangResult};
use openfang_types::project::{Project, ProjectId, ProjectPatch, SpokeDescriptor};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use tracing::{debug, info};

/// In-memory project map with JSON persistence.
pub struct ProjectStore {
    projects: Arc<RwLock<HashMap<ProjectId, Project>>>,
    persist_path: PathBuf,
}

impl ProjectStore {
    /// New store; persistence at `home_dir/projects.json`.
    pub fn new(home_dir: &Path) -> Self {
        Self {
            projects: Arc::new(RwLock::new(HashMap::new())),
            persist_path: home_dir.join("projects.json"),
        }
    }

    /// Load `projects.json` into the map. Returns count loaded; no-op if missing.
    pub fn load(&self) -> OpenFangResult<usize> {
        if !self.persist_path.exists() {
            return Ok(0);
        }
        let data = std::fs::read_to_string(&self.persist_path).map_err(|e| {
            OpenFangError::Internal(format!("Failed to read projects: {e}"))
        })?;
        let list: Vec<Project> = serde_json::from_str(&data).map_err(|e| {
            OpenFangError::Internal(format!("Failed to parse projects: {e}"))
        })?;
        let count = list.len();
        let mut map = self
            .projects
            .write()
            .map_err(|_| OpenFangError::Internal("Project store lock poisoned".into()))?;
        map.clear();
        for p in list {
            map.insert(p.id, p);
        }
        info!(count, "Loaded projects from disk");
        Ok(count)
    }

    /// Write all projects to disk (atomic rename, same as cron scheduler).
    pub fn persist(&self) -> OpenFangResult<()> {
        let map = self
            .projects
            .read()
            .map_err(|_| OpenFangError::Internal("Project store lock poisoned".into()))?;
        let mut list: Vec<Project> = map.values().cloned().collect();
        list.sort_by(|a, b| a.id.to_string().cmp(&b.id.to_string()));
        let data = serde_json::to_string_pretty(&list)
            .map_err(|e| OpenFangError::Internal(format!("Failed to serialize projects: {e}")))?;
        let tmp_path = self.persist_path.with_extension("json.tmp");
        std::fs::write(&tmp_path, data.as_bytes()).map_err(|e| {
            OpenFangError::Internal(format!("Failed to write projects temp file: {e}"))
        })?;
        std::fs::rename(&tmp_path, &self.persist_path).map_err(|e| {
            OpenFangError::Internal(format!("Failed to rename projects file: {e}"))
        })?;
        debug!(count = list.len(), "Persisted projects");
        Ok(())
    }

    /// Register a project (new id assigned). Validates name and path overlap.
    pub fn register(&self, mut project: Project) -> OpenFangResult<ProjectId> {
        if project.name.trim().is_empty() {
            return Err(OpenFangError::InvalidInput(
                "Project name must not be empty".into(),
            ));
        }
        if project.path.as_os_str().is_empty() {
            return Err(OpenFangError::InvalidInput(
                "Project path must not be empty".into(),
            ));
        }

        let mut map = self
            .projects
            .write()
            .map_err(|_| OpenFangError::Internal("Project store lock poisoned".into()))?;

        validate_new_project(&map, &project.name, &project.path, None)?;

        let now = chrono::Utc::now();
        project.id = ProjectId::new();
        project.created_at = now;
        project.updated_at = now;
        let id = project.id;
        map.insert(id, project);
        drop(map);
        self.persist()?;
        Ok(id)
    }

    pub fn list(&self) -> Vec<Project> {
        self.projects
            .read()
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default()
    }

    pub fn get(&self, id: ProjectId) -> Option<Project> {
        self.projects.read().ok()?.get(&id).cloned()
    }

    /// Apply patch; checks name uniqueness when name changes.
    pub fn update(&self, id: ProjectId, patch: ProjectPatch) -> OpenFangResult<Project> {
        let mut map = self
            .projects
            .write()
            .map_err(|_| OpenFangError::Internal("Project store lock poisoned".into()))?;

        let existing = map
            .get(&id)
            .ok_or_else(|| OpenFangError::InvalidInput(format!("Unknown project id {id}")))?
            .clone();

        if let Some(ref name) = patch.name {
            if name.trim().is_empty() {
                return Err(OpenFangError::InvalidInput(
                    "Project name must not be empty".into(),
                ));
            }
            validate_new_project(&map, name, &existing.path, Some(id))?;
        }

        let mut updated = existing;
        if let Some(name) = patch.name {
            updated.name = name;
        }
        if let Some(spokes) = patch.spokes {
            updated.spokes = spokes;
        }
        if let Some(overrides) = patch.pipeline_overrides {
            updated.pipeline_overrides = overrides;
        }
        updated.updated_at = chrono::Utc::now();
        map.insert(id, updated.clone());
        drop(map);
        self.persist()?;
        Ok(updated)
    }

    pub fn remove(&self, id: ProjectId) -> OpenFangResult<Project> {
        let mut map = self
            .projects
            .write()
            .map_err(|_| OpenFangError::Internal("Project store lock poisoned".into()))?;
        let removed = map
            .remove(&id)
            .ok_or_else(|| OpenFangError::InvalidInput(format!("Unknown project id {id}")))?;
        drop(map);
        self.persist()?;
        Ok(removed)
    }

    /// Discover spokes: sibling dirs of `backlog/` under the project root that contain `mise.toml`.
    pub fn discover_spokes(&self, id: ProjectId) -> OpenFangResult<Vec<SpokeDescriptor>> {
        let project = self
            .get(id)
            .ok_or_else(|| OpenFangError::InvalidInput(format!("Unknown project id {id}")))?;
        let root = &project.path;
        let backlog = project.backlog_root();
        let parent = backlog
            .parent()
            .ok_or_else(|| OpenFangError::InvalidInput("Project path has no parent".into()))?;

        let mut out = Vec::new();
        let read = std::fs::read_dir(parent).map_err(|e| {
            OpenFangError::Internal(format!("Failed to read project directory: {e}"))
        })?;

        for ent in read {
            let ent = ent.map_err(|e| {
                OpenFangError::Internal(format!("Failed to read directory entry: {e}"))
            })?;
            let ptype = ent.file_type().map_err(|e| {
                OpenFangError::Internal(format!("Failed to stat directory entry: {e}"))
            })?;
            if !ptype.is_dir() {
                continue;
            }
            let path = ent.path();
            if path.file_name().and_then(|n| n.to_str()) == Some("backlog") {
                continue;
            }
            if !path.join("mise.toml").is_file() {
                continue;
            }
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("spoke")
                .to_string();
            let rel = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
            out.push(SpokeDescriptor {
                name,
                path: rel,
                labels: Vec::new(),
            });
        }

        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }
}

/// `skip_id`: when updating, ignore this project for name/path checks.
fn validate_new_project(
    map: &HashMap<ProjectId, Project>,
    name: &str,
    path: &Path,
    skip_id: Option<ProjectId>,
) -> OpenFangResult<()> {
    for (pid, p) in map {
        if Some(*pid) == skip_id {
            continue;
        }
        if p.name == name {
            return Err(OpenFangError::InvalidInput(format!(
                "A project named '{name}' already exists"
            )));
        }
        if paths_overlap(&p.path, path) {
            return Err(OpenFangError::InvalidInput(format!(
                "Project path overlaps existing project '{}' ({})",
                p.name,
                p.path.display()
            )));
        }
    }
    Ok(())
}

fn paths_overlap(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    a.starts_with(b) || b.starts_with(a)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openfang_types::project::ProjectPipelineOverrides;
    use tempfile::tempdir;

    fn sample_project(name: &str, path: PathBuf) -> Project {
        Project {
            name: name.into(),
            path,
            ..Default::default()
        }
    }

    #[test]
    fn load_missing_is_noop() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        assert_eq!(store.load().unwrap(), 0);
    }

    #[test]
    fn register_persist_reload() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("myproj");
        std::fs::create_dir_all(&p).unwrap();

        let store = ProjectStore::new(dir.path());
        let id = store.register(sample_project("alpha", p.clone())).unwrap();

        let store2 = ProjectStore::new(dir.path());
        assert_eq!(store2.load().unwrap(), 1);
        let loaded = store2.get(id).unwrap();
        assert_eq!(loaded.name, "alpha");
        assert_eq!(loaded.path, p);
    }

    #[test]
    fn duplicate_name_rejected() {
        let dir = tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let store = ProjectStore::new(dir.path());
        store.register(sample_project("same", a)).unwrap();
        let err = store
            .register(sample_project("same", b))
            .expect_err("dup name");
        let msg = err.to_string();
        assert!(
            msg.contains("already exists"),
            "unexpected message: {msg}"
        );
    }

    #[test]
    fn overlapping_path_rejected() {
        let dir = tempdir().unwrap();
        let parent = dir.path().join("parent");
        let child = parent.join("child");
        std::fs::create_dir_all(&child).unwrap();
        let store = ProjectStore::new(dir.path());
        store.register(sample_project("outer", parent.clone())).unwrap();
        let err = store
            .register(sample_project("inner", child))
            .expect_err("overlap");
        let msg = err.to_string();
        assert!(msg.contains("overlaps"), "unexpected message: {msg}");
    }

    #[test]
    fn update_patch() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("r");
        std::fs::create_dir_all(&p).unwrap();
        let store = ProjectStore::new(dir.path());
        let id = store.register(sample_project("n1", p)).unwrap();
        let u = store
            .update(
                id,
                ProjectPatch {
                    name: Some("n2".into()),
                    spokes: Some(vec![SpokeDescriptor {
                        name: "s".into(),
                        path: PathBuf::from("x"),
                        labels: vec![],
                    }]),
                    pipeline_overrides: Some(ProjectPipelineOverrides {
                        max_retries: Some(3),
                        model_routing: None,
                    }),
                },
            )
            .unwrap();
        assert_eq!(u.name, "n2");
        assert_eq!(u.spokes.len(), 1);
        assert_eq!(u.pipeline_overrides.max_retries, Some(3));
    }

    #[test]
    fn remove_roundtrip() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("z");
        std::fs::create_dir_all(&p).unwrap();
        let store = ProjectStore::new(dir.path());
        let id = store.register(sample_project("z", p)).unwrap();
        store.remove(id).unwrap();
        assert!(store.get(id).is_none());
    }

    #[test]
    fn discover_spokes_finds_mise() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("proj");
        std::fs::create_dir_all(root.join("backlog")).unwrap();
        let spoke = root.join("spoke1");
        std::fs::create_dir_all(&spoke).unwrap();
        std::fs::write(spoke.join("mise.toml"), "[tools]\n").unwrap();

        let store = ProjectStore::new(dir.path());
        let id = store.register(sample_project("p", root.clone())).unwrap();
        let found = store.discover_spokes(id).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "spoke1");
        assert_eq!(found[0].path, PathBuf::from("spoke1"));
    }
}
