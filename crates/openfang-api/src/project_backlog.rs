//! Scan `backlog/tasks` and `backlog/docs` for backlog.md-style markdown.

use serde_json::{json, Value as JsonValue};
use std::fs;
use std::path::Path;

/// Split YAML frontmatter (`---` … `---`) from body. Normalizes `\r\n`.
pub fn split_frontmatter(content: &str) -> Result<(serde_yaml::Value, String), String> {
    let content = content.replace("\r\n", "\n");
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return Err("missing opening ---".into());
    }
    let after_open = &trimmed[3..];
    let end = after_open
        .find("\n---")
        .ok_or_else(|| "missing closing ---".to_string())?;
    let yaml_raw = &after_open[..end];
    let body = after_open[end + 4..].trim_start().to_string();
    let fm: serde_yaml::Value =
        serde_yaml::from_str(yaml_raw).map_err(|e| format!("YAML: {e}"))?;
    Ok((fm, body))
}

fn yaml_to_json(v: &serde_yaml::Value) -> JsonValue {
    match v {
        serde_yaml::Value::Null => JsonValue::Null,
        serde_yaml::Value::Bool(b) => JsonValue::Bool(*b),
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                JsonValue::Number(i.into())
            } else if let Some(u) = n.as_u64() {
                JsonValue::Number(u.into())
            } else {
                n.as_f64()
                    .and_then(serde_json::Number::from_f64)
                    .map(JsonValue::Number)
                    .unwrap_or(JsonValue::Null)
            }
        }
        serde_yaml::Value::String(s) => JsonValue::String(s.clone()),
        serde_yaml::Value::Sequence(seq) => {
            JsonValue::Array(seq.iter().map(yaml_to_json).collect())
        }
        serde_yaml::Value::Mapping(m) => JsonValue::Object(
            m.iter()
                .filter_map(|(k, v)| {
                    k.as_str().map(|key| (key.to_string(), yaml_to_json(v)))
                })
                .collect(),
        ),
        serde_yaml::Value::Tagged(t) => yaml_to_json(&t.value),
    }
}

fn mapping_str_ci(m: &serde_yaml::Mapping, keys: &[&str]) -> Option<String> {
    for (k, v) in m {
        let ks = k.as_str()?;
        if keys.iter().any(|want| ks.eq_ignore_ascii_case(want)) {
            return v.as_str().map(String::from);
        }
    }
    None
}

fn mapping_labels(m: &serde_yaml::Mapping) -> Vec<String> {
    let Some(v) = m
        .get(serde_yaml::Value::String("labels".into()))
        .or_else(|| m.get(serde_yaml::Value::String("label".into())))
    else {
        return Vec::new();
    };
    match v {
        serde_yaml::Value::Sequence(seq) => seq
            .iter()
            .filter_map(|x| x.as_str().map(String::from))
            .collect(),
        serde_yaml::Value::String(s) => vec![s.clone()],
        _ => Vec::new(),
    }
}

/// Extract `<!-- SECTION:{key}:BEGIN -->` … `<!-- SECTION:{key}:END -->` (key is uppercased in files).
pub fn extract_section(body: &str, key: &str) -> Option<String> {
    let body = body.replace("\r\n", "\n");
    let begin = format!("<!-- SECTION:{key}:BEGIN -->");
    let end = format!("<!-- SECTION:{key}:END -->");
    let start = body.find(&begin)? + begin.len();
    let rest = &body[start..];
    let e = rest.find(&end)?;
    Some(rest[..e].trim().to_string())
}

/// Parse `<!-- AC:BEGIN -->` … `<!-- AC:END -->` into checkbox items.
pub fn parse_acceptance_criteria(body: &str) -> Vec<JsonValue> {
    let body = body.replace("\r\n", "\n");
    let Some(start) = body.find("<!-- AC:BEGIN -->") else {
        return Vec::new();
    };
    let after = &body[start + "<!-- AC:BEGIN -->".len()..];
    let Some(end) = after.find("<!-- AC:END -->") else {
        return Vec::new();
    };
    let block = &after[..end];
    let mut out = Vec::new();
    for (idx, line) in block.lines().enumerate() {
        let t = line.trim();
        if !t.starts_with("- [") {
            continue;
        }
        let after_dash = &t[3..];
        let Some(rb) = after_dash.find(']') else {
            continue;
        };
        let inside = &after_dash[..rb];
        let checked = inside.contains('x') || inside.contains('X');
        let text = after_dash[rb + 1..].trim().to_string();
        out.push(json!({
            "index": idx,
            "checked": checked,
            "text": text,
        }));
    }
    out
}

