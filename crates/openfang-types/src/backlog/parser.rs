//! Parse backlog.md-style markdown into [`super::BacklogTask`] and related types.

use super::{
    AcceptanceCriterion, BacklogDecision, BacklogDocument, BacklogMilestone, BacklogTask,
    DecisionStatus, TaskPriority,
};
use chrono::{NaiveDate, NaiveDateTime};
use regex_lite::Regex;
use serde_yaml::Value as Yaml;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Failure while parsing backlog markdown.
#[derive(Debug, thiserror::Error)]
pub enum BacklogParseError {
    #[error("missing opening `---` frontmatter delimiter")]
    MissingOpeningDelimiter,
    #[error("missing closing `---` frontmatter delimiter")]
    MissingClosingDelimiter,
    #[error("invalid YAML frontmatter: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("frontmatter must be a YAML mapping at the top level")]
    FrontmatterNotMapping,
    #[error("missing required field: {0}")]
    MissingField(&'static str),
    #[error("invalid decision status in frontmatter")]
    InvalidDecisionStatus,
}

/// Split YAML frontmatter (`---` … `---`) from the body; returns key → value map and remainder.
pub fn parse_frontmatter(content: &str) -> Result<(HashMap<String, Yaml>, String), BacklogParseError> {
    let content = content.replace("\r\n", "\n");
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return Err(BacklogParseError::MissingOpeningDelimiter);
    }
    let after_open = &trimmed[3..];
    let end = after_open
        .find("\n---")
        .ok_or(BacklogParseError::MissingClosingDelimiter)?;
    let yaml_raw = &after_open[..end];
    let body = after_open[end + 4..].trim_start().to_string();
    let root: Yaml = serde_yaml::from_str(yaml_raw)?;
    let mapping = root
        .as_mapping()
        .ok_or(BacklogParseError::FrontmatterNotMapping)?;
    let mut map = HashMap::with_capacity(mapping.len());
    for (k, v) in mapping {
        let Some(ks) = k.as_str() else {
            continue;
        };
        map.insert(ks.to_string(), v.clone());
    }
    Ok((map, body))
}

/// `<!-- SECTION:{KEY}:BEGIN -->` … `<!-- SECTION:{KEY}:END -->` (key matched case-insensitively in marker).
pub fn extract_section(content: &str, key: &str) -> Option<String> {
    let body = content.replace("\r\n", "\n");
    let key_u = key.to_ascii_uppercase();
    let begin = format!("<!-- SECTION:{key_u}:BEGIN -->");
    let end = format!("<!-- SECTION:{key_u}:END -->");
    let start = body.find(&begin)? + begin.len();
    let rest = &body[start..];
    let e = rest.find(&end)?;
    Some(rest[..e].trim().to_string())
}

fn checklist_line_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\s*-\s*\[([ xX])\]\s*(.*)$").expect("checklist regex"))
}

fn parse_checkbox_block(marked_block: &str) -> Vec<AcceptanceCriterion> {
    let re = checklist_line_re();
    let mut out = Vec::new();
    let mut idx = 0usize;
    for line in marked_block.lines() {
        let Some(cap) = re.captures(line) else {
            continue;
        };
        let mark = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let checked = mark.eq_ignore_ascii_case("x");
        let text = cap.get(2).map(|m| m.as_str().trim().to_string()).unwrap_or_default();
        out.push(AcceptanceCriterion {
            index: idx,
            text,
            checked,
        });
        idx += 1;
    }
    out
}

/// Parse `<!-- AC:BEGIN -->` … `<!-- AC:END -->` checklist lines (`- [ ]` / `- [x]`).
pub fn parse_acceptance_criteria(content: &str) -> Vec<AcceptanceCriterion> {
    let body = content.replace("\r\n", "\n");
    let Some(start) = body.find("<!-- AC:BEGIN -->") else {
        return Vec::new();
    };
    let after = &body[start + "<!-- AC:BEGIN -->".len()..];
    let Some(end) = after.find("<!-- AC:END -->") else {
        return Vec::new();
    };
    parse_checkbox_block(&after[..end])
}

