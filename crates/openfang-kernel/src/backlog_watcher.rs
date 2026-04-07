//! Recursive `notify` watcher for each loaded project `backlog/` tree.
//!
//! Debounces filesystem events (500ms), classifies paths, refreshes [`crate::backlog_store::BacklogStore`],
//! then invokes a sink (dashboard broadcast).

use crate::backlog_store::BacklogStore;
use crossbeam::channel::{select, unbounded, RecvTimeoutError};
use notify::event::{EventKind, MetadataKind, ModifyKind};
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use openfang_types::project::ProjectId;
use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

const DEBOUNCE: Duration = Duration::from_millis(500);

/// Classify a path relative to `backlog/` (POSIX-style, lowercased prefix match).
pub fn backlog_entity_for_rel_path(rel: &str) -> &'static str {
    let r = rel.replace('\\', "/");
    let r = r.trim_start_matches('/').to_ascii_lowercase();
    if r == "backlog.config.yml" || r.ends_with("/backlog.config.yml") {
        return "config";
    }
    if r.starts_with("tasks/") {
        return "task";
    }
    if r.starts_with("docs/") {
        return "doc";
    }
    if r.starts_with("decisions/") {
        return "decision";
    }
    if r.starts_with("drafts/") {
        return "draft";
    }
    if r.starts_with("milestones/") {
        return "milestone";
    }
    if r.starts_with("completed/") {
        return "completed";
    }
    if r.starts_with("archive/milestones/") {
        return "milestone";
    }
    "unknown"
}

/// Non-mutating filesystem notifications (and atime bumps from our own `read_all` during refresh).
/// Without this, `refresh` → read tree → access/atime events → `refresh` loops at ~debounce interval.
fn ignore_notify_event_kind(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Access(_) | EventKind::Modify(ModifyKind::Metadata(MetadataKind::AccessTime))
    )
}

fn ignore_event_path(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
        return true;
    };
    if name.starts_with(".#") {
        return true;
    }
    if name.ends_with('~')
        || name.ends_with(".tmp")
        || name.ends_with(".swp")
        || name.ends_with(".swx")
        || name == ".DS_Store"
    {
        return true;
    }
    false
}

struct WatcherEntry {
    stop_tx: crossbeam::channel::Sender<()>,
    thread: JoinHandle<()>,
}

