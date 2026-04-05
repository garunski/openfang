//! Resolve `repo:<spoke>` backlog labels to configured spoke workspace paths.

use std::path::PathBuf;

/// Extract the spoke name from exactly one `repo:<spoke>` label (case-insensitive `repo:` prefix).
pub fn extract_repo_spoke_name_from_labels<'a>(
    labels: impl IntoIterator<Item = &'a str>,
) -> Result<String, String> {
    let mut repo_labels: Vec<String> = Vec::new();
    for raw in labels {
        let s = raw.trim();
        if s.len() >= 5 {
            let (head, tail) = s.split_at(5);
            if head.eq_ignore_ascii_case("repo:") {
                let name = tail.trim();
                if name.is_empty() {
                    return Err("repo: label has empty spoke name".to_string());
                }
                repo_labels.push(name.to_string());
            }
        }
    }
    match repo_labels.len() {
        0 => Err(
            "Task must have exactly one repo:<spoke> label; none found.".to_string(),
        ),
        1 => Ok(repo_labels.pop().expect("one repo label")),
        n => Err(format!(
            "Task must have exactly one repo:<spoke> label; found {n}: {:?}",
            repo_labels
        )),
    }
}

/// Find the canonical spoke root whose final path component matches `spoke_name` (ASCII case-insensitive).
pub fn resolve_spoke_root_from_name(
    spoke_roots: &[PathBuf],
    spoke_name: &str,
) -> Result<PathBuf, String> {
    let key = spoke_name.trim();
    if key.is_empty() {
        return Err("spoke name is empty".to_string());
    }
    if spoke_roots.is_empty() {
        return Err(
            "No automation spoke roots configured. Add paths to [automation].spoke_roots in config.toml."
                .to_string(),
        );
    }
    let mut matches: Vec<PathBuf> = Vec::new();
    for root in spoke_roots {
        if !root.is_absolute() {
            continue;
        }
        let Ok(canon) = std::fs::canonicalize(root) else {
            continue;
        };
        let Some(base) = canon.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if base.eq_ignore_ascii_case(key) {
            matches.push(canon);
        }
    }
    match matches.len() {
        0 => Err(format!(
            "Unknown spoke '{key}': no [automation].spoke_roots directory basename matches this name"
        )),
        1 => Ok(matches.pop().expect("one match")),
        _ => Err(format!(
            "Ambiguous spoke '{key}': multiple spoke_roots share this directory basename"
        )),
    }
}

/// Full routing: labels → canonical spoke root under `spoke_roots`.
pub fn resolve_spoke_workspace_from_task_labels(
    spoke_roots: &[PathBuf],
    labels: &[String],
) -> Result<PathBuf, String> {
    let name = extract_repo_spoke_name_from_labels(labels.iter().map(String::as_str))?;
    resolve_spoke_root_from_name(spoke_roots, &name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn extract_single_label() {
        let labels = ["rust".to_string(), "repo:my-spoke".to_string()];
        assert_eq!(
            extract_repo_spoke_name_from_labels(labels.iter().map(String::as_str)).unwrap(),
            "my-spoke"
        );
    }

    #[test]
    fn extract_repo_prefix_case_insensitive() {
        assert_eq!(
            extract_repo_spoke_name_from_labels(["RePo:Foo"].into_iter()).unwrap(),
            "Foo"
        );
    }

    #[test]
    fn extract_missing_label() {
        let err = extract_repo_spoke_name_from_labels(["a", "b"]).unwrap_err();
        assert!(err.contains("none found"), "{err}");
    }

    #[test]
    fn extract_duplicate_repo_labels() {
        let labels = ["repo:a".to_string(), "repo:b".to_string()];
        let err = extract_repo_spoke_name_from_labels(labels.iter().map(String::as_str)).unwrap_err();
        assert!(err.contains("exactly one"), "{err}");
        assert!(err.contains("2"), "{err}");
    }

    #[test]
    fn extract_empty_spoke_name() {
        let err = extract_repo_spoke_name_from_labels(std::iter::once("repo:")).unwrap_err();
        assert!(err.contains("empty"), "{err}");
    }

    #[test]
    fn resolve_known_spoke() {
        let tmp = tempfile::tempdir().unwrap();
        let spoke = tmp.path().join("myspoke");
        fs::create_dir_all(&spoke).unwrap();
        let canon = spoke.canonicalize().unwrap();
        let roots = vec![canon.clone()];
        let got = resolve_spoke_root_from_name(&roots, "myspoke").unwrap();
        assert_eq!(got, canon);
    }

    #[test]
    fn resolve_unknown_spoke() {
        let tmp = tempfile::tempdir().unwrap();
        let spoke = tmp.path().join("foo");
        fs::create_dir_all(&spoke).unwrap();
        let roots = vec![spoke.canonicalize().unwrap()];
        let err = resolve_spoke_root_from_name(&roots, "bar").unwrap_err();
        assert!(err.contains("Unknown spoke"), "{err}");
    }

    #[test]
    fn resolve_spoke_case_insensitive_basename() {
        let tmp = tempfile::tempdir().unwrap();
        let spoke = tmp.path().join("OpenFang");
        fs::create_dir_all(&spoke).unwrap();
        let roots = vec![spoke.canonicalize().unwrap()];
        let got = resolve_spoke_root_from_name(&roots, "openfang").unwrap();
        assert_eq!(got, roots[0].canonicalize().unwrap());
    }

    #[test]
    fn resolve_full_pipeline() {
        let tmp = tempfile::tempdir().unwrap();
        let spoke = tmp.path().join("wheel");
        fs::create_dir_all(&spoke).unwrap();
        let roots = vec![spoke.canonicalize().unwrap()];
        let labels = vec!["repo:wheel".to_string()];
        let got = resolve_spoke_workspace_from_task_labels(&roots, &labels).unwrap();
        assert_eq!(got, roots[0].canonicalize().unwrap());
        let v = crate::config::validate_spoke_root_allowlisted(&roots, got.as_path()).unwrap();
        assert_eq!(v, got);
    }

    #[test]
    fn resolve_ambiguous_two_same_basename() {
        let t1 = tempfile::tempdir().unwrap();
        let t2 = tempfile::tempdir().unwrap();
        let a = t1.path().join("dup");
        let b = t2.path().join("dup");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        let roots = vec![a.canonicalize().unwrap(), b.canonicalize().unwrap()];
        let err = resolve_spoke_root_from_name(&roots, "dup").unwrap_err();
        assert!(err.contains("Ambiguous"), "{err}");
    }
}