/// Parse `<!-- DOD:BEGIN -->` … `<!-- DOD:END -->` checklist lines.
pub fn parse_definition_of_done(content: &str) -> Vec<AcceptanceCriterion> {
    let body = content.replace("\r\n", "\n");
    let Some(start) = body.find("<!-- DOD:BEGIN -->") else {
        return Vec::new();
    };
    let after = &body[start + "<!-- DOD:BEGIN -->".len()..];
    let Some(end) = after.find("<!-- DOD:END -->") else {
        return Vec::new();
    };
    parse_checkbox_block(&after[..end])
}

/// Normalize date strings to `YYYY-MM-DD` when a known format parses; otherwise returns trimmed input.
pub fn normalize_date(input: &str) -> String {
    let s = input.trim();
    if s.is_empty() {
        return String::new();
    }
    let head = s.split_whitespace().next().unwrap_or(s);
    let date_part = head.split('T').next().unwrap_or(head);

    const DATE_ONLY: &[&str] = &["%Y-%m-%d", "%m/%d/%Y", "%d/%m/%Y"];
    for fmt in DATE_ONLY {
        if let Ok(d) = NaiveDate::parse_from_str(date_part, fmt) {
            return d.format("%Y-%m-%d").to_string();
        }
    }

    const WITH_TIME: &[&str] = &[
        "%Y-%m-%d %H:%M",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M:%S%.f",
    ];
    for fmt in WITH_TIME {
        if let Ok(dt) = NaiveDateTime::parse_from_str(s, fmt) {
            return dt.date().format("%Y-%m-%d").to_string();
        }
    }

    if let Ok(d) = NaiveDate::parse_from_str(date_part, "%Y-%m-%d") {
        return d.format("%Y-%m-%d").to_string();
    }

    s.to_string()
}

fn map_get_str(map: &HashMap<String, Yaml>, keys: &[&str]) -> Option<String> {
    for want in keys {
        if let Some(v) = map.get(*want) {
            return yaml_scalar_to_string(v);
        }
    }
    for (k, v) in map {
        let kl = k.to_ascii_lowercase();
        if keys
            .iter()
            .any(|w| kl == w.to_ascii_lowercase().as_str())
        {
            return yaml_scalar_to_string(v);
        }
    }
    None
}

