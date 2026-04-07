//! Compile-time embedded integration templates.

/// Returns all bundled integration templates as `(id, TOML content)` pairs.
pub fn bundled_integrations() -> Vec<(&'static str, &'static str)> {
    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_count() {
        assert_eq!(bundled_integrations().len(), 0);
    }

    #[test]
    fn no_duplicate_ids() {
        let integrations = bundled_integrations();
        let mut seen = std::collections::HashSet::new();
        for (id, _) in &integrations {
            assert!(seen.insert(id), "Duplicate integration id: {}", id);
        }
    }
}
