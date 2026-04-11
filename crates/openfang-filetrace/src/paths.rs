//! Canonical paths under `<home>/logs/trace/` — single source of truth for layout + API resolution.

use std::path::{Path, PathBuf};

/// Subdirectory name under `<home>/logs/`.
pub const TRACE_SUBDIR: &str = "trace";

#[inline]
pub fn logs_dir(home: &Path) -> PathBuf {
    home.join("logs")
}

/// `<home>/logs/trace`
#[inline]
pub fn trace_root(home: &Path) -> PathBuf {
    logs_dir(home).join(TRACE_SUBDIR)
}

#[inline]
pub fn trace_archive_dir(home: &Path) -> PathBuf {
    trace_root(home).join("archive")
}

#[inline]
pub fn trace_projects_dir(home: &Path) -> PathBuf {
    trace_root(home).join("projects")
}

#[inline]
pub fn trace_agents_dir(home: &Path) -> PathBuf {
    trace_root(home).join("agents")
}

#[inline]
pub fn trace_system_log_path(home: &Path) -> PathBuf {
    trace_root(home).join("system.log")
}

#[inline]
pub fn trace_readme_path(home: &Path) -> PathBuf {
    trace_root(home).join("README.txt")
}