fn yaml_scalar_to_string(v: &Yaml) -> Option<String> {
    match v {
        Yaml::String(s) => Some(s.clone()),
        Yaml::Bool(b) => Some(b.to_string()),
        Yaml::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn yaml_string_list_field(map: &HashMap<String, Yaml>, keys: &[&str]) -> Option<Vec<String>> {
    let v = keys.iter().find_map(|k| map.get(*k))?;
    Some(yaml_to_string_vec(v))
}

fn yaml_to_string_vec(v: &Yaml) -> Vec<String> {
    match v {
        Yaml::Null => Vec::new(),
        Yaml::String(s) => vec![s.clone()],
        Yaml::Sequence(seq) => seq.iter().filter_map(yaml_scalar_to_string).collect(),
        _ => Vec::new(),
    }
}

fn yaml_assignee(v: &Yaml) -> Vec<String> {
    yaml_to_string_vec(v)
}

fn map_get_i32(map: &HashMap<String, Yaml>, keys: &[&str]) -> Option<i32> {
    let v = map_get_str(map, keys)?;
    v.parse().ok()
}

fn map_get_i32_from_number(map: &HashMap<String, Yaml>, keys: &[&str]) -> Option<i32> {
    for want in keys {
        if let Some(Yaml::Number(n)) = map.get(*want) {
            return n.as_i64().and_then(|i| i.try_into().ok());
        }
    }
    for (k, val) in map {
        if !keys
            .iter()
            .any(|w| k.eq_ignore_ascii_case(w))
        {
            continue;
        }
        if let Yaml::Number(n) = val {
            return n.as_i64().and_then(|i| i.try_into().ok());
        }
    }
    map_get_i32(map, keys)
}

fn parse_task_priority(s: &str) -> Option<TaskPriority> {
    match s.trim().to_ascii_lowercase().as_str() {
        "high" => Some(TaskPriority::High),
        "medium" => Some(TaskPriority::Medium),
        "low" => Some(TaskPriority::Low),
        _ => None,
    }
}

fn non_empty(s: String) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

fn extract_h2_section(content: &str, title: &str) -> Option<String> {
    let normalized = content.replace("\r\n", "\n");
    let lines: Vec<&str> = normalized.lines().collect();
    let header = format!("## {title}");
    let mut i = 0usize;
    while i < lines.len() {
        if lines[i].trim() == header {
            let mut out = Vec::new();
            i += 1;
            while i < lines.len() {
                let t = lines[i].trim_start();
                if t.starts_with("## ") {
                    break;
                }
                out.push(lines[i]);
                i += 1;
            }
            return Some(out.join("\n").trim().to_string());
        }
        i += 1;
    }
    None
}

/// Full task: YAML frontmatter + structured body sections.
pub fn parse_task(content: &str) -> Result<BacklogTask, BacklogParseError> {
    let (map, body) = parse_frontmatter(content)?;
    let id = map_get_str(&map, &["id"]).ok_or(BacklogParseError::MissingField("id"))?;
    let title = map_get_str(&map, &["title"]).ok_or(BacklogParseError::MissingField("title"))?;
    let status = map_get_str(&map, &["status"]).unwrap_or_default();
    let assignee = map
        .get("assignee")
        .map(yaml_assignee)
        .unwrap_or_default();
    let reporter = map_get_str(&map, &["reporter"]);
    let created_raw = map_get_str(&map, &["created_date", "createdDate"])
        .ok_or(BacklogParseError::MissingField("created_date"))?;
    let created_date = normalize_date(&created_raw);
    let updated_date = map_get_str(&map, &["updated_date", "updatedDate"])
        .map(|s| normalize_date(&s))
        .filter(|s| !s.is_empty());
    let labels = map
        .get("labels")
        .or_else(|| map.get("label"))
        .map(yaml_to_string_vec)
        .unwrap_or_default();
    let milestone = map_get_str(&map, &["milestone"]);
    let dependencies = map
        .get("dependencies")
        .map(yaml_to_string_vec)
        .unwrap_or_default();
    let references = yaml_string_list_field(&map, &["references"]);
    let documentation = yaml_string_list_field(&map, &["documentation"]);
    let parent_task_id = map_get_str(&map, &["parent_task_id", "parentTaskId"]);
    let subtasks = yaml_string_list_field(&map, &["subtasks"]);
    let priority = map_get_str(&map, &["priority"])
        .as_deref()
        .and_then(parse_task_priority);
    let ordinal = map_get_i32_from_number(&map, &["ordinal"]);
    let branch = map_get_str(&map, &["branch"]);
    let on_status_change = map_get_str(&map, &["on_status_change", "onStatusChange"]);

    let raw_content = body.clone();
    let description = extract_section(&body, "DESCRIPTION").and_then(non_empty);
    let implementation_plan = extract_section(&body, "PLAN").and_then(non_empty);
    let implementation_notes = extract_section(&body, "NOTES").and_then(non_empty);
    let final_summary = extract_section(&body, "FINAL_SUMMARY").and_then(non_empty);
    let acceptance_criteria = parse_acceptance_criteria(&body);
    let definition_of_done = parse_definition_of_done(&body);

    Ok(BacklogTask {
        id,
        title,
        status,
        assignee,
        reporter,
        created_date,
        updated_date,
        labels,
        milestone,
        dependencies,
        references,
        documentation,
        parent_task_id,
        subtasks,
        priority,
        ordinal,
        branch,
        on_status_change,
        raw_content,
        description,
        acceptance_criteria,
        definition_of_done,
        implementation_plan,
        implementation_notes,
        final_summary,
        file_path: None,
    })
}

/// Document (`doc-*.md`): frontmatter + raw markdown body.
pub fn parse_document(content: &str) -> Result<BacklogDocument, BacklogParseError> {
    let (map, body) = parse_frontmatter(content)?;
    let id = map_get_str(&map, &["id"]).ok_or(BacklogParseError::MissingField("id"))?;
    let title = map_get_str(&map, &["title"]).unwrap_or_default();
    let doc_type = map_get_str(&map, &["type", "doc_type", "docType"])
        .unwrap_or_else(|| "other".to_string());
    let created_raw = map_get_str(&map, &["created_date", "createdDate"])
        .ok_or(BacklogParseError::MissingField("created_date"))?;
    let created_date = normalize_date(&created_raw);
    let updated_date = map_get_str(&map, &["updated_date", "updatedDate"])
        .map(|s| normalize_date(&s))
        .filter(|s| !s.is_empty());
    let tags = yaml_string_list_field(&map, &["tags"]);
    Ok(BacklogDocument {
        id,
        title,
        doc_type,
        created_date,
        updated_date,
        tags,
        raw_content: body,
        path: None,
        file_path: None,
    })
}

fn parse_decision_status_yaml(v: &Yaml) -> Result<DecisionStatus, BacklogParseError> {
    let s = v.as_str().ok_or(BacklogParseError::InvalidDecisionStatus)?;
    match s.trim().to_ascii_lowercase().as_str() {
        "proposed" => Ok(DecisionStatus::Proposed),
        "accepted" => Ok(DecisionStatus::Accepted),
        "rejected" => Ok(DecisionStatus::Rejected),
        "superseded" => Ok(DecisionStatus::Superseded),
        _ => Err(BacklogParseError::InvalidDecisionStatus),
    }
}

/// ADR-style decision: frontmatter + `## Context` / `## Decision` / … sections.
pub fn parse_decision(content: &str) -> Result<BacklogDecision, BacklogParseError> {
    let (map, body) = parse_frontmatter(content)?;
    let id = map_get_str(&map, &["id"]).ok_or(BacklogParseError::MissingField("id"))?;
    let title = map_get_str(&map, &["title"]).unwrap_or_default();
    let date_raw = map_get_str(&map, &["date"]).ok_or(BacklogParseError::MissingField("date"))?;
    let date = normalize_date(&date_raw);
    let status = parse_decision_status_yaml(
        map.get("status")
            .ok_or(BacklogParseError::MissingField("status"))?,
    )?;
    let context = extract_h2_section(&body, "Context").unwrap_or_default();
    let decision = extract_h2_section(&body, "Decision").unwrap_or_default();
    let consequences = extract_h2_section(&body, "Consequences").unwrap_or_default();
    let alternatives = extract_h2_section(&body, "Alternatives").and_then(non_empty);
    Ok(BacklogDecision {
        id,
        title,
        date,
        status,
        context,
        decision,
        consequences,
        alternatives,
        raw_content: body,
        file_path: None,
    })
}

/// Milestone file: frontmatter + `## Description` body section.
pub fn parse_milestone(content: &str) -> Result<BacklogMilestone, BacklogParseError> {
    let (map, body) = parse_frontmatter(content)?;
    let id = map_get_str(&map, &["id"]).ok_or(BacklogParseError::MissingField("id"))?;
    let title = map_get_str(&map, &["title"]).unwrap_or_default();
    let description = extract_h2_section(&body, "Description").unwrap_or_default();
    Ok(BacklogMilestone {
        id,
        title,
        description,
        raw_content: body,
        file_path: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_TASK: &str = r"---
id: TASK-99
title: Sample task
status: Open
assignee:
  - alice
reporter: bob
created_date: 2026-04-06 14:30
updated_date: 2026-04-07T10:00:00
labels:
  - rust
dependencies: []
priority: high
ordinal: 42
---

<!-- SECTION:DESCRIPTION:BEGIN -->
Hello **world**
<!-- SECTION:DESCRIPTION:END -->

<!-- SECTION:PLAN:BEGIN -->
Step one
<!-- SECTION:PLAN:END -->

<!-- AC:BEGIN -->
- [ ] First AC
- [x] Done AC
<!-- AC:END -->

<!-- DOD:BEGIN -->
- [ ] DoD line
<!-- DOD:END -->
";

    const SAMPLE_DOC: &str = r"---
id: doc-1
title: My doc
type: guide
created_date: 2026-01-15
updated_date: 2026-02-01
tags: [api, rest]
---

# Body here
";

    const SAMPLE_DECISION: &str = r"---
id: DEC-1
title: Pick Rust
date: 2026-03-01
status: accepted
---

## Context

We need a systems language.

## Decision

Use Rust.

## Consequences

Memory safe.

## Alternatives

Go.
";

    const SAMPLE_MILESTONE: &str = r"---
id: MS-1
title: v1.0
---

## Description

Ship it.
";

    #[test]
    fn parse_frontmatter_returns_map_and_body() {
        let (m, body) = parse_frontmatter(SAMPLE_TASK).unwrap();
        assert_eq!(m.get("id").and_then(|v| v.as_str()), Some("TASK-99"));
        assert!(body.contains("SECTION:DESCRIPTION"));
    }

    #[test]
    fn parse_task_populates_fields() {
        let t = parse_task(SAMPLE_TASK).unwrap();
        assert_eq!(t.id, "TASK-99");
        assert_eq!(t.title, "Sample task");
        assert_eq!(t.status, "Open");
        assert_eq!(t.assignee, vec!["alice"]);
        assert_eq!(t.reporter.as_deref(), Some("bob"));
        assert_eq!(t.created_date, "2026-04-06");
        assert_eq!(t.updated_date.as_deref(), Some("2026-04-07"));
        assert_eq!(t.labels, vec!["rust"]);
        assert_eq!(t.priority, Some(TaskPriority::High));
        assert_eq!(t.ordinal, Some(42));
        assert!(t.description.as_deref().unwrap().contains("Hello"));
        assert!(t.implementation_plan.as_deref().unwrap().contains("Step one"));
        assert_eq!(t.acceptance_criteria.len(), 2);
        assert!(t.acceptance_criteria[1].checked);
        assert_eq!(t.definition_of_done.len(), 1);
    }

    #[test]
    fn parse_document_populates_fields() {
        let d = parse_document(SAMPLE_DOC).unwrap();
        assert_eq!(d.id, "doc-1");
        assert_eq!(d.title, "My doc");
        assert_eq!(d.doc_type, "guide");
        assert_eq!(d.created_date, "2026-01-15");
        assert_eq!(d.updated_date.as_deref(), Some("2026-02-01"));
        assert_eq!(d.tags.as_ref().unwrap(), &vec!["api".to_string(), "rest".to_string()]);
        assert!(d.raw_content.contains("Body here"));
    }

    #[test]
    fn parse_decision_sections() {
        let d = parse_decision(SAMPLE_DECISION).unwrap();
        assert_eq!(d.id, "DEC-1");
        assert_eq!(d.status, DecisionStatus::Accepted);
        assert!(d.context.contains("systems language"));
        assert!(d.decision.contains("Rust"));
        assert!(d.consequences.contains("Memory"));
        assert!(d.alternatives.as_deref().unwrap().contains("Go"));
    }

    #[test]
    fn parse_milestone_description() {
        let m = parse_milestone(SAMPLE_MILESTONE).unwrap();
        assert_eq!(m.id, "MS-1");
        assert_eq!(m.title, "v1.0");
        assert_eq!(m.description, "Ship it.");
    }

    #[test]
    fn missing_optional_task_fields_ok() {
        let raw = "---\nid: TASK-X\ntitle: T\ncreated_date: 2026-01-01\n---\n\n";
        let t = parse_task(raw).unwrap();
        assert_eq!(t.id, "TASK-X");
        assert!(t.assignee.is_empty());
        assert!(t.reporter.is_none());
        assert!(t.acceptance_criteria.is_empty());
    }

    #[test]
    fn malformed_yaml_errors() {
        let raw = "---\nid: [\n---\n";
        assert!(parse_frontmatter(raw).is_err());
    }

    #[test]
    fn normalize_date_variants() {
        assert_eq!(normalize_date("2026-04-06"), "2026-04-06");
        assert_eq!(normalize_date("2026-04-06 15:00"), "2026-04-06");
        assert_eq!(normalize_date("2026-04-06T12:30:00"), "2026-04-06");
        assert_eq!(normalize_date("04/15/2026"), "2026-04-15");
    }
}
