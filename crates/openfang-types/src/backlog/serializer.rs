//! Serialize backlog entities to markdown and apply surgical updates (status, sections, AC).

use std::fmt::Write as _;

use super::{
    AcceptanceCriterion, BacklogDecision, BacklogDocument, BacklogMilestone, BacklogTask,
    DecisionStatus, TaskPriority,
};
use regex_lite::Regex;
use serde_yaml::{Mapping, Value as Yaml};
use std::sync::OnceLock;

fn checklist_line_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\s*-\s*\[([ xX])\]\s*(.*)$").expect("checklist regex"))
}

fn normalize_nl(s: &str) -> String {
    s.replace("\r\n", "\n")
}

fn format_yaml_scalar(value: &str) -> String {
    let v = Yaml::String(value.to_string());
    serde_yaml::to_string(&v)
        .expect("yaml scalar")
        .trim_end()
        .to_string()
}

fn yaml_string_seq(items: &[String]) -> Yaml {
    Yaml::Sequence(
        items
            .iter()
            .map(|s| Yaml::String(s.clone()))
            .collect(),
    )
}

fn task_to_mapping(task: &BacklogTask) -> Mapping {
    let mut m = Mapping::new();
    m.insert(Yaml::String("id".into()), Yaml::String(task.id.clone()));
    m.insert(Yaml::String("title".into()), Yaml::String(task.title.clone()));
    m.insert(Yaml::String("status".into()), Yaml::String(task.status.clone()));
    if !task.assignee.is_empty() {
        m.insert(
            Yaml::String("assignee".into()),
            yaml_string_seq(&task.assignee),
        );
    }
    if let Some(ref r) = task.reporter {
        m.insert(Yaml::String("reporter".into()), Yaml::String(r.clone()));
    }
    m.insert(
        Yaml::String("created_date".into()),
        Yaml::String(task.created_date.clone()),
    );
    if let Some(ref u) = task.updated_date {
        m.insert(Yaml::String("updated_date".into()), Yaml::String(u.clone()));
    }
    if !task.labels.is_empty() {
        m.insert(Yaml::String("labels".into()), yaml_string_seq(&task.labels));
    }
    if let Some(ref ms) = task.milestone {
        m.insert(Yaml::String("milestone".into()), Yaml::String(ms.clone()));
    }
    if !task.dependencies.is_empty() {
        m.insert(
            Yaml::String("dependencies".into()),
            yaml_string_seq(&task.dependencies),
        );
    }
    if let Some(ref r) = task.references {
        if !r.is_empty() {
            m.insert(Yaml::String("references".into()), yaml_string_seq(r));
        }
    }
    if let Some(ref d) = task.documentation {
        if !d.is_empty() {
            m.insert(Yaml::String("documentation".into()), yaml_string_seq(d));
        }
    }
    if let Some(ref p) = task.parent_task_id {
        m.insert(Yaml::String("parent_task_id".into()), Yaml::String(p.clone()));
    }
    if let Some(ref st) = task.subtasks {
        if !st.is_empty() {
            m.insert(Yaml::String("subtasks".into()), yaml_string_seq(st));
        }
    }
    if let Some(p) = task.priority {
        let s = match p {
            TaskPriority::High => "high",
            TaskPriority::Medium => "medium",
            TaskPriority::Low => "low",
        };
        m.insert(Yaml::String("priority".into()), Yaml::String(s.into()));
    }
    if let Some(o) = task.ordinal {
        m.insert(
            Yaml::String("ordinal".into()),
            Yaml::Number(serde_yaml::Number::from(o)),
        );
    }
    if let Some(ref b) = task.branch {
        m.insert(Yaml::String("branch".into()), Yaml::String(b.clone()));
    }
    if let Some(ref o) = task.on_status_change {
        m.insert(
            Yaml::String("on_status_change".into()),
            Yaml::String(o.clone()),
        );
    }
    m
}

fn append_section(out: &mut String, key: &str, text: &str) {
    let ku = key.to_ascii_uppercase();
    use std::fmt::Write;
    let _ = writeln!(out, "<!-- SECTION:{ku}:BEGIN -->");
    let _ = write!(out, "{text}");
    let _ = writeln!(out, "\n<!-- SECTION:{ku}:END -->");
}