/// Manages one OS watcher thread per project.
pub struct BacklogWatcherManager {
    sink: Arc<dyn Fn(ProjectId, &'static str) + Send + Sync>,
    entries: Mutex<std::collections::HashMap<ProjectId, WatcherEntry>>,
}

impl BacklogWatcherManager {
    pub fn new(sink: Arc<dyn Fn(ProjectId, &'static str) + Send + Sync>) -> Self {
        Self {
            sink,
            entries: Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Start watching `backlog_root` if not already watching this project.
    pub fn ensure_watching(
        &self,
        project_id: ProjectId,
        backlog_root: PathBuf,
        store: Arc<BacklogStore>,
    ) {
        let mut g = self.entries.lock().unwrap();
        if g.contains_key(&project_id) {
            return;
        }
        let root_watch = match std::fs::canonicalize(&backlog_root) {
            Ok(p) => p,
            Err(_) => backlog_root.clone(),
        };
        let (event_tx, event_rx) = unbounded::<PathBuf>();
        let (stop_tx, stop_rx) = unbounded::<()>();
        let sink = Arc::clone(&self.sink);
        let thread = std::thread::spawn(move || {
            let mut watcher = match RecommendedWatcher::new(
                move |res: notify::Result<Event>| {
                    if let Ok(ev) = res {
                        if ignore_notify_event_kind(&ev.kind) {
                            return;
                        }
                        for p in ev.paths {
                            let _ = event_tx.send(p);
                        }
                    }
                },
                Config::default(),
            ) {
                Ok(w) => w,
                Err(e) => {
                    warn!(?e, "backlog RecommendedWatcher create failed");
                    return;
                }
            };
            if let Err(e) = watcher.watch(&root_watch, RecursiveMode::Recursive) {
                warn!(?e, path = %root_watch.display(), "backlog watch() failed");
                return;
            }
            loop {
                select! {
                    recv(stop_rx) -> _ => break,
                    recv(event_rx) -> msg => {
                        let Ok(first_path) = msg else { break };
                        let mut batch = vec![first_path];
                        let deadline = Instant::now() + DEBOUNCE;
                        loop {
                            let wait = deadline.saturating_duration_since(Instant::now());
                            if wait.is_zero() {
                                break;
                            }
                            match event_rx.recv_timeout(wait) {
                                Ok(p) => batch.push(p),
                                Err(RecvTimeoutError::Timeout) => break,
                                Err(RecvTimeoutError::Disconnected) => break,
                            }
                        }
                        let raw_batch_len = batch.len();
                        let mut kinds: HashSet<&'static str> = HashSet::new();
                        let mut trigger_rels: BTreeSet<String> = BTreeSet::new();
                        let mut skipped_ignore = 0u32;
                        let mut skipped_prefix = 0u32;
                        let mut skipped_git = 0u32;
                        for p in batch {
                            if ignore_event_path(&p) {
                                skipped_ignore += 1;
                                continue;
                            }
                            let rel = match p.strip_prefix(&root_watch) {
                                Ok(r) => r,
                                Err(_) => {
                                    skipped_prefix += 1;
                                    continue;
                                }
                            };
                            let rel_str = rel.to_string_lossy();
                            if rel_str.contains(".git/") || rel_str.contains(".git\\") {
                                skipped_git += 1;
                                continue;
                            }
                            let norm = rel_str
                                .trim_start_matches(['/', '\\'])
                                .replace('\\', "/");
                            trigger_rels.insert(norm);
                            kinds.insert(backlog_entity_for_rel_path(
                                rel_str.trim_start_matches(['/', '\\']),
                            ));
                        }
                        if trigger_rels.is_empty() {
                            debug!(
                                ?project_id,
                                raw_batch_len,
                                skipped_ignore,
                                skipped_prefix,
                                skipped_git,
                                "backlog watch: debounced batch produced no actionable paths"
                            );
                            continue;
                        }
                        if let Err(e) = store.refresh(&project_id) {
                            warn!(%e, "backlog refresh after watch event failed");
                            continue;
                        }
                        let entity: &'static str = if kinds.is_empty() {
                            "unknown"
                        } else if kinds.len() == 1 {
                            kinds.iter().next().expect("one element")
                        } else {
                            "mixed"
                        };
                        let paths_vec: Vec<String> = trigger_rels.iter().cloned().collect();
                        let total_paths = paths_vec.len();
                        const LOG_PATH_CAP: usize = 32;
                        let preview: Vec<String> = paths_vec.iter().take(LOG_PATH_CAP).cloned().collect();
                        let omitted = total_paths.saturating_sub(LOG_PATH_CAP);
                        info!(
                            project_id = %project_id,
                            entity_type = entity,
                            raw_notify_events = raw_batch_len,
                            actionable_paths = total_paths,
                            skipped_ignored_name = skipped_ignore,
                            skipped_outside_backlog_root = skipped_prefix,
                            skipped_under_dot_git = skipped_git,
                            paths_preview = %preview.join(", "),
                            omitted_paths = omitted,
                            backlog_root = %root_watch.display(),
                            "backlog watch: refresh + backlog-updated broadcast (paths_preview = relative paths under backlog_root)"
                        );
                        sink(project_id, entity);
                        debug!(?project_id, %entity, "backlog store refreshed from filesystem watch");
                    }
                }
            }
            drop(watcher);
        });
        g.insert(
            project_id,
            WatcherEntry {
                stop_tx,
                thread,
            },
        );
    }

    pub fn stop_watching(&self, project_id: &ProjectId) {
        let mut g = self.entries.lock().unwrap();
        if let Some(entry) = g.remove(project_id) {
            let _ = entry.stop_tx.send(());
            if let Err(e) = entry.thread.join() {
                warn!(?e, "backlog watcher thread join error");
            }
        }
    }

    pub fn stop_all(&self) {
        let mut g = self.entries.lock().unwrap();
        for (_, entry) in g.drain() {
            let _ = entry.stop_tx.send(());
            let _ = entry.thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{AccessKind, DataChange};

    #[test]
    fn ignore_notify_skips_access_and_atime_metadata() {
        assert!(ignore_notify_event_kind(&EventKind::Access(AccessKind::Read)));
        assert!(ignore_notify_event_kind(&EventKind::Modify(ModifyKind::Metadata(
            MetadataKind::AccessTime,
        ))));
        assert!(!ignore_notify_event_kind(&EventKind::Modify(ModifyKind::Data(
            DataChange::Content,
        ))));
        assert!(!ignore_notify_event_kind(&EventKind::Create(
            notify::event::CreateKind::File,
        )));
    }

    #[test]
    fn classify_rel_paths() {
        assert_eq!(backlog_entity_for_rel_path("tasks/foo.md"), "task");
        assert_eq!(backlog_entity_for_rel_path(r"docs\guides\x.md"), "doc");
        assert_eq!(backlog_entity_for_rel_path("decisions/d.md"), "decision");
        assert_eq!(backlog_entity_for_rel_path("drafts/d.md"), "draft");
        assert_eq!(backlog_entity_for_rel_path("milestones/m.md"), "milestone");
        assert_eq!(backlog_entity_for_rel_path("completed/t.md"), "completed");
        assert_eq!(
            backlog_entity_for_rel_path("archive/milestones/old.md"),
            "milestone"
        );
        assert_eq!(backlog_entity_for_rel_path("backlog.config.yml"), "config");
        assert_eq!(backlog_entity_for_rel_path("other/x"), "unknown");
    }
}
