//! Detect a usable Git work tree (same idea as `git rev-parse --is-inside-work-tree`).

use std::path::Path;
use std::process::{Command, Stdio};

/// True if `path` is a directory and `git` reports it is inside a work tree.
///
/// Returns `false` if `git` is missing, fails to start, or exits non-zero.
pub fn path_is_git_worktree(path: &Path) -> bool {
    if !path.is_dir() {
        return false;
    }
    let output = match Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(path)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("LANG", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
    {
        Ok(o) => o,
        Err(_) => return false,
    };
    if !output.status.success() {
        return false;
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .eq_ignore_ascii_case("true")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::tempdir;

    #[test]
    fn path_is_false_without_git_dir() {
        let dir = tempdir().unwrap();
        let plain = dir.path().join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        assert!(!path_is_git_worktree(&plain));
    }

    #[test]
    fn path_is_true_after_git_init() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        assert!(
            Command::new("git")
                .arg("init")
                .current_dir(&repo)
                .status()
                .map(|s| s.success())
                .unwrap_or(false),
            "git must be available for path_is_git_worktree test"
        );
        assert!(path_is_git_worktree(&repo));
    }
}
