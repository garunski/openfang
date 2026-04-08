//! In-memory backlog cache per project; reads via [`openfang_types::backlog::reader`], writes via serializer.

use chrono::{Duration, NaiveDate, Utc};
use openfang_types::backlog::parser::normalize_date;
use openfang_types::backlog::reader::{read_all, BacklogReadError};
use openfang_types::backlog::serializer::{
    serialize_decision, serialize_document, serialize_milestone, serialize_task,
    toggle_acceptance_criterion, update_frontmatter_field, update_frontmatter_i32, update_task_status,
};
use openfang_types::backlog::{
    BacklogConfig, BacklogDecision, BacklogDocument, BacklogMilestone, BacklogSnapshot, BacklogTask,
    DecisionStatus,
};
use openfang_types::project::ProjectId;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use crate::project_store::ProjectStore;

/// One task eligible for backlog.md-style cleanup (Done, under `tasks/`, reference date older than cutoff).
#[derive(Debug, Clone)]
pub struct DoneTaskCleanupCandidate {
    pub id: String,
    pub title: String,
    pub created_date: String,
    pub updated_date: Option<String>,
}

fn task_status_done_like_cleanup(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        "done" | "closed" | "complete" | "completed"
    )
}

fn parse_task_reference_date(raw: &str) -> Option<NaiveDate> {
    let n = normalize_date(raw.trim());
    if n.is_empty() {
        return None;
    }
    NaiveDate::parse_from_str(n.as_str(), "%Y-%m-%d").ok()
}

#[derive(Debug, thiserror::Error)]
pub enum BacklogStoreError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Read(#[from] BacklogReadError),
    #[error("project not found")]
    ProjectNotFound,
    #[error("admin spoke required")]
    AdminSpokeRequired,
    #[error("backlog not loaded for project")]
    NotLoaded,
    #[error("entity not found: {0}")]
    NotFound(String),
    #[error("{0}")]
    Msg(String),
}

#[derive(Debug, Clone)]
struct CachedBacklog {
    backlog_root: PathBuf,
    snapshot: BacklogSnapshot,
}

/// Per-project backlog snapshot cache (`RwLock<HashMap<…>>`).
#[derive(Debug, Clone)]
pub struct BacklogStore {
    data: Arc<RwLock<HashMap<ProjectId, CachedBacklog>>>,
}

impl Default for BacklogStore {
    fn default() -> Self {
        Self::new()
    }
}

