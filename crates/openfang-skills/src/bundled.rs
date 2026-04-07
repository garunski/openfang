//! Bundled skills — compile-time embedded SKILL.md files.
//!
//! User-installed skills with the same name override bundled ones.

use crate::openclaw_compat::convert_skillmd_str;
use crate::SkillManifest;

/// Return all bundled (name, raw SKILL.md content) pairs.
pub fn bundled_skills() -> Vec<(&'static str, &'static str)> {
    vec![]
}

/// Parse a bundled SKILL.md into a `SkillManifest`.
pub fn parse_bundled(name: &str, content: &str) -> Result<SkillManifest, crate::SkillError> {
    let converted = convert_skillmd_str(name, content)?;
    Ok(converted.manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bundled_skills_count() {
        let skills = bundled_skills();
        assert_eq!(skills.len(), 0, "Expected 0 bundled skills");
    }

    #[test]
    fn test_user_skill_overrides_bundled() {
        use crate::registry::SkillRegistry;
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let mut registry = SkillRegistry::new(dir.path().to_path_buf());

        let bundled_count = registry.load_bundled();
        assert_eq!(bundled_count, 0);
    }
}