fn build_task_body(task: &BacklogTask) -> String {
    let mut out = String::new();
    if let Some(ref d) = task.description {
        if !d.is_empty() {
            append_section(&mut out, "DESCRIPTION", d);
            out.push('\n');
        }
    }
    if let Some(ref p) = task.implementation_plan {
        if !p.is_empty() {
            append_section(&mut out, "PLAN", p);
            out.push('\n');
        }
    }
    if let Some(ref n) = task.implementation_notes {
        if !n.is_empty() {
            append_section(&mut out, "NOTES", n);
            out.push('\n');
        }
    }
    if let Some(ref f) = task.final_summary {
        if !f.is_empty() {
            append_section(&mut out, "FINAL_SUMMARY", f);
            out.push('\n');
        }
    }
    if !task.acceptance_criteria.is_empty() {
        out.push_str("<!-- AC:BEGIN -->\n");
        for c in &task.acceptance_criteria {
            let mark = if c.checked { "x" } else { " " };
            let _ = writeln!(out, "- [{mark}] {}", c.text);
        }
        out.push_str("<!-- AC:END -->\n");
    }
    if !task.definition_of_done.is_empty() {
        out.push_str("<!-- DOD:BEGIN -->\n");
        for c in &task.definition_of_done {
            let mark = if c.checked { "x" } else { " " };
            let _ = writeln!(out, "- [{mark}] {}", c.text);
        }
        out.push_str("<!-- DOD:END -->\n");
    }
    out.trim_end().to_string()
}

/// Full task markdown: YAML frontmatter + structured body sections.
pub fn serialize_task(task: &BacklogTask) -> String {
    let map = task_to_mapping(task);
    let yaml = serde_yaml::to_string(&Yaml::Mapping(map)).expect("task frontmatter yaml");
    let yaml = yaml.trim_end();
    let body = build_task_body(task);
    format!("---\n{yaml}\n---\n\n{body}")
}

fn try_split_frontmatter(content: &str) -> Option<(String, String)> {
    let n = normalize_nl(content);
    let trimmed = n.trim_start();
    if !trimmed.starts_with("---") {
        return None;
    }
    let after_open = &trimmed[3..];
    let end = after_open.find("\n---")?;
    let yaml_raw = after_open[..end].to_string();
    let body = after_open[end + 4..].trim_start().to_string();
    Some((yaml_raw, body))
}

fn join_frontmatter(yaml_raw: &str, body: &str) -> String {
    format!("---\n{yaml_raw}\n---\n\n{body}")
}

fn line_yaml_key(line: &str) -> Option<&str> {
    let t = line.trim_start();
    let (k, _) = t.split_once(':')?;
    Some(k.trim_end())
}

fn line_matches_key(line: &str, key: &str) -> bool {
    line_yaml_key(line)
        .map(|k| k.eq_ignore_ascii_case(key))
        .unwrap_or(false)
}

/// Replace or append one top-level `key: …` line with an integer YAML value.
pub fn update_frontmatter_i32(content: &str, key: &str, value: i32) -> String {
    let Some((yaml_raw, body)) = try_split_frontmatter(content) else {
        return content.to_string();
    };
    let formatted = serde_yaml::to_string(&Yaml::Number(serde_yaml::Number::from(value)))
        .expect("yaml number")
        .trim_end()
        .to_string();
    let mut lines: Vec<String> = yaml_raw.lines().map(|l| l.to_string()).collect();
    let mut found = false;
    for line in &mut lines {
        if line_matches_key(line, key) {
            let orig_key = line_yaml_key(line).unwrap_or(key);
            *line = format!("{orig_key}: {formatted}");
            found = true;
            break;
        }
    }
    if !found {
        lines.push(format!("{key}: {formatted}"));
    }
    let new_yaml = lines.join("\n");
    join_frontmatter(&new_yaml, &body)
}

/// Replace or append one top-level `key: …` line in YAML frontmatter; body unchanged.
pub fn update_frontmatter_field(content: &str, key: &str, value: &str) -> String {
    let Some((yaml_raw, body)) = try_split_frontmatter(content) else {
        return content.to_string();
    };
    let formatted = format_yaml_scalar(value);
    let mut lines: Vec<String> = yaml_raw.lines().map(|l| l.to_string()).collect();
    let mut found = false;
    for line in &mut lines {
        if line_matches_key(line, key) {
            let orig_key = line_yaml_key(line).unwrap_or(key);
            *line = format!("{orig_key}: {formatted}");
            found = true;
            break;
        }
    }
    if !found {
        lines.push(format!("{key}: {formatted}"));
    }
    let new_yaml = lines.join("\n");
    join_frontmatter(&new_yaml, &body)
}

