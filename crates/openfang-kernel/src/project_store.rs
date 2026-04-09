//! File-backed project registry (`<home>/projects.json`).

use openfang_types::agent::AgentId;
use openfang_types::error::{OpenFangError, OpenFangResult};
use openfang_types::project::{Project, ProjectId, ProjectPatch, SpokeDescriptor};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
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
        let data = std::fs::read_to_string(&self.persist_path)
            .map_err(|e| OpenFangError::Internal(format!("Failed to read projects: {e}")))?;
        let list: Vec<Project> = serde_json::from_str(&data)
            .map_err(|e| OpenFangError::Internal(format!("Failed to parse projects: {e}")))?;
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
        std::fs::rename(&tmp_path, &self.persist_path)
            .map_err(|e| OpenFangError::Internal(format!("Failed to rename projects file: {e}")))?;
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

        project.normalize_admin_spoke_field();

        let mut map = self
            .projects
            .write()
            .map_err(|_| OpenFangError::Internal("Project store lock poisoned".into()))?;

        validate_new_project(&map, &project.name, &project.path, None)?;
        validate_admin_spoke_references(&project)?;

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

    /// Project that binds this Mattermost channel id, if any.
    pub fn find_by_mattermost_channel_id(&self, channel_id: &str) -> Option<Project> {
        self.list()
            .into_iter()
            .find(|p| p.mattermost_channel_id.as_deref() == Some(channel_id))
    }

    /// Route Mattermost traffic: [`Project::orchestrator_agent_id`] when set, else last `bound_agents`.
    pub fn mattermost_project_route(&self, channel_id: &str) -> Option<(AgentId, ProjectId)> {
        let p = self.find_by_mattermost_channel_id(channel_id)?;
        if let Some(ref raw) = p.orchestrator_agent_id {
            let t = raw.trim();
            if !t.is_empty() {
                if let Ok(aid) = AgentId::from_str(t) {
                    return Some((aid, p.id));
                }
            }
        }
        let raw = p.bound_agents.last()?.trim();
        if raw.is_empty() {
            return None;
        }
        let aid = AgentId::from_str(raw).ok()?;
        Some((aid, p.id))
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
        if let Some(admin) = patch.admin_spoke {
            updated.admin_spoke = admin
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
        }
        if let Some(v) = patch.mattermost_channel_id {
            updated.mattermost_channel_id =
                v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        }
        if let Some(v) = patch.mattermost_team_name {
            updated.mattermost_team_name =
                v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        }
        if let Some(v) = patch.mattermost_channel_name {
            updated.mattermost_channel_name =
                v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        }
        if let Some(v) = patch.orchestrator_agent_id {
            updated.orchestrator_agent_id =
                v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        }
        updated.normalize_admin_spoke_field();
        validate_admin_spoke_references(&updated)?;
        updated.updated_at = chrono::Utc::now();
        map.insert(id, updated.clone());
        drop(map);
        self.persist()?;
        Ok(updated)
    }

    /// Append `agent_id` (UUID string) to [`Project::bound_agents`] if not already present.
    pub fn bind_agent(&self, project_id: ProjectId, agent_id: String) -> OpenFangResult<()> {
        let id_norm = agent_id.trim();
        if id_norm.is_empty() {
            return Err(OpenFangError::InvalidInput(
                "agent_id must be non-empty".into(),
            ));
        }
        let id_norm = id_norm.to_string();
        let mut map = self
            .projects
            .write()
            .map_err(|_| OpenFangError::Internal("Project store lock poisoned".into()))?;
        let project = map.get_mut(&project_id).ok_or_else(|| {
            OpenFangError::InvalidInput(format!("Unknown project id {project_id}"))
        })?;
        if project.bound_agents.iter().any(|a| a == &id_norm) {
            return Err(OpenFangError::InvalidInput(
                "Agent already bound to project".into(),
            ));
        }
        project.bound_agents.push(id_norm);
        project.updated_at = chrono::Utc::now();
        drop(map);
        self.persist()
    }

    /// Remove `agent_id` from [`Project::bound_agents`].
    pub fn unbind_agent(&self, project_id: ProjectId, agent_id: &str) -> OpenFangResult<()> {
        let needle = agent_id.trim();
        if needle.is_empty() {
            return Err(OpenFangError::InvalidInput(
                "agent_id must be non-empty".into(),
            ));
        }
        let mut map = self
            .projects
            .write()
            .map_err(|_| OpenFangError::Internal("Project store lock poisoned".into()))?;
        let project = map.get_mut(&project_id).ok_or_else(|| {
            OpenFangError::InvalidInput(format!("Unknown project id {project_id}"))
        })?;
        let pos = project
            .bound_agents
            .iter()
            .position(|a| a == needle)
            .ok_or_else(|| OpenFangError::InvalidInput("Agent binding not found".into()))?;
        project.bound_agents.remove(pos);
        project.updated_at = chrono::Utc::now();
        drop(map);
        self.persist()
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

    /// Replace a project by id (used after kernel updates orchestrator fields).
    pub fn replace(&self, project: Project) -> OpenFangResult<()> {
        validate_admin_spoke_references(&project)?;
        let mut map = self
            .projects
            .write()
            .map_err(|_| OpenFangError::Internal("Project store lock poisoned".into()))?;
        map.insert(project.id, project);
        drop(map);
        self.persist()
    }

    /// Discover spokes: immediate child dirs of the project root that are Git work trees
    /// (`git rev-parse --is-inside-work-tree`), skipping a top-level `backlog/` folder.
    pub fn discover_spokes(&self, id: ProjectId) -> OpenFangResult<Vec<SpokeDescriptor>> {
        let project = self
            .get(id)
            .ok_or_else(|| OpenFangError::InvalidInput(format!("Unknown project id {id}")))?;
        let root = &project.path;

        let mut out = Vec::new();
        let read = std::fs::read_dir(root).map_err(|e| {
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
            if !crate::git_worktree::path_is_git_worktree(&path) {
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

/// When set, `admin_spoke` must match a [`SpokeDescriptor::name`].
fn validate_admin_spoke_references(project: &Project) -> OpenFangResult<()> {
    let Some(ref raw) = project.admin_spoke else {
        return Ok(());
    };
    let name = raw.trim();
    if name.is_empty() {
        return Ok(());
    }
    if project.spokes.iter().any(|s| s.name == name) {
        return Ok(());
    }
    Err(OpenFangError::InvalidInput(format!(
        "admin_spoke '{name}' must match a spoke name"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openfang_types::project::ProjectPipelineOverrides;
    use std::process::Command;
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
        assert!(msg.contains("already exists"), "unexpected message: {msg}");
    }

    #[test]
    fn overlapping_path_rejected() {
        let dir = tempdir().unwrap();
        let parent = dir.path().join("parent");
        let child = parent.join("child");
        std::fs::create_dir_all(&child).unwrap();
        let store = ProjectStore::new(dir.path());
        store
            .register(sample_project("outer", parent.clone()))
            .unwrap();
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
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(u.name, "n2");
        assert_eq!(u.spokes.len(), 1);
        assert_eq!(u.pipeline_overrides.max_retries, Some(3));
    }

    #[test]
    fn update_mattermost_fields_patch() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("r");
        std::fs::create_dir_all(&p).unwrap();
        let store = ProjectStore::new(dir.path());
        let id = store.register(sample_project("n1", p)).unwrap();
        let u = store
            .update(
                id,
                ProjectPatch {
                    mattermost_channel_id: Some(Some("ch-1".into())),
                    mattermost_team_name: Some(Some("acme".into())),
                    mattermost_channel_name: Some(Some("town-square".into())),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(u.mattermost_channel_id.as_deref(), Some("ch-1"));
        assert_eq!(u.mattermost_team_name.as_deref(), Some("acme"));
        assert_eq!(u.mattermost_channel_name.as_deref(), Some("town-square"));

        let cleared = store
            .update(
                id,
                ProjectPatch {
                    mattermost_channel_id: Some(None),
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(cleared.mattermost_channel_id.is_none());
        assert_eq!(
            cleared.mattermost_channel_name.as_deref(),
            Some("town-square")
        );
    }

    #[test]
    fn mattermost_project_route_uses_last_bound_agent() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("r");
        std::fs::create_dir_all(&p).unwrap();
        let store = ProjectStore::new(dir.path());
        let orch = uuid::Uuid::new_v4();
        let other = uuid::Uuid::new_v4();
        let id = store
            .register(Project {
                name: "p".into(),
                path: p,
                mattermost_channel_id: Some("mm-ch-99".into()),
                bound_agents: vec![other.to_string(), orch.to_string()],
                ..Default::default()
            })
            .unwrap();
        let got = store.mattermost_project_route("mm-ch-99").unwrap();
        assert_eq!(got.0 .0, orch);
        assert_eq!(got.1, id);
        assert!(store.mattermost_project_route("unknown").is_none());
    }

    #[test]
    fn mattermost_project_route_prefers_orchestrator_agent_id() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("r");
        std::fs::create_dir_all(&p).unwrap();
        let store = ProjectStore::new(dir.path());
        let orch = uuid::Uuid::new_v4();
        let other = uuid::Uuid::new_v4();
        let id = store
            .register(Project {
                name: "p".into(),
                path: p,
                mattermost_channel_id: Some("mm-orch".into()),
                orchestrator_agent_id: Some(orch.to_string()),
                bound_agents: vec![other.to_string()],
                ..Default::default()
            })
            .unwrap();
        let got = store.mattermost_project_route("mm-orch").unwrap();
        assert_eq!(got.0 .0, orch);
        assert_eq!(got.1, id);
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
    fn bind_and_unbind_agent_roundtrip() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("r");
        std::fs::create_dir_all(&p).unwrap();
        let store = ProjectStore::new(dir.path());
        let id = store.register(sample_project("n1", p)).unwrap();
        store.bind_agent(id, "a1-uuid".into()).unwrap();
        let loaded = store.get(id).unwrap();
        assert_eq!(loaded.bound_agents, vec!["a1-uuid".to_string()]);
        store.unbind_agent(id, "a1-uuid").unwrap();
        assert!(store.get(id).unwrap().bound_agents.is_empty());
        store.bind_agent(id, "a1-uuid".into()).unwrap();
        let err2 = store.bind_agent(id, "a1-uuid".into()).unwrap_err();
        assert!(err2.to_string().contains("already bound"), "{err2}");
        let err3 = store.unbind_agent(id, "missing").unwrap_err();
        assert!(err3.to_string().contains("binding not found"), "{err3}");
    }

    #[test]
    fn discover_spokes_finds_git_worktree() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("proj");
        std::fs::create_dir_all(root.join("backlog")).unwrap();
        let spoke = root.join("spoke1");
        std::fs::create_dir_all(&spoke).unwrap();
        assert!(
            Command::new("git")
                .arg("init")
                .current_dir(&spoke)
                .status()
                .map(|s| s.success())
                .unwrap_or(false),
            "git init required for discover_spokes test"
        );

        let store = ProjectStore::new(dir.path());
        let id = store.register(sample_project("p", root.clone())).unwrap();
        let found = store.discover_spokes(id).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "spoke1");
        assert_eq!(found[0].path, PathBuf::from("spoke1"));
    }

    #[test]
    fn register_rejects_unknown_admin_spoke() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("r");
        std::fs::create_dir_all(&p).unwrap();
        let store = ProjectStore::new(dir.path());
        let err = store
            .register(Project {
                name: "x".into(),
                path: p,
                admin_spoke: Some("nope".into()),
                ..Default::default()
            })
            .unwrap_err();
        assert!(err.to_string().contains("admin_spoke"), "{err}");
    }

    #[test]
    fn discover_rejected_when_admin_spoke_not_in_discovered_list() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("proj");
        let disk_a = root.join("legacy");
        std::fs::create_dir_all(&disk_a).unwrap();
        assert!(Command::new("git")
            .arg("init")
            .current_dir(&disk_a)
            .status()
            .unwrap()
            .success());
        let disk_b = root.join("only");
        std::fs::create_dir_all(&disk_b).unwrap();
        assert!(Command::new("git")
            .arg("init")
            .current_dir(&disk_b)
            .status()
            .unwrap()
            .success());

        let store = ProjectStore::new(dir.path());
        let id = store
            .register(Project {
                name: "p".into(),
                path: root.clone(),
                spokes: vec![SpokeDescriptor {
                    name: "keep".into(),
                    path: PathBuf::from("keep"),
                    labels: vec![],
                }],
                admin_spoke: Some("keep".into()),
                ..Default::default()
            })
            .unwrap();

        let discovered = store.discover_spokes(id).unwrap();
        assert_eq!(discovered.len(), 2);
        let patch = ProjectPatch {
            spokes: Some(discovered),
            ..Default::default()
        };
        let err = store.update(id, patch).unwrap_err();
        assert!(err.to_string().contains("admin_spoke"), "{err}");
    }
}