impl BacklogStore {
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn lock_read(&self) -> Result<std::sync::RwLockReadGuard<'_, HashMap<ProjectId, CachedBacklog>>, BacklogStoreError> {
        self.data
            .read()
            .map_err(|_| BacklogStoreError::Msg("backlog store lock poisoned".into()))
    }

    fn lock_write(&self) -> Result<std::sync::RwLockWriteGuard<'_, HashMap<ProjectId, CachedBacklog>>, BacklogStoreError> {
        self.data
            .write()
            .map_err(|_| BacklogStoreError::Msg("backlog store lock poisoned".into()))
    }

    /// Load from disk and replace the cache entry for this project.
    pub fn load_backlog(
        &self,
        project_id: &ProjectId,
        backlog_root: &Path,
    ) -> Result<BacklogSnapshot, BacklogStoreError> {
        let snapshot = read_all(backlog_root)?;
        self.lock_write()?.insert(
            *project_id,
            CachedBacklog {
                backlog_root: backlog_root.to_path_buf(),
                snapshot: snapshot.clone(),
            },
        );
        Ok(snapshot)
    }

    pub fn get_snapshot(&self, project_id: &ProjectId) -> Option<BacklogSnapshot> {
        let g = self.data.read().ok()?;
        Some(g.get(project_id)?.snapshot.clone())
    }

    pub fn refresh(&self, project_id: &ProjectId) -> Result<(), BacklogStoreError> {
        let root = self
            .lock_read()?
            .get(project_id)
            .map(|c| c.backlog_root.clone())
            .ok_or(BacklogStoreError::NotLoaded)?;
        let snapshot = read_all(&root)?;
        let mut w = self.lock_write()?;
        let entry = w.get_mut(project_id).ok_or(BacklogStoreError::NotLoaded)?;
        entry.snapshot = snapshot;
        Ok(())
    }

    /// Drop cached backlog for a project (e.g. after unregister).
    pub fn unload(&self, project_id: &ProjectId) -> bool {
        self.lock_write()
            .ok()
            .map(|mut w| w.remove(project_id).is_some())
            .unwrap_or(false)
    }

    /// Load from disk if missing, using [`ProjectStore`] to resolve `path/backlog`.
    pub fn ensure_loaded(
        &self,
        project_id: &ProjectId,
        projects: &ProjectStore,
    ) -> Result<(), BacklogStoreError> {
        let mut w = self.lock_write()?;
        if w.contains_key(project_id) {
            return Ok(());
        }
        let p = projects
            .get(*project_id)
            .ok_or(BacklogStoreError::ProjectNotFound)?;
        let root = p
            .admin_backlog_root()
            .ok_or(BacklogStoreError::AdminSpokeRequired)?;
        let snapshot = read_all(&root)?;
        w.insert(
            *project_id,
            CachedBacklog {
                backlog_root: root,
                snapshot,
            },
        );
        Ok(())
    }

    fn backlog_root(&self, project_id: &ProjectId) -> Result<PathBuf, BacklogStoreError> {
        self.lock_read()?
            .get(project_id)
            .map(|c| c.backlog_root.clone())
            .ok_or(BacklogStoreError::NotLoaded)
    }

    fn join_safe(backlog_root: &Path, rel: &str) -> Result<PathBuf, BacklogStoreError> {
        let rel = rel.trim_start_matches(['/', '\\']);
        if rel.contains("..") {
            return Err(BacklogStoreError::Msg("invalid relative path".into()));
        }
        Ok(backlog_root.join(rel))
    }

    fn find_task_rel(snapshot: &BacklogSnapshot, task_id: &str) -> Option<String> {
        snapshot
            .tasks
            .iter()
            .find(|t| t.id == task_id)
            .and_then(|t| t.file_path.clone())
            .or_else(|| {
                snapshot
                    .drafts
                    .iter()
                    .find(|t| t.id == task_id)
                    .and_then(|t| t.file_path.clone())
            })
            .or_else(|| {
                snapshot
                    .completed
                    .iter()
                    .find(|t| t.id == task_id)
                    .and_then(|t| t.file_path.clone())
            })
    }

    fn find_task_rel_tasks_only(snapshot: &BacklogSnapshot, task_id: &str) -> Option<String> {
        snapshot
            .tasks
            .iter()
            .find(|t| t.id == task_id)
            .and_then(|t| t.file_path.clone())
    }

    /// Normalized task id prefix from backlog config (`TASK` when unset).
    pub fn effective_task_prefix(config: &BacklogConfig) -> String {
        let p = config.task_prefix.trim();
        if p.is_empty() {
            "TASK".to_string()
        } else {
            p.trim_matches('-').to_ascii_uppercase()
        }
    }

    /// Max numeric suffix for ids like `{PREFIX}-42`.
    pub fn max_num_for_prefix(snapshot: &BacklogSnapshot, prefix: &str) -> u32 {
        let p = prefix.trim().trim_matches('-').to_ascii_uppercase();
        let head = format!("{p}-");
        let mut m = 0u32;
        for t in snapshot
            .tasks
            .iter()
            .chain(snapshot.drafts.iter())
            .chain(snapshot.completed.iter())
        {
            let u = t.id.to_ascii_uppercase();
            if let Some(rest) = u.strip_prefix(&head) {
                if let Ok(n) = rest.parse::<u32>() {
                    m = m.max(n);
                }
            }
        }
        m
    }

    /// Active task clone from cache (`tasks/` only).
    pub fn get_active_task(
        &self,
        project_id: &ProjectId,
        task_id: &str,
    ) -> Option<BacklogTask> {
        let g = self.data.read().ok()?;
        let snap = &g.get(project_id)?.snapshot;
        snap.tasks.iter().find(|t| t.id == task_id).cloned()
    }

    /// Write a full task under `tasks/` (after in-memory merge).
    pub fn write_active_task(
        &self,
        project_id: &ProjectId,
        task: &BacklogTask,
        projects: &ProjectStore,
    ) -> Result<(), BacklogStoreError> {
        self.ensure_loaded(project_id, projects)?;
        let rel = {
            let g = self.lock_read()?;
            let snap = &g.get(project_id).ok_or(BacklogStoreError::NotLoaded)?.snapshot;
            Self::find_task_rel_tasks_only(snap, &task.id)
                .ok_or_else(|| BacklogStoreError::NotFound(task.id.clone()))?
        };
        let root = self.backlog_root(project_id)?;
        let path = Self::join_safe(&root, &rel)?;
        let mut t = task.clone();
        t.file_path = None;
        fs::write(&path, serialize_task(&t))?;
        self.refresh(project_id)
    }

    /// Move `task_id` to `target_status`, then set `ordinal` from order in `ordered_task_ids`.
    pub fn reorder_tasks_in_column(
        &self,
        project_id: &ProjectId,
        task_id: &str,
        target_status: &str,
        ordered_task_ids: &[String],
        projects: &ProjectStore,
    ) -> Result<(), BacklogStoreError> {
        self.update_task_status(project_id, task_id, target_status, projects)?;
        self.reorder_tasks(project_id, ordered_task_ids, projects)
    }

    pub fn update_task_status(
        &self,
        project_id: &ProjectId,
        task_id: &str,
        status: &str,
        projects: &ProjectStore,
    ) -> Result<(), BacklogStoreError> {
        self.ensure_loaded(project_id, projects)?;
        let rel = {
            let g = self.lock_read()?;
            let snap = &g.get(project_id).ok_or(BacklogStoreError::NotLoaded)?.snapshot;
            Self::find_task_rel(snap, task_id)
                .ok_or_else(|| BacklogStoreError::NotFound(task_id.to_string()))?
        };
        let root = self.backlog_root(project_id)?;
        let path = Self::join_safe(&root, &rel)?;
        let raw = fs::read_to_string(&path)?;
        fs::write(&path, update_task_status(&raw, status))?;
        self.refresh(project_id)
    }

    pub fn update_task_field(
        &self,
        project_id: &ProjectId,
        task_id: &str,
        key: &str,
        value: &str,
        projects: &ProjectStore,
    ) -> Result<(), BacklogStoreError> {
        self.ensure_loaded(project_id, projects)?;
        let rel = {
            let g = self.lock_read()?;
            let snap = &g.get(project_id).ok_or(BacklogStoreError::NotLoaded)?.snapshot;
            Self::find_task_rel(snap, task_id)
                .ok_or_else(|| BacklogStoreError::NotFound(task_id.to_string()))?
        };
        let root = self.backlog_root(project_id)?;
        let path = Self::join_safe(&root, &rel)?;
        let raw = fs::read_to_string(&path)?;
        fs::write(&path, update_frontmatter_field(&raw, key, value))?;
        self.refresh(project_id)
    }

    pub fn toggle_ac(
        &self,
        project_id: &ProjectId,
        task_id: &str,
        index: usize,
        projects: &ProjectStore,
    ) -> Result<(), BacklogStoreError> {
        self.ensure_loaded(project_id, projects)?;
        let rel = {
            let g = self.lock_read()?;
            let snap = &g.get(project_id).ok_or(BacklogStoreError::NotLoaded)?.snapshot;
            Self::find_task_rel(snap, task_id)
                .ok_or_else(|| BacklogStoreError::NotFound(task_id.to_string()))?
        };
        let root = self.backlog_root(project_id)?;
        let path = Self::join_safe(&root, &rel)?;
        let raw = fs::read_to_string(&path)?;
        fs::write(&path, toggle_acceptance_criterion(&raw, index))?;
        self.refresh(project_id)
    }

    pub fn create_task(
        &self,
        project_id: &ProjectId,
        mut task: BacklogTask,
        projects: &ProjectStore,
    ) -> Result<BacklogTask, BacklogStoreError> {
        self.ensure_loaded(project_id, projects)?;
        let (root, next, prefix) = {
            let g = self.lock_read()?;
            let ent = g.get(project_id).ok_or(BacklogStoreError::NotLoaded)?;
            let prefix = Self::effective_task_prefix(&ent.snapshot.config);
            let next = Self::max_num_for_prefix(&ent.snapshot, &prefix) + 1;
            (ent.backlog_root.clone(), next, prefix)
        };
        task.id = format!("{prefix}-{next}");
        task.file_path = None;
        let slug = slug_title(&task.title);
        let fname = format!("task-{next} - {slug}.md");
        let tasks_dir = root.join("tasks");
        fs::create_dir_all(&tasks_dir)?;
        let path = tasks_dir.join(&fname);
        if path.exists() {
            return Err(BacklogStoreError::Msg(format!("task file exists: {fname}")));
        }
        fs::write(&path, serialize_task(&task))?;
        self.refresh(project_id)?;
        self.get_active_task(project_id, &task.id)
            .ok_or_else(|| BacklogStoreError::Msg("created task missing after refresh".into()))
    }

    pub fn archive_task(
        &self,
        project_id: &ProjectId,
        task_id: &str,
        projects: &ProjectStore,
    ) -> Result<(), BacklogStoreError> {
        self.ensure_loaded(project_id, projects)?;
        let rel = {
            let g = self.lock_read()?;
            let snap = &g.get(project_id).ok_or(BacklogStoreError::NotLoaded)?.snapshot;
            Self::find_task_rel_tasks_only(snap, task_id)
                .ok_or_else(|| BacklogStoreError::NotFound(task_id.to_string()))?
        };
        if !rel.starts_with("tasks/") {
            return Err(BacklogStoreError::Msg(
                "archive_task only applies to tasks under tasks/".into(),
            ));
        }
        let root = self.backlog_root(project_id)?;
        let src = Self::join_safe(&root, &rel)?;
        let fname = src
            .file_name()
            .ok_or_else(|| BacklogStoreError::Msg("bad task path".into()))?;
        let dest_dir = root.join("archive").join("tasks");
        fs::create_dir_all(&dest_dir)?;
        let dest = dest_dir.join(fname);
        fs::rename(&src, &dest)?;
        self.refresh(project_id)
    }

    pub fn complete_task(
        &self,
        project_id: &ProjectId,
        task_id: &str,
        projects: &ProjectStore,
    ) -> Result<(), BacklogStoreError> {
        self.ensure_loaded(project_id, projects)?;
        let rel = {
            let g = self.lock_read()?;
            let snap = &g.get(project_id).ok_or(BacklogStoreError::NotLoaded)?.snapshot;
            Self::find_task_rel_tasks_only(snap, task_id)
                .ok_or_else(|| BacklogStoreError::NotFound(task_id.to_string()))?
        };
        if !rel.starts_with("tasks/") {
            return Err(BacklogStoreError::Msg(
                "complete_task only applies to tasks under tasks/".into(),
            ));
        }
        let root = self.backlog_root(project_id)?;
        let src = Self::join_safe(&root, &rel)?;
        let fname = src
            .file_name()
            .ok_or_else(|| BacklogStoreError::Msg("bad task path".into()))?;
        let dest_dir = root.join("completed");
        fs::create_dir_all(&dest_dir)?;
        let dest = dest_dir.join(fname);
        fs::rename(&src, &dest)?;
        self.refresh(project_id)
    }

    /// Done tasks still under `tasks/` whose reference date (`updated_date` or else `created_date`)
    /// is strictly before `today - older_than_days` (same rules as [Backlog.md](https://github.com/MrLesk/Backlog.md) cleanup preview).
    pub fn cleanup_done_tasks_preview(
        &self,
        project_id: &ProjectId,
        older_than_days: u32,
    ) -> Result<Vec<DoneTaskCleanupCandidate>, BacklogStoreError> {
        let g = self.lock_read()?;
        let ent = g.get(project_id).ok_or(BacklogStoreError::NotLoaded)?;
        let snap = &ent.snapshot;
        let cutoff = Utc::now().date_naive() - Duration::days(i64::from(older_than_days));
        let mut out = Vec::new();
        for t in &snap.tasks {
            if !t
                .file_path
                .as_deref()
                .is_some_and(|r| r.starts_with("tasks/"))
            {
                continue;
            }
            if !task_status_done_like_cleanup(&t.status) {
                continue;
            }
            let date_raw = t
                .updated_date
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(t.created_date.as_str());
            let date_raw = date_raw.trim();
            if date_raw.is_empty() {
                continue;
            }
            let Some(d) = parse_task_reference_date(date_raw) else {
                continue;
            };
            if d < cutoff {
                out.push(DoneTaskCleanupCandidate {
                    id: t.id.clone(),
                    title: t.title.clone(),
                    created_date: t.created_date.clone(),
                    updated_date: t.updated_date.clone(),
                });
            }
        }
        Ok(out)
    }

    /// Moves each matching task to `completed/` via [`Self::complete_task`] (Backlog.md `cleanup/execute`).
    pub fn cleanup_done_tasks_execute(
        &self,
        project_id: &ProjectId,
        older_than_days: u32,
        projects: &ProjectStore,
    ) -> Result<(usize, usize, Vec<String>), BacklogStoreError> {
        let candidates = self.cleanup_done_tasks_preview(project_id, older_than_days)?;
        let total = candidates.len();
        let mut moved = 0usize;
        let mut failed = Vec::new();
        for c in candidates {
            match self.complete_task(project_id, &c.id, projects) {
                Ok(()) => moved += 1,
                Err(_) => failed.push(c.id),
            }
        }
        Ok((moved, total, failed))
    }

    pub fn reorder_tasks(
        &self,
        project_id: &ProjectId,
        ordered_ids: &[String],
        projects: &ProjectStore,
    ) -> Result<(), BacklogStoreError> {
        self.ensure_loaded(project_id, projects)?;
        let root = self.backlog_root(project_id)?;
        for (ord, id) in ordered_ids.iter().enumerate() {
            let rel = {
                let g = self.lock_read()?;
                let snap = &g.get(project_id).ok_or(BacklogStoreError::NotLoaded)?.snapshot;
                Self::find_task_rel_tasks_only(snap, id.as_str())
                    .ok_or_else(|| BacklogStoreError::NotFound(id.clone()))?
            };
            let path = Self::join_safe(&root, &rel)?;
            let raw = fs::read_to_string(&path)?;
            fs::write(
                &path,
                update_frontmatter_i32(&raw, "ordinal", ord as i32),
            )?;
        }
        self.refresh(project_id)
    }

    /// Filename stem under `docs/` (id is usually `doc-1`, not a bare number).
    fn doc_filename_stem(id: &str) -> String {
        let u = id.trim();
        if u.to_ascii_lowercase().starts_with("doc-") {
            u.to_string()
        } else {
            format!("doc-{u}")
        }
    }

    fn doc_rel_for_write(doc: &BacklogDocument) -> Result<String, BacklogStoreError> {
        if let Some(ref fp) = doc.file_path {
            if fp.contains("..") {
                return Err(BacklogStoreError::Msg("invalid doc file_path".into()));
            }
            return Ok(fp.clone());
        }
        let sub = doc
            .path
            .as_deref()
            .map(|p| format!("{}/", p.trim_matches(['/', '\\'])))
            .unwrap_or_default();
        let stem = Self::doc_filename_stem(&doc.id);
        Ok(format!("docs/{sub}{stem}.md"))
    }

    fn next_doc_numeric_id(docs: &[BacklogDocument]) -> u32 {
        let mut m = 0u32;
        for d in docs {
            let u = d.id.to_ascii_lowercase();
            if let Some(rest) = u.strip_prefix("doc-") {
                if let Ok(n) = rest.parse::<u32>() {
                    m = m.max(n);
                }
            }
        }
        m + 1
    }

    /// Find a document in the cached snapshot by `id` (frontmatter).
    pub fn get_document(&self, project_id: &ProjectId, doc_id: &str) -> Option<BacklogDocument> {
        let g = self.data.read().ok()?;
        let snap = &g.get(project_id)?.snapshot;
        snap.documents.iter().find(|d| d.id == doc_id).cloned()
    }

    /// Create a doc under `docs/{category_path}/` with the next `doc-N` id; returns the loaded row.
    pub fn create_document(
        &self,
        project_id: &ProjectId,
        title: &str,
        doc_type: &str,
        category_path: &str,
        content: &str,
        projects: &ProjectStore,
    ) -> Result<BacklogDocument, BacklogStoreError> {
        let cat = category_path.trim().trim_matches(['/', '\\']);
        if cat.contains("..") {
            return Err(BacklogStoreError::Msg("invalid category_path".into()));
        }
        self.ensure_loaded(project_id, projects)?;
        let n = {
            let g = self.lock_read()?;
            let ent = g.get(project_id).ok_or(BacklogStoreError::NotLoaded)?;
            Self::next_doc_numeric_id(&ent.snapshot.documents)
        };
        let doc = BacklogDocument {
            id: format!("doc-{n}"),
            title: title.trim().to_string(),
            doc_type: if doc_type.trim().is_empty() {
                "guide".to_string()
            } else {
                doc_type.trim().to_string()
            },
            created_date: chrono::Utc::now().format("%Y-%m-%d").to_string(),
            updated_date: None,
            tags: None,
            raw_content: content.to_string(),
            path: if cat.is_empty() {
                None
            } else {
                Some(cat.to_string())
            },
            file_path: None,
        };
        let id = doc.id.clone();
        self.create_doc(project_id, &doc, projects)?;
        self.get_document(project_id, &id).ok_or_else(|| {
            BacklogStoreError::Msg("document missing after create".into())
        })
    }

    pub fn create_doc(
        &self,
        project_id: &ProjectId,
        doc: &BacklogDocument,
        projects: &ProjectStore,
    ) -> Result<(), BacklogStoreError> {
        self.ensure_loaded(project_id, projects)?;
        let root = self.backlog_root(project_id)?;
        let rel = Self::doc_rel_for_write(doc)?;
        let path = Self::join_safe(&root, &rel)?;
        if path.exists() {
            return Err(BacklogStoreError::Msg(format!("doc already exists: {rel}")));
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, serialize_document(doc))?;
        self.refresh(project_id)
    }

    pub fn update_doc(
        &self,
        project_id: &ProjectId,
        doc: &BacklogDocument,
        projects: &ProjectStore,
    ) -> Result<(), BacklogStoreError> {
        self.ensure_loaded(project_id, projects)?;
        let rel = {
            if let Some(ref fp) = doc.file_path {
                fp.clone()
            } else {
                let g = self.lock_read()?;
                let snap = &g.get(project_id).ok_or(BacklogStoreError::NotLoaded)?.snapshot;
                snap.documents
                    .iter()
                    .find(|d| d.id == doc.id)
                    .and_then(|d| d.file_path.clone())
                    .ok_or_else(|| BacklogStoreError::NotFound(doc.id.clone()))?
            }
        };
        let root = self.backlog_root(project_id)?;
        let path = Self::join_safe(&root, &rel)?;
        fs::write(&path, serialize_document(doc))?;
        self.refresh(project_id)
    }

    pub fn delete_document(
        &self,
        project_id: &ProjectId,
        doc_id: &str,
        projects: &ProjectStore,
    ) -> Result<(), BacklogStoreError> {
        self.ensure_loaded(project_id, projects)?;
        let rel = {
            let g = self.lock_read()?;
            let snap = &g.get(project_id).ok_or(BacklogStoreError::NotLoaded)?.snapshot;
            snap.documents
                .iter()
                .find(|d| d.id == doc_id)
                .and_then(|d| d.file_path.clone())
                .ok_or_else(|| BacklogStoreError::NotFound(doc_id.to_string()))?
        };
        if !rel.starts_with("docs/") {
            return Err(BacklogStoreError::Msg(
                "delete_document only for files under docs/".into(),
            ));
        }
        let root = self.backlog_root(project_id)?;
        let path = Self::join_safe(&root, &rel)?;
        if !path.is_file() {
            return Err(BacklogStoreError::NotFound(doc_id.to_string()));
        }
        fs::remove_file(&path)?;
        self.refresh(project_id)
    }

    pub fn create_decision(
        &self,
        project_id: &ProjectId,
        decision: &BacklogDecision,
        projects: &ProjectStore,
    ) -> Result<(), BacklogStoreError> {
        self.ensure_loaded(project_id, projects)?;
        let root = self.backlog_root(project_id)?;
        let slug = slug_title(&decision.id);
        let rel = format!("decisions/decision-{slug}.md");
        let path = Self::join_safe(&root, &rel)?;
        if path.exists() {
            return Err(BacklogStoreError::Msg(format!(
                "decision file exists: {rel}"
            )));
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, serialize_decision(decision))?;
        self.refresh(project_id)
    }

    pub fn update_decision(
        &self,
        project_id: &ProjectId,
        decision: &BacklogDecision,
        projects: &ProjectStore,
    ) -> Result<(), BacklogStoreError> {
        self.ensure_loaded(project_id, projects)?;
        let rel = {
            if let Some(ref fp) = decision.file_path {
                fp.clone()
            } else {
                let g = self.lock_read()?;
                let snap = &g.get(project_id).ok_or(BacklogStoreError::NotLoaded)?.snapshot;
                snap.decisions
                    .iter()
                    .find(|d| d.id == decision.id)
                    .and_then(|d| d.file_path.clone())
                    .ok_or_else(|| BacklogStoreError::NotFound(decision.id.clone()))?
            }
        };
        let root = self.backlog_root(project_id)?;
        let path = Self::join_safe(&root, &rel)?;
        fs::write(&path, serialize_decision(decision))?;
        self.refresh(project_id)
    }

    pub fn delete_decision(
        &self,
        project_id: &ProjectId,
        decision_id: &str,
        projects: &ProjectStore,
    ) -> Result<(), BacklogStoreError> {
        self.ensure_loaded(project_id, projects)?;
        let rel = {
            let g = self.lock_read()?;
            let snap = &g.get(project_id).ok_or(BacklogStoreError::NotLoaded)?.snapshot;
            snap.decisions
                .iter()
                .find(|d| d.id == decision_id)
                .and_then(|d| d.file_path.clone())
                .ok_or_else(|| BacklogStoreError::NotFound(decision_id.to_string()))?
        };
        if !rel.starts_with("decisions/") {
            return Err(BacklogStoreError::Msg(
                "delete_decision only for files under decisions/".into(),
            ));
        }
        let root = self.backlog_root(project_id)?;
        let path = Self::join_safe(&root, &rel)?;
        if !path.is_file() {
            return Err(BacklogStoreError::NotFound(decision_id.to_string()));
        }
        fs::remove_file(&path)?;
        self.refresh(project_id)
    }

    pub fn get_decision(&self, project_id: &ProjectId, decision_id: &str) -> Option<BacklogDecision> {
        let g = self.data.read().ok()?;
        let snap = &g.get(project_id)?.snapshot;
        snap.decisions
            .iter()
            .find(|d| d.id == decision_id)
            .cloned()
    }

    pub fn get_draft(&self, project_id: &ProjectId, draft_id: &str) -> Option<BacklogTask> {
        let g = self.data.read().ok()?;
        let snap = &g.get(project_id)?.snapshot;
        snap.drafts.iter().find(|t| t.id == draft_id).cloned()
    }

    pub fn get_milestone(&self, project_id: &ProjectId, milestone_id: &str) -> Option<BacklogMilestone> {
        let g = self.data.read().ok()?;
        let snap = &g.get(project_id)?.snapshot;
        snap.milestones
            .iter()
            .find(|m| m.id == milestone_id)
            .cloned()
    }

    fn next_decision_num(decisions: &[BacklogDecision]) -> u32 {
        let mut m = 0u32;
        for d in decisions {
            let u = d.id.to_ascii_uppercase();
            if let Some(r) = u.strip_prefix("DEC-") {
                if let Ok(n) = r.parse::<u32>() {
                    m = m.max(n);
                }
            }
        }
        m + 1
    }

    /// New decision `DEC-{n}` with empty ADR sections.
    pub fn create_decision_with_title(
        &self,
        project_id: &ProjectId,
        title: &str,
        projects: &ProjectStore,
    ) -> Result<BacklogDecision, BacklogStoreError> {
        let title = title.trim();
        if title.is_empty() {
            return Err(BacklogStoreError::Msg("title must not be empty".into()));
        }
        self.ensure_loaded(project_id, projects)?;
        let n = {
            let g = self.lock_read()?;
            let ent = g.get(project_id).ok_or(BacklogStoreError::NotLoaded)?;
            Self::next_decision_num(&ent.snapshot.decisions)
        };
        let id = format!("DEC-{n}");
        let decision = BacklogDecision {
            id: id.clone(),
            title: title.to_string(),
            date: chrono::Utc::now().format("%Y-%m-%d").to_string(),
            status: DecisionStatus::Proposed,
            context: String::new(),
            decision: String::new(),
            consequences: String::new(),
            alternatives: None,
            raw_content: String::new(),
            file_path: None,
        };
        self.create_decision(project_id, &decision, projects)?;
        self.get_decision(project_id, &id).ok_or_else(|| {
            BacklogStoreError::Msg("decision missing after create".into())
        })
    }

    fn next_milestone_num(active: &[BacklogMilestone], archived: &[BacklogMilestone]) -> u32 {
        let mut m = 0u32;
        for x in active.iter().chain(archived.iter()) {
            let u = x.id.to_ascii_uppercase();
            if let Some(r) = u.strip_prefix("MS-") {
                if let Ok(n) = r.parse::<u32>() {
                    m = m.max(n);
                }
            }
        }
        m + 1
    }

    pub fn create_milestone(
        &self,
        project_id: &ProjectId,
        title: &str,
        description: &str,
        projects: &ProjectStore,
    ) -> Result<BacklogMilestone, BacklogStoreError> {
        let title = title.trim();
        if title.is_empty() {
            return Err(BacklogStoreError::Msg("title must not be empty".into()));
        }
        self.ensure_loaded(project_id, projects)?;
        let (root, n) = {
            let g = self.lock_read()?;
            let ent = g.get(project_id).ok_or(BacklogStoreError::NotLoaded)?;
            let n = Self::next_milestone_num(&ent.snapshot.milestones, &ent.snapshot.archived_milestones);
            (ent.backlog_root.clone(), n)
        };
        let id = format!("MS-{n}");
        let slug = slug_title(title);
        let fname = format!("milestone-{n} - {slug}.md");
        let ms_dir = root.join("milestones");
        fs::create_dir_all(&ms_dir)?;
        let path = ms_dir.join(&fname);
        if path.exists() {
            return Err(BacklogStoreError::Msg(format!(
                "milestone file exists: {fname}"
            )));
        }
        let ms = BacklogMilestone {
            id: id.clone(),
            title: title.to_string(),
            description: description.to_string(),
            raw_content: String::new(),
            file_path: None,
        };
        fs::write(&path, serialize_milestone(&ms))?;
        self.refresh(project_id)?;
        self.get_milestone(project_id, &id).ok_or_else(|| {
            BacklogStoreError::Msg("milestone missing after create".into())
        })
    }

    pub fn update_milestone(
        &self,
        project_id: &ProjectId,
        milestone: &BacklogMilestone,
        projects: &ProjectStore,
    ) -> Result<(), BacklogStoreError> {
        self.ensure_loaded(project_id, projects)?;
        let rel = {
            if let Some(ref fp) = milestone.file_path {
                fp.clone()
            } else {
                let g = self.lock_read()?;
                let snap = &g.get(project_id).ok_or(BacklogStoreError::NotLoaded)?.snapshot;
                snap.milestones
                    .iter()
                    .find(|m| m.id == milestone.id)
                    .and_then(|m| m.file_path.clone())
                    .ok_or_else(|| BacklogStoreError::NotFound(milestone.id.clone()))?
            }
        };
        if !rel.starts_with("milestones/") {
            return Err(BacklogStoreError::Msg(
                "update_milestone only for milestones under milestones/".into(),
            ));
        }
        let root = self.backlog_root(project_id)?;
        let path = Self::join_safe(&root, &rel)?;
        fs::write(&path, serialize_milestone(milestone))?;
        self.refresh(project_id)
    }

    pub fn delete_milestone(
        &self,
        project_id: &ProjectId,
        milestone_id: &str,
        projects: &ProjectStore,
    ) -> Result<(), BacklogStoreError> {
        self.ensure_loaded(project_id, projects)?;
        let rel = {
            let g = self.lock_read()?;
            let snap = &g.get(project_id).ok_or(BacklogStoreError::NotLoaded)?.snapshot;
            snap.milestones
                .iter()
                .find(|m| m.id == milestone_id)
                .and_then(|m| m.file_path.clone())
                .or_else(|| {
                    snap.archived_milestones
                        .iter()
                        .find(|m| m.id == milestone_id)
                        .and_then(|m| m.file_path.clone())
                })
                .ok_or_else(|| BacklogStoreError::NotFound(milestone_id.to_string()))?
        };
        let ok = rel.starts_with("milestones/") || rel.starts_with("archive/milestones/");
        if !ok {
            return Err(BacklogStoreError::Msg(
                "delete_milestone only for milestones/ or archive/milestones/".into(),
            ));
        }
        let root = self.backlog_root(project_id)?;
        let path = Self::join_safe(&root, &rel)?;
        if !path.is_file() {
            return Err(BacklogStoreError::NotFound(milestone_id.to_string()));
        }
        fs::remove_file(&path)?;
        self.refresh(project_id)
    }

    pub fn promote_draft(
        &self,
        project_id: &ProjectId,
        draft_id: &str,
        projects: &ProjectStore,
    ) -> Result<(), BacklogStoreError> {
        self.ensure_loaded(project_id, projects)?;
        let (root, rel, title) = {
            let g = self.lock_read()?;
            let ent = g.get(project_id).ok_or(BacklogStoreError::NotLoaded)?;
            let snap = &ent.snapshot;
            let d = snap
                .drafts
                .iter()
                .find(|t| t.id == draft_id)
                .ok_or_else(|| BacklogStoreError::NotFound(draft_id.to_string()))?;
            let rel = d
                .file_path
                .clone()
                .ok_or_else(|| BacklogStoreError::Msg("draft missing file_path".into()))?;
            (ent.backlog_root.clone(), rel, d.title.clone())
        };
        if !rel.starts_with("drafts/") {
            return Err(BacklogStoreError::Msg("promote_draft expects drafts/ path".into()));
        }
        let draft_path = Self::join_safe(&root, &rel)?;
        let raw = fs::read_to_string(&draft_path)?;
        let (next, prefix) = {
            let g = self.lock_read()?;
            let ent = g.get(project_id).ok_or(BacklogStoreError::NotLoaded)?;
            let prefix = Self::effective_task_prefix(&ent.snapshot.config);
            let next = Self::max_num_for_prefix(&ent.snapshot, &prefix) + 1;
            (next, prefix)
        };
        let new_id = format!("{prefix}-{next}");
        let mut updated = update_frontmatter_field(&raw, "id", &new_id);
        updated = update_task_status(&updated, "Open");
        let slug = slug_title(&title);
        let fname = format!("task-{next} - {slug}.md");
        let tasks_dir = root.join("tasks");
        fs::create_dir_all(&tasks_dir)?;
        let dest = tasks_dir.join(&fname);
        if dest.exists() {
            return Err(BacklogStoreError::Msg(format!(
                "target task file exists: {fname}"
            )));
        }
        fs::write(&dest, updated)?;
        fs::remove_file(&draft_path)?;
        self.refresh(project_id)
    }

    pub fn archive_milestone(
        &self,
        project_id: &ProjectId,
        milestone_id: &str,
        projects: &ProjectStore,
    ) -> Result<(), BacklogStoreError> {
        self.ensure_loaded(project_id, projects)?;
        let rel = {
            let g = self.lock_read()?;
            let snap = &g.get(project_id).ok_or(BacklogStoreError::NotLoaded)?.snapshot;
            snap.milestones
                .iter()
                .find(|m| m.id == milestone_id)
                .and_then(|m| m.file_path.clone())
                .ok_or_else(|| BacklogStoreError::NotFound(milestone_id.to_string()))?
        };
        if !rel.starts_with("milestones/") {
            return Err(BacklogStoreError::Msg(
                "archive_milestone only for milestones under milestones/".into(),
            ));
        }
        let root = self.backlog_root(project_id)?;
        let src = Self::join_safe(&root, &rel)?;
        let fname = src
            .file_name()
            .ok_or_else(|| BacklogStoreError::Msg("bad milestone path".into()))?;
        let dest_dir = root.join("archive").join("milestones");
        fs::create_dir_all(&dest_dir)?;
        let dest = dest_dir.join(fname);
        fs::rename(&src, &dest)?;
        self.refresh(project_id)
    }
}

