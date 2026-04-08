//! Walk a `backlog/` tree and load config, tasks, docs, decisions, drafts, milestones, completed, archive.

use super::parser::{parse_decision, parse_document, parse_milestone, parse_task};
use super::{
    BacklogConfig, BacklogDecision, BacklogDocument, BacklogMilestone, BacklogSnapshot,
    BacklogTask, DocTree, DocTreeNode,
};
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

/// I/O or config YAML errors while reading backlog files.
#[derive(Debug, thiserror::Error)]
pub enum BacklogReadError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Yaml(#[from] serde_yaml::Error),
}

fn is_md(path: &Path) -> bool {
    path.extension()
        .map(|e| e.eq_ignore_ascii_case("md"))
        .unwrap_or(false)
}

fn name_has_prefix(name: &str, prefix: &str) -> bool {
    let n = name.to_ascii_lowercase();
    let p = prefix.to_ascii_lowercase();
    n.starts_with(&p) && n.ends_with(".md")
}

fn rel_from_backlog(backlog_root: &Path, path: &Path) -> String {
    path.strip_prefix(backlog_root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn read_task_files(
    backlog_root: &Path,
    subdir: &str,
    file_prefix: &str,
) -> Result<Vec<BacklogTask>, BacklogReadError> {
    let dir = backlog_root.join(subdir);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for ent in WalkDir::new(&dir).into_iter().filter_map(|e| e.ok()) {
        let path = ent.path();
        if !path.is_file() || !is_md(path) {
            continue;
        }
        let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name_has_prefix(fname, file_prefix) {
            continue;
        }
        let Ok(raw) = fs::read_to_string(path) else {
            continue;
        };
        let Ok(mut t) = parse_task(&raw) else {
            continue;
        };
        t.file_path = Some(rel_from_backlog(backlog_root, path));
        out.push(t);
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

/// Parse `config.yml` under the backlog root (default config if missing).
pub fn read_backlog_config(backlog_root: &Path) -> Result<BacklogConfig, BacklogReadError> {
    let p = backlog_root.join("config.yml");
    if !p.is_file() {
        return Ok(BacklogConfig::default());
    }
    let raw = fs::read_to_string(&p)?;
    Ok(serde_yaml::from_str(&raw)?)
}

/// All `task-*.md` under `tasks/`.
pub fn read_tasks(backlog_root: &Path) -> Result<Vec<BacklogTask>, BacklogReadError> {
    read_task_files(backlog_root, "tasks", "task-")
}

/// Recursive `doc-*.md` under `docs/`; [`BacklogDocument::path`] is parent dir relative to `docs/`.
pub fn read_documents(backlog_root: &Path) -> Result<Vec<BacklogDocument>, BacklogReadError> {
    let docs_root = backlog_root.join("docs");
    if !docs_root.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for ent in WalkDir::new(&docs_root).into_iter().filter_map(|e| e.ok()) {
        let path = ent.path();
        if !path.is_file() || !is_md(path) {
            continue;
        }
        let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name_has_prefix(fname, "doc-") {
            continue;
        }
        let Ok(raw) = fs::read_to_string(path) else {
            continue;
        };
        let Ok(mut d) = parse_document(&raw) else {
            continue;
        };
        let rel_from_docs = path.strip_prefix(&docs_root).unwrap_or(path);
        let parent = rel_from_docs.parent().unwrap_or(Path::new(""));
        let cat = parent
            .to_string_lossy()
            .replace('\\', "/")
            .trim_matches('/')
            .to_string();
        d.path = if cat.is_empty() { None } else { Some(cat) };
        d.file_path = Some(rel_from_backlog(backlog_root, path));
        out.push(d);
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

fn read_decisions_dir(backlog_root: &Path, dir: &Path) -> Vec<BacklogDecision> {
    if !dir.is_dir() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for ent in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        let path = ent.path();
        if !path.is_file() || !is_md(path) {
            continue;
        }
        let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name_has_prefix(fname, "decision-") {
            continue;
        }
        let Ok(raw) = fs::read_to_string(path) else {
            continue;
        };
        if let Ok(mut d) = parse_decision(&raw) {
            d.file_path = Some(rel_from_backlog(backlog_root, path));
            out.push(d);
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// `decision-*.md` under `decisions/`.
pub fn read_decisions(backlog_root: &Path) -> Result<Vec<BacklogDecision>, BacklogReadError> {
    Ok(read_decisions_dir(
        backlog_root,
        &backlog_root.join("decisions"),
    ))
}

/// `draft-*.md` under `drafts/` (same shape as tasks).
pub fn read_drafts(backlog_root: &Path) -> Result<Vec<BacklogTask>, BacklogReadError> {
    read_task_files(backlog_root, "drafts", "draft-")
}

fn read_milestones_dir(backlog_root: &Path, dir: &Path) -> Vec<BacklogMilestone> {
    if !dir.is_dir() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for ent in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        let path = ent.path();
        if !path.is_file() || !is_md(path) {
            continue;
        }
        let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name_has_prefix(fname, "milestone-") {
            continue;
        }
        let Ok(raw) = fs::read_to_string(path) else {
            continue;
        };
        if let Ok(mut m) = parse_milestone(&raw) {
            m.file_path = Some(rel_from_backlog(backlog_root, path));
            out.push(m);
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// `milestone-*.md` under `milestones/`.
pub fn read_milestones(backlog_root: &Path) -> Result<Vec<BacklogMilestone>, BacklogReadError> {
    Ok(read_milestones_dir(
        backlog_root,
        &backlog_root.join("milestones"),
    ))
}

/// Completed tasks under `completed/`.
pub fn read_completed(backlog_root: &Path) -> Result<Vec<BacklogTask>, BacklogReadError> {
    read_task_files(backlog_root, "completed", "task-")
}

/// Archived milestones under `archive/milestones/`.
pub fn read_archived_milestones(
    backlog_root: &Path,
) -> Result<Vec<BacklogMilestone>, BacklogReadError> {
    Ok(read_milestones_dir(
        backlog_root,
        &backlog_root.join("archive").join("milestones"),
    ))
}

/// Load every supported subtree into a [`BacklogSnapshot`].
pub fn read_all(backlog_root: &Path) -> Result<BacklogSnapshot, BacklogReadError> {
    Ok(BacklogSnapshot {
        config: read_backlog_config(backlog_root)?,
        tasks: read_tasks(backlog_root)?,
        documents: read_documents(backlog_root)?,
        decisions: read_decisions(backlog_root)?,
        drafts: read_drafts(backlog_root)?,
        milestones: read_milestones(backlog_root)?,
        completed: read_completed(backlog_root)?,
        archived_milestones: read_archived_milestones(backlog_root)?,
    })
}

fn insert_doc_at_path(node: &mut DocTreeNode, parts: &[&str], doc: BacklogDocument) {
    if parts.is_empty() {
        node.docs.push(doc);
        return;
    }
    let head = parts[0];
    let tail = &parts[1..];
    if let Some(c) = node.children.iter_mut().find(|c| c.name == head) {
        insert_doc_at_path(c, tail, doc);
    } else {
        let mut n = DocTreeNode {
            name: head.to_string(),
            children: Vec::new(),
            docs: Vec::new(),
        };
        insert_doc_at_path(&mut n, tail, doc);
        node.children.push(n);
    }
}

fn sort_doc_tree(node: &mut DocTreeNode) {
    node.children.sort_by(|a, b| a.name.cmp(&b.name));
    node.docs.sort_by(|a, b| a.id.cmp(&b.id));
    for c in &mut node.children {
        sort_doc_tree(c);
    }
}

/// Build a nested [`DocTree`] from document category paths.
pub fn build_doc_tree(docs: &[BacklogDocument]) -> DocTree {
    let mut root = DocTreeNode::root();
    for d in docs {
        let parts: Vec<&str> = d
            .path
            .as_deref()
            .map(|p| p.split('/').filter(|s| !s.is_empty()).collect())
            .unwrap_or_default();
        insert_doc_at_path(&mut root, &parts, d.clone());
    }
    sort_doc_tree(&mut root);
    DocTree { root }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    fn write(p: &Path, s: &str) {
        let parent = p.parent().unwrap();
        fs::create_dir_all(parent).unwrap();
        fs::File::create(p)
            .unwrap()
            .write_all(s.as_bytes())
            .unwrap();
    }

    #[test]
    fn reader_loads_sample_tree() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("backlog");
        write(
            &root.join("config.yml"),
            "projectName: TestProj\ndateFormat: \"%Y-%m-%d\"\n",
        );
        write(
            &root.join("tasks/task-1 - A.md"),
            "---\nid: TASK-1\ntitle: A\nstatus: Open\ncreated_date: 2026-01-01\n---\n\n",
        );
        write(
            &root.join("docs/guide/doc-1.md"),
            "---\nid: doc-1\ntitle: G\ntype: guide\ncreated_date: 2026-01-01\n---\n\nx\n",
        );
        write(
            &root.join("decisions/decision-1.md"),
            "---\nid: DEC-1\ntitle: D\ndate: 2026-01-01\nstatus: proposed\n---\n\n## Context\n\nc\n\n## Decision\n\nd\n\n## Consequences\n\nx\n",
        );
        write(
            &root.join("drafts/draft-1.md"),
            "---\nid: DRAFT-1\ntitle: Dr\nstatus: Draft\ncreated_date: 2026-01-01\n---\n\n",
        );
        write(
            &root.join("milestones/milestone-1.md"),
            "---\nid: MS-1\ntitle: M\n---\n\n## Description\n\ndesc\n",
        );
        write(
            &root.join("completed/task-9.md"),
            "---\nid: TASK-9\ntitle: Done\nstatus: Done\ncreated_date: 2026-01-01\n---\n\n",
        );
        write(
            &root.join("archive/milestones/milestone-old.md"),
            "---\nid: MS-OLD\ntitle: Old\n---\n\n## Description\n\no\n",
        );
        write(&root.join("tasks/.DS_Store"), "junk");
        write(&root.join("tasks/readme.txt"), "no");

        let snap = read_all(&root).unwrap();
        assert_eq!(snap.config.project_name, "TestProj");
        assert_eq!(snap.tasks.len(), 1);
        assert_eq!(snap.tasks[0].id, "TASK-1");
        assert_eq!(
            snap.tasks[0].file_path.as_deref(),
            Some("tasks/task-1 - A.md")
        );
        assert_eq!(snap.documents.len(), 1);
        assert_eq!(snap.documents[0].path.as_deref(), Some("guide"));
        assert_eq!(
            snap.documents[0].file_path.as_deref(),
            Some("docs/guide/doc-1.md")
        );
        assert_eq!(snap.decisions.len(), 1);
        assert_eq!(
            snap.decisions[0].file_path.as_deref(),
            Some("decisions/decision-1.md")
        );
        assert_eq!(snap.drafts.len(), 1);
        assert_eq!(snap.milestones.len(), 1);
        assert_eq!(
            snap.milestones[0].file_path.as_deref(),
            Some("milestones/milestone-1.md")
        );
        assert_eq!(snap.completed.len(), 1);
        assert_eq!(snap.archived_milestones.len(), 1);
        assert_eq!(
            snap.archived_milestones[0].file_path.as_deref(),
            Some("archive/milestones/milestone-old.md")
        );

        let tree = build_doc_tree(&snap.documents);
        assert_eq!(tree.root.children.len(), 1);
        assert_eq!(tree.root.children[0].name, "guide");
        assert_eq!(tree.root.children[0].docs.len(), 1);
    }

    #[test]
    fn missing_subdirs_empty() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("empty");
        fs::create_dir_all(&root).unwrap();
        assert!(read_tasks(&root).unwrap().is_empty());
        assert!(read_documents(&root).unwrap().is_empty());
        let snap = read_all(&root).unwrap();
        assert!(snap.tasks.is_empty());
    }
}