/// Set `status` in frontmatter only.
pub fn update_task_status(content: &str, new_status: &str) -> String {
    update_frontmatter_field(content, "status", new_status)
}

/// Replace inner text of a `SECTION:{KEY}` block; if missing, appends a new block at end of body.
pub fn update_section(content: &str, section_key: &str, new_content: &str) -> String {
    let Some((yaml_raw, body)) = try_split_frontmatter(content) else {
        return content.to_string();
    };
    let n = normalize_nl(&body);
    let key_u = section_key.to_ascii_uppercase();
    let begin = format!("<!-- SECTION:{key_u}:BEGIN -->");
    let end = format!("<!-- SECTION:{key_u}:END -->");
    let new_body = if let Some(si) = n.find(&begin) {
        let inner_start = si + begin.len();
        if let Some(rel) = n[inner_start..].find(&end) {
            let ei = inner_start + rel;
            let after_block = ei + end.len();
            format!("{}{}{}", &n[..inner_start], new_content, &n[after_block..])
        } else {
            n
        }
    } else {
        let mut s = n;
        if !s.is_empty() && !s.ends_with('\n') {
            s.push('\n');
        }
        s.push('\n');
        let ku = section_key.to_ascii_uppercase();
        use std::fmt::Write;
        let _ = writeln!(s, "<!-- SECTION:{ku}:BEGIN -->");
        s.push_str(new_content);
        let _ = writeln!(s, "<!-- SECTION:{ku}:END -->");
        s
    };
    join_frontmatter(yaml_raw.trim_end(), &new_body)
}

fn format_ac_block(criteria: &[AcceptanceCriterion]) -> String {
    let mut s = String::from("<!-- AC:BEGIN -->\n");
    for c in criteria {
        let mark = if c.checked { "x" } else { " " };
        let _ = writeln!(s, "- [{mark}] {}", c.text);
    }
    s.push_str("<!-- AC:END -->");
    s
}

/// Replace the `<!-- AC:BEGIN -->` … `<!-- AC:END -->` region.
pub fn update_acceptance_criteria(content: &str, criteria: &[AcceptanceCriterion]) -> String {
    let Some((yaml_raw, body)) = try_split_frontmatter(content) else {
        return content.to_string();
    };
    let n = normalize_nl(&body);
    let start_tag = "<!-- AC:BEGIN -->";
    let end_tag = "<!-- AC:END -->";
    let replacement = format_ac_block(criteria);
    let new_body = if let (Some(si), Some(rel_ei)) = (n.find(start_tag), n.find(end_tag)) {
        if rel_ei < si {
            n
        } else {
            let ei = rel_ei + end_tag.len();
            format!("{}{}{}", &n[..si], replacement, &n[ei..])
        }
    } else {
        let mut s = n;
        if !s.is_empty() && !s.ends_with('\n') {
            s.push('\n');
        }
        s.push('\n');
        s.push_str(&replacement);
        s
    };
    join_frontmatter(yaml_raw.trim_end(), &new_body)
}

/// Toggle the n-th checklist item inside `<!-- AC:BEGIN -->` … (0-based among `- [` lines only).
pub fn toggle_acceptance_criterion(content: &str, index: usize) -> String {
    let Some((yaml_raw, body)) = try_split_frontmatter(content) else {
        return content.to_string();
    };
    let n = normalize_nl(&body);
    let start_tag = "<!-- AC:BEGIN -->";
    let end_tag = "<!-- AC:END -->";
    let (Some(si), Some(ei_tag)) = (n.find(start_tag), n.find(end_tag)) else {
        return content.to_string();
    };
    let inner_start = si + start_tag.len();
    if ei_tag < inner_start {
        return content.to_string();
    }
    let inner = &n[inner_start..ei_tag];
    let before = &n[..inner_start];
    let after = &n[ei_tag..];

    let re = checklist_line_re();
    let mut checklist_count = 0usize;
    let mut lines_out: Vec<String> = Vec::new();
    for line in inner.lines() {
        let mut line = line.to_string();
        if re.captures(&line).is_some() {
            if checklist_count == index {
                if line.contains("[x]") || line.contains("[X]") {
                    line = line.replacen("[x]", "[ ]", 1);
                    line = line.replacen("[X]", "[ ]", 1);
                } else if line.contains("[ ]") {
                    line = line.replacen("[ ]", "[x]", 1);
                }
            }
            checklist_count += 1;
        }
        lines_out.push(line);
    }
    if index >= checklist_count {
        return content.to_string();
    }
    let new_inner = lines_out.join("\n");
    let new_body = format!("{before}{new_inner}{after}");
    join_frontmatter(yaml_raw.trim_end(), new_body.trim_end())
}

