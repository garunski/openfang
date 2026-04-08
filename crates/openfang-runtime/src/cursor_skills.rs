//! Bundled Cursor skill files deployed into spoke workspaces before `cursor agent` runs.

use std::fs;
use std::path::Path;

const CURSOR_SKILLS: &[(&str, &str)] = &[
    (
        "explore",
        include_str!("../bundled/cursor-skills/explore/SKILL.md"),
    ),
    (
        "review",
        include_str!("../bundled/cursor-skills/review/SKILL.md"),
    ),
    (
        "test-write",
        include_str!("../bundled/cursor-skills/test-write/SKILL.md"),
    ),
    (
        "implement",
        include_str!("../bundled/cursor-skills/implement/SKILL.md"),
    ),
];

/// Writes bundled `.cursor/skills/<name>/SKILL.md` under `spoke_root`.
/// Skips a file when existing content already matches (idempotent).
pub fn deploy_cursor_skills(spoke_root: &Path) -> Result<usize, String> {
    let base = spoke_root.join(".cursor").join("skills");
    let mut written = 0;
    for (name, content) in CURSOR_SKILLS {
        let dir = base.join(name);
        fs::create_dir_all(&dir).map_err(|e| format!("create_dir_all {}: {e}", dir.display()))?;
        let path = dir.join("SKILL.md");
        if path.is_file() {
            if let Ok(existing) = fs::read_to_string(&path) {
                if existing == *content {
                    continue;
                }
            }
        }
        fs::write(&path, content.as_bytes())
            .map_err(|e| format!("write {}: {e}", path.display()))?;
        written += 1;
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deploy_cursor_skills_writes_and_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let n = deploy_cursor_skills(root).expect("deploy");
        assert_eq!(n, 4, "expected four new files");
        let p = root.join(".cursor/skills/implement/SKILL.md");
        assert!(p.is_file());
        let body = std::fs::read_to_string(&p).unwrap();
        assert!(body.contains("name: implement"));
        let n2 = deploy_cursor_skills(root).expect("redeploy");
        assert_eq!(n2, 0);
    }

    #[test]
    fn deploy_cursor_skills_overwrites_when_content_differs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        deploy_cursor_skills(root).unwrap();
        let p = root.join(".cursor/skills/explore/SKILL.md");
        std::fs::write(&p, "stale").unwrap();
        let n = deploy_cursor_skills(root).unwrap();
        assert_eq!(n, 1);
        assert!(std::fs::read_to_string(&p)
            .unwrap()
            .contains("name: explore"));
    }
}