fn slug_title(s: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for c in s.chars().take(80) {
        let c = if c.is_ascii_alphanumeric() {
            c.to_ascii_lowercase()
        } else {
            '-'
        };
        if c == '-' {
            if !out.is_empty() && !prev_dash {
                out.push('-');
            }
            prev_dash = true;
        } else {
            out.push(c);
            prev_dash = false;
        }
    }
    let t = out.trim_matches('-').to_string();
    if t.is_empty() {
        "item".into()
    } else {
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openfang_types::project::{Project, SpokeDescriptor};
    use std::io::Write;
    use tempfile::tempdir;

    fn write(p: &Path, s: &str) {
        let parent = p.parent().unwrap();
        fs::create_dir_all(parent).unwrap();
        fs::File::create(p).unwrap().write_all(s.as_bytes()).unwrap();
    }

    fn admin_backlog_root(repo: &Path) -> PathBuf {
        repo.join("admin").join("backlog")
    }

    fn sample_project(repo: &Path, store: &ProjectStore) -> ProjectId {
        fs::create_dir_all(repo).unwrap();
        let p = Project {
            path: repo.to_path_buf(),
            name: "p".into(),
            spokes: vec![SpokeDescriptor {
                name: "admin".into(),
                path: PathBuf::from("admin"),
                labels: vec![],
            }],
            admin_spoke: Some("admin".into()),
            ..Default::default()
        };
        store.register(p).unwrap()
    }

    #[test]
    fn load_get_refresh() {
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let store = ProjectStore::new(&home);
        let repo = tmp.path().join("repo");
        let root = admin_backlog_root(&repo);
        write(
            &root.join("tasks/task-1 - t.md"),
            "---\nid: TASK-1\ntitle: T\nstatus: Open\ncreated_date: 2026-01-01\n---\n\n",
        );
        let pid = sample_project(&repo, &store);
        let bs = BacklogStore::new();
        bs.load_backlog(&pid, &root).unwrap();
        assert_eq!(bs.get_snapshot(&pid).unwrap().tasks.len(), 1);
        write(
            &root.join("tasks/task-2 - u.md"),
            "---\nid: TASK-2\ntitle: U\nstatus: Open\ncreated_date: 2026-01-01\n---\n\n",
        );
        bs.refresh(&pid).unwrap();
        assert_eq!(bs.get_snapshot(&pid).unwrap().tasks.len(), 2);
    }

    #[test]
    fn update_status_and_create_task() {
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let pstore = ProjectStore::new(&home);
        let repo = tmp.path().join("repo");
        let root = admin_backlog_root(&repo);
        write(
            &root.join("tasks/task-1 - t.md"),
            "---\nid: TASK-1\ntitle: T\nstatus: Open\ncreated_date: 2026-01-01\n---\n\n",
        );
        let pid = sample_project(&repo, &pstore);
        let bs = BacklogStore::new();
        bs.load_backlog(&pid, &root).unwrap();
        bs.update_task_status(&pid, "TASK-1", "Done", &pstore).unwrap();
        let raw = fs::read_to_string(root.join("tasks/task-1 - t.md")).unwrap();
        assert!(raw.contains("status: Done"));
        let nt = BacklogTask {
            title: "New".into(),
            status: "Open".into(),
            created_date: "2026-01-02".into(),
            ..Default::default()
        };
        let created = bs.create_task(&pid, nt, &pstore).unwrap();
        assert_eq!(created.id, "TASK-2");
        assert_eq!(bs.get_snapshot(&pid).unwrap().tasks.len(), 2);
    }

    #[test]
    fn promote_draft_and_archive_milestone() {
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let pstore = ProjectStore::new(&home);
        let repo = tmp.path().join("repo");
        let root = admin_backlog_root(&repo);
        write(
            &root.join("drafts/draft-1.md"),
            "---\nid: DRAFT-1\ntitle: Dr\nstatus: Draft\ncreated_date: 2026-01-01\n---\n\n",
        );
        write(
            &root.join("milestones/milestone-1.md"),
            "---\nid: MS-1\ntitle: M\n---\n\n## Description\n\nx\n",
        );
        let pid = sample_project(&repo, &pstore);
        let bs = BacklogStore::new();
        bs.load_backlog(&pid, &root).unwrap();
        bs.promote_draft(&pid, "DRAFT-1", &pstore).unwrap();
        assert!(!root.join("drafts/draft-1.md").exists());
        assert!(root.join("tasks").read_dir().unwrap().count() >= 1);
        bs.archive_milestone(&pid, "MS-1", &pstore).unwrap();
        assert!(root
            .join("archive/milestones")
            .read_dir()
            .unwrap()
            .count()
            >= 1);
    }

    #[test]
    fn create_document_writes_doc_n_under_category() {
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let pstore = ProjectStore::new(&home);
        let repo = tmp.path().join("repo");
        let root = admin_backlog_root(&repo);
        fs::create_dir_all(&root).unwrap();
        let pid = sample_project(&repo, &pstore);
        let bs = BacklogStore::new();
        bs.load_backlog(&pid, &root).unwrap();
        let d = bs
            .create_document(
                &pid,
                "Title",
                "reference",
                "overview/architecture",
                "# Hello",
                &pstore,
            )
            .unwrap();
        assert_eq!(d.id, "doc-1");
        assert_eq!(
            d.file_path.as_deref(),
            Some("docs/overview/architecture/doc-1.md")
        );
        assert!(root
            .join("docs/overview/architecture/doc-1.md")
            .is_file());
    }

    #[test]
    fn create_decision_and_milestone() {
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let pstore = ProjectStore::new(&home);
        let repo = tmp.path().join("repo");
        let root = admin_backlog_root(&repo);
        fs::create_dir_all(&root).unwrap();
        let pid = sample_project(&repo, &pstore);
        let bs = BacklogStore::new();
        bs.load_backlog(&pid, &root).unwrap();
        let d = bs
            .create_decision_with_title(&pid, "Choose Rust", &pstore)
            .unwrap();
        assert!(d.id.starts_with("DEC-"));
        let m = bs
            .create_milestone(&pid, "v1", "Ship it", &pstore)
            .unwrap();
        assert_eq!(m.id, "MS-1");
        assert!(root.join("milestones").read_dir().unwrap().count() >= 1);
    }

    #[test]
    fn ensure_loaded_requires_admin_spoke() {
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let store = ProjectStore::new(&home);
        let repo = tmp.path().join("repo");
        fs::create_dir_all(&repo).unwrap();
        let pid = store
            .register(Project {
                path: repo,
                name: "p".into(),
                ..Default::default()
            })
            .unwrap();
        let bs = BacklogStore::new();
        let err = bs.ensure_loaded(&pid, &store).unwrap_err();
        assert!(matches!(err, BacklogStoreError::AdminSpokeRequired));
    }
}