fn norm_task_id(s: &str) -> String {
    let t = s.trim();
    let u = t.to_ascii_uppercase();
    if u.starts_with("TASK-") {
        u
    } else {
        format!("TASK-{u}")
    }
}

fn task_id_matches(param: &str, fm_id: Option<&str>, file_stem: &str) -> bool {
    let want = norm_task_id(param);
    if let Some(id) = fm_id {
        if norm_task_id(id) == want {
            return true;
        }
    }
    // Filename: `task-19 - Title.md` → stem `task-19 - Title`
    let lower = file_stem.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("task-") {
        let num = rest
            .split(" - ")
            .next()
            .unwrap_or(rest)
            .trim();
        if norm_task_id(num) == want {
            return true;
        }
    }
    false
}

fn is_task_markdown(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.starts_with("task-") && n.ends_with(".md")
}

fn is_doc_markdown(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.starts_with("doc-") && n.ends_with(".md")
}

/// Summary row for `GET .../tasks`.
pub fn task_summary_from_file(path: &Path) -> Option<JsonValue> {
    let raw = fs::read_to_string(path).ok()?;
    let (fm, _) = split_frontmatter(&raw).ok()?;
    let m = fm.as_mapping()?;
    let id = mapping_str_ci(m, &["id"])?.to_string();
    let title = mapping_str_ci(m, &["title"]).unwrap_or_default();
    let status = mapping_str_ci(m, &["status"]).unwrap_or_default();
    let priority = mapping_str_ci(m, &["priority"]).unwrap_or_default();
    let labels = mapping_labels(m);
    let created_date = mapping_str_ci(m, &["created_date", "createdDate"]).unwrap_or_default();
    Some(json!({
        "id": id,
        "title": title,
        "status": status,
        "priority": priority,
        "labels": labels,
        "created_date": created_date,
    }))
}

pub fn list_tasks(
    backlog_root: &Path,
    status_filter: Option<&str>,
) -> Result<Vec<JsonValue>, std::io::Error> {
    let tasks_dir = backlog_root.join("tasks");
    if !tasks_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for ent in fs::read_dir(&tasks_dir)? {
        let ent = ent?;
        let name = ent.file_name().to_string_lossy().into_owned();
        if !is_task_markdown(&name) {
            continue;
        }
        let path = ent.path();
        let Some(row) = task_summary_from_file(&path) else {
            tracing::warn!(path = %path.display(), "skip task file: bad frontmatter");
            continue;
        };
        if let Some(f) = status_filter {
            let st = row["status"].as_str().unwrap_or("");
            if !st.eq_ignore_ascii_case(f.trim()) {
                continue;
            }
        }
        out.push(row);
    }
    out.sort_by(|a, b| {
        let ia = a["id"].as_str().unwrap_or("");
        let ib = b["id"].as_str().unwrap_or("");
        ia.cmp(ib)
    });
    Ok(out)
}

pub fn get_task_detail(backlog_root: &Path, task_id: &str) -> Result<Option<JsonValue>, std::io::Error> {
    let tasks_dir = backlog_root.join("tasks");
    if !tasks_dir.is_dir() {
        return Ok(None);
    }
    for ent in fs::read_dir(&tasks_dir)? {
        let ent = ent?;
        let name = ent.file_name().to_string_lossy().into_owned();
        if !is_task_markdown(&name) {
            continue;
        }
        let path = ent.path();
        let raw = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let Ok((fm, body)) = split_frontmatter(&raw) else {
            continue;
        };
        let fm_id = fm
            .as_mapping()
            .and_then(|m| mapping_str_ci(m, &["id"]));
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        if !task_id_matches(task_id, fm_id.as_deref(), stem) {
            continue;
        }
        let fm_json = yaml_to_json(&fm);
        let description = extract_section(&body, "DESCRIPTION").unwrap_or_default();
        let ac = parse_acceptance_criteria(&body);
        return Ok(Some(json!({
            "frontmatter": fm_json,
            "description": description,
            "acceptance_criteria": ac,
        })));
    }
    Ok(None)
}

