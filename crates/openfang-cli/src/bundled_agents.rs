//! Compile-time embedded agent templates.

/// Returns all bundled agent templates as `(name, toml_content)` pairs.
pub fn bundled_agents() -> Vec<(&'static str, &'static str)> {
    vec![]
}

/// Install bundled agent templates to `~/.openfang/agents/`.
/// Skips any template that already exists on disk (user customization preserved).
pub fn install_bundled_agents(agents_dir: &std::path::Path) {
    for (name, content) in bundled_agents() {
        let dest_dir = agents_dir.join(name);
        let dest_file = dest_dir.join("agent.toml");
        if dest_file.exists() {
            continue;
        }
        if std::fs::create_dir_all(&dest_dir).is_ok() {
            let _ = std::fs::write(&dest_file, content);
        }
    }
}