fn decision_status_str(s: DecisionStatus) -> &'static str {
    match s {
        DecisionStatus::Proposed => "proposed",
        DecisionStatus::Accepted => "accepted",
        DecisionStatus::Rejected => "rejected",
        DecisionStatus::Superseded => "superseded",
    }
}

/// Document markdown: frontmatter + raw body.
pub fn serialize_document(doc: &BacklogDocument) -> String {
    let mut m = Mapping::new();
    m.insert(Yaml::String("id".into()), Yaml::String(doc.id.clone()));
    m.insert(Yaml::String("title".into()), Yaml::String(doc.title.clone()));
    m.insert(
        Yaml::String("type".into()),
        Yaml::String(doc.doc_type.clone()),
    );
    m.insert(
        Yaml::String("created_date".into()),
        Yaml::String(doc.created_date.clone()),
    );
    if let Some(ref u) = doc.updated_date {
        m.insert(Yaml::String("updated_date".into()), Yaml::String(u.clone()));
    }
    if let Some(ref tags) = doc.tags {
        if !tags.is_empty() {
            m.insert(Yaml::String("tags".into()), yaml_string_seq(tags));
        }
    }
    let yaml = serde_yaml::to_string(&Yaml::Mapping(m)).expect("doc fm");
    format!("---\n{}\n---\n\n{}", yaml.trim_end(), doc.raw_content)
}

/// Decision markdown: frontmatter + `##` sections.
pub fn serialize_decision(decision: &BacklogDecision) -> String {
    let mut m = Mapping::new();
    m.insert(Yaml::String("id".into()), Yaml::String(decision.id.clone()));
    m.insert(Yaml::String("title".into()), Yaml::String(decision.title.clone()));
    m.insert(Yaml::String("date".into()), Yaml::String(decision.date.clone()));
    m.insert(
        Yaml::String("status".into()),
        Yaml::String(decision_status_str(decision.status).into()),
    );
    let yaml = serde_yaml::to_string(&Yaml::Mapping(m)).expect("decision fm");
    use std::fmt::Write;
    let mut body = String::new();
    let _ = writeln!(body, "## Context\n\n{}", decision.context.trim_end());
    let _ = writeln!(body, "\n## Decision\n\n{}", decision.decision.trim_end());
    let _ = writeln!(
        body,
        "\n## Consequences\n\n{}",
        decision.consequences.trim_end()
    );
    if let Some(ref alt) = decision.alternatives {
        let _ = writeln!(body, "\n## Alternatives\n\n{}", alt.trim_end());
    }
    format!("---\n{}\n---\n\n{}", yaml.trim_end(), body.trim_end())
}