#[derive(Debug, serde::Serialize)]
pub struct DocEntry {
    pub id: String,
    pub title: String,
    #[serde(rename = "type")]
    pub doc_type: String,
    pub path: String,
}

fn walk_docs_dir(
    dir: &Path,
    docs_root: &Path,
    out: &mut Vec<DocEntry>,
) -> Result<(), std::io::Error> {
    for ent in fs::read_dir(dir)? {
        let ent = ent?;
        let p = ent.path();
        let meta = ent.metadata()?;
        if meta.is_dir() {
            walk_docs_dir(&p, docs_root, out)?;
            continue;
        }
        let Some(name) = p.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !is_doc_markdown(name) {
            continue;
        }
        let raw = match fs::read_to_string(&p) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let Ok((fm, _)) = split_frontmatter(&raw) else {
            tracing::warn!(path = %p.display(), "skip doc: bad frontmatter");
            continue;
        };
        let m = match fm.as_mapping() {
            Some(m) => m,
            None => continue,
        };
        let id = mapping_str_ci(m, &["id"]).unwrap_or_else(|| name.trim_end_matches(".md").to_string());
        let title = mapping_str_ci(m, &["title"]).unwrap_or_default();
        let doc_type = mapping_str_ci(m, &["type", "doc_type", "docType"]).unwrap_or_else(|| "doc".to_string());
        let rel = p
            .strip_prefix(docs_root)
            .unwrap_or(&p)
            .to_string_lossy()
            .replace('\\', "/");
        out.push(DocEntry {
            id,
            title,
            doc_type,
            path: rel,
        });
    }
    Ok(())
}

pub fn list_docs(backlog_root: &Path) -> Result<Vec<DocEntry>, std::io::Error> {
    let docs_root = backlog_root.join("docs");
    if !docs_root.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    walk_docs_dir(&docs_root, &docs_root, &mut out)?;
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn split_frontmatter_roundtrip() {
        let s = "---\nid: TASK-1\ntitle: Hi\n---\n\nbody\n";
        let (fm, body) = split_frontmatter(s).unwrap();
        assert_eq!(fm["id"].as_str(), Some("TASK-1"));
        assert!(body.contains("body"));
    }

    #[test]
    fn extract_description_and_ac() {
        let body = r"<!-- SECTION:DESCRIPTION:BEGIN -->
Hello
<!-- SECTION:DESCRIPTION:END -->
<!-- AC:BEGIN -->
- [ ] #1 one
- [x] #2 two
<!-- AC:END -->
";
        assert_eq!(
            extract_section(body, "DESCRIPTION").as_deref(),
            Some("Hello")
        );
        let ac = parse_acceptance_criteria(body);
        assert_eq!(ac.len(), 2);
        assert_eq!(ac[0]["checked"], false);
        assert_eq!(ac[1]["checked"], true);
    }

    #[test]
    fn list_and_get_task() {
        let dir = tempdir().unwrap();
        let backlog = dir.path().join("backlog");
        let tasks = backlog.join("tasks");
        fs::create_dir_all(&tasks).unwrap();
        fs::write(
            tasks.join("task-99 - X.md"),
            "---\nid: TASK-99\ntitle: T\nstatus: Open\npriority: high\nlabels:\n  - rust\ncreated_date: '2026-01-01'\n---\n",
        )
        .unwrap();

        let list = list_tasks(&backlog, None).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0]["id"], "TASK-99");

        let list_f = list_tasks(&backlog, Some("open")).unwrap();
        assert_eq!(list_f.len(), 1);

        let list_x = list_tasks(&backlog, Some("closed")).unwrap();
        assert_eq!(list_x.len(), 0);

        let d = get_task_detail(&backlog, "99").unwrap().unwrap();
        assert!(d.get("frontmatter").is_some());
    }
}
