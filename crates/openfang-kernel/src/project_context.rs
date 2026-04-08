//! Per-project `context.json` under `<home_dir>/projects/<project_id>/`.

use openfang_types::project::{ProjectContext, ProjectId};
use std::path::{Path, PathBuf};

/// `<home>/projects/<uuid>/context.json`
pub fn project_context_path(home_dir: &Path, project_id: ProjectId) -> PathBuf {
    home_dir
        .join("projects")
        .join(project_id.to_string())
        .join("context.json")
}

pub fn load_project_context_file(
    path: &Path,
    max_failures: usize,
    decision_max_age_days: u32,
) -> ProjectContext {
    let mut ctx = if path.exists() {
        match std::fs::read_to_string(path) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(_) => ProjectContext::default(),
        }
    } else {
        ProjectContext::default()
    };
    ctx.prune(max_failures, decision_max_age_days);
    ctx
}

pub fn save_project_context_file(path: &Path, ctx: &ProjectContext) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let data = serde_json::to_string_pretty(ctx).map_err(|e| e.to_string())?;
    std::fs::write(path, data).map_err(|e| e.to_string())
}