/// Milestone markdown: frontmatter + `## Description`.
pub fn serialize_milestone(m: &BacklogMilestone) -> String {
    let mut map = Mapping::new();
    map.insert(Yaml::String("id".into()), Yaml::String(m.id.clone()));
    map.insert(Yaml::String("title".into()), Yaml::String(m.title.clone()));
    let yaml = serde_yaml::to_string(&Yaml::Mapping(map)).expect("milestone fm");
    use std::fmt::Write;
    let mut body = String::new();
    let _ = writeln!(body, "## Description\n\n{}", m.description.trim_end());
    format!("---\n{}\n---\n\n{}", yaml.trim_end(), body.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backlog::parser::{parse_decision, parse_document, parse_milestone, parse_task};

    const SAMPLE: &str = r"---
id: TASK-RT
title: Roundtrip
status: Open
assignee:
  - u1
created_date: 2026-04-06
labels:
  - l1
priority: high
ordinal: 7
---

<!-- SECTION:DESCRIPTION:BEGIN -->
Desc line
<!-- SECTION:DESCRIPTION:END -->

<!-- SECTION:PLAN:BEGIN -->
Plan
<!-- SECTION:PLAN:END -->

<!-- AC:BEGIN -->
- [ ] A
- [x] B
<!-- AC:END -->

<!-- DOD:BEGIN -->
- [ ] D
<!-- DOD:END -->
";

    #[test]
    fn round_trip_task() {
        let t1 = parse_task(SAMPLE).unwrap();
        let md = serialize_task(&t1);
        let t2 = parse_task(&md).unwrap();
        let mut a = t1.clone();
        let mut b = t2.clone();
        a.raw_content.clear();
        b.raw_content.clear();
        a.file_path = None;
        b.file_path = None;
        assert_eq!(a, b);
    }

    #[test]
    fn update_frontmatter_i32_sets_ordinal() {
        let s = "---\nid: T\ntitle: x\n---\n\nbody\n";
        let u = update_frontmatter_i32(s, "ordinal", 3);
        assert!(u.contains("ordinal: 3"));
    }

    #[test]
    fn update_status_preserves_body_bytes() {
        let orig = "---\nid: T\ntitle: x\nstatus: Old\n---\n\nKEEP\n";
        let updated = update_task_status(orig, "New");
        let (_, b0) = try_split_frontmatter(orig).unwrap();
        let (_, b1) = try_split_frontmatter(&updated).unwrap();
        assert_eq!(b0.as_bytes(), b1.as_bytes());
        assert!(updated.contains("status: New"));
    }

    #[test]
    fn update_section_replace() {
        let s = "---\na: b\n---\n\n<!-- SECTION:DESCRIPTION:BEGIN -->\nold\n<!-- SECTION:DESCRIPTION:END -->\n";
        let u = update_section(s, "DESCRIPTION", "new");
        assert!(u.contains("new"));
        assert!(!u.contains("old"));
    }

    #[test]
    fn toggle_ac_line() {
        let s = "---\na: b\n---\n\n<!-- AC:BEGIN -->\n- [ ] one\n- [ ] two\n<!-- AC:END -->\n";
        let u = toggle_acceptance_criterion(s, 0);
        assert!(u.contains("- [x] one"));
        assert!(u.contains("- [ ] two"));
    }

    #[test]
    fn update_ac_regenerates_block() {
        let s = "---\na: b\n---\n\n<!-- AC:BEGIN -->\n- [ ] old\n<!-- AC:END -->\n";
        let crit = [AcceptanceCriterion {
            index: 0,
            text: "new".into(),
            checked: true,
        }];
        let u = update_acceptance_criteria(s, &crit);
        assert!(u.contains("- [x] new"));
        assert!(!u.contains("old"));
    }

    #[test]
    fn document_round_trip() {
        let md = "---\nid: d1\ntitle: T\ntype: guide\ncreated_date: 2026-01-01\n---\n\nBody\n";
        let d = parse_document(md).unwrap();
        let out = serialize_document(&d);
        let d2 = parse_document(&out).unwrap();
        assert_eq!(d, d2);
    }

    #[test]
    fn milestone_round_trip() {
        let md = "---\nid: MS-1\ntitle: M\n---\n\n## Description\n\nD\n";
        let m = parse_milestone(md).unwrap();
        let out = serialize_milestone(&m);
        let m2 = parse_milestone(&out).unwrap();
        assert_eq!(m.id, m2.id);
        assert_eq!(m.title, m2.title);
        assert_eq!(m.description.trim(), m2.description.trim());
    }

    #[test]
    fn decision_round_trip() {
        let md = "---\nid: DEC\ntitle: T\ndate: 2026-01-01\nstatus: proposed\n---\n\n## Context\n\nC\n\n## Decision\n\nD\n\n## Consequences\n\nX\n";
        let d = parse_decision(md).unwrap();
        let out = serialize_decision(&d);
        let d2 = parse_decision(&out).unwrap();
        assert_eq!(d.id, d2.id);
        assert_eq!(d.status, d2.status);
        assert_eq!(d.context.trim(), d2.context.trim());
        assert_eq!(d.decision.trim(), d2.decision.trim());
    }
}
