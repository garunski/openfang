//! Run `git` in a project spoke directory (registered spoke roots only).

use openfang_types::project::Project;
use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug)]
pub enum GitWorkspaceError {
    SpokeNotFound,
    NotADirectory,
    GitSpawn(String),
    NotAGitRepo,
    InvalidPathArg,
    GitFailed {
        code: Option<i32>,
        stderr: String,
        stdout: String,
    },
}

impl fmt::Display for GitWorkspaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GitWorkspaceError::SpokeNotFound => write!(f, "unknown spoke"),
            GitWorkspaceError::NotADirectory => write!(f, "spoke path is not a directory"),
            GitWorkspaceError::GitSpawn(s) => write!(f, "git executable failed: {s}"),
            GitWorkspaceError::NotAGitRepo => write!(f, "not a git repository"),
            GitWorkspaceError::InvalidPathArg => write!(f, "invalid path argument"),
            GitWorkspaceError::GitFailed { stderr, .. } => write!(f, "git: {stderr}"),
        }
    }
}

impl std::error::Error for GitWorkspaceError {}

/// Resolve registered spoke `name` to an absolute directory path.
pub fn resolve_spoke_root(project: &Project, spoke_name: &str) -> Result<PathBuf, GitWorkspaceError> {
    let name = spoke_name.trim();
    if name.is_empty() {
        return Err(GitWorkspaceError::SpokeNotFound);
    }
    if !project.spokes.iter().any(|s| s.name == name) {
        return Err(GitWorkspaceError::SpokeNotFound);
    }
    let p = project
        .resolve_spoke(name)
        .ok_or(GitWorkspaceError::SpokeNotFound)?;
    if !p.is_dir() {
        return Err(GitWorkspaceError::NotADirectory);
    }
    Ok(p.canonicalize().unwrap_or(p))
}

fn run_git(cwd: &Path, args: &[&str]) -> Result<std::process::Output, GitWorkspaceError> {
    Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("LANG", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| GitWorkspaceError::GitSpawn(e.to_string()))
}

fn git_ok(output: std::process::Output) -> Result<String, GitWorkspaceError> {
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    Err(GitWorkspaceError::GitFailed {
        code: output.status.code(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
    })
}

/// True if `cwd` is inside a git work tree.
pub fn is_git_repo(cwd: &Path) -> Result<bool, GitWorkspaceError> {
    let out = run_git(cwd, &["rev-parse", "--is-inside-work-tree"])?;
    if !out.status.success() {
        return Ok(false);
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_ascii_lowercase();
    Ok(s == "true")
}

pub fn ensure_git_repo(cwd: &Path) -> Result<(), GitWorkspaceError> {
    if is_git_repo(cwd)? {
        Ok(())
    } else {
        Err(GitWorkspaceError::NotAGitRepo)
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct GitBranchInfo {
    pub branch: Option<String>,
    pub detached: bool,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct GitFileEntry {
    pub path: String,
    /// Two-char porcelain index/worktree codes when present (e.g. `M `, `MM`, `??`).
    pub xy: String,
    pub staged: bool,
    pub unstaged: bool,
    pub untracked: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct GitStatusResponse {
    pub branch: GitBranchInfo,
    pub dirty: bool,
    pub files: Vec<GitFileEntry>,
}

/// Undo C-style quoting used in `git status --porcelain` for non-trivial paths.
fn normalize_porcelain_path(path: String) -> String {
    let t = path.trim();
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        return unquote_git_c_style(&t[1..t.len() - 1]);
    }
    t.to_string()
}

fn unquote_git_c_style(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('t') => out.push('\t'),
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some(o) => {
                    out.push('\\');
                    out.push(o);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Parse `git status --porcelain=v1 -b` output.
pub fn parse_status_porcelain(raw: &str) -> GitStatusResponse {
    let mut branch = GitBranchInfo {
        branch: None,
        detached: false,
        upstream: None,
        ahead: 0,
        behind: 0,
    };
    let mut files = Vec::new();
    for line in raw.lines() {
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("## ") {
            parse_branch_line(rest, &mut branch);
            continue;
        }
        if line.len() < 3 {
            continue;
        }
        // rename: XY old -> new (simplified: skip parsing old name, take last token)
        let xy = &line[0..2];
        let path_part = line[2..].trim_start();
        let path_raw = if path_part.contains(" -> ") {
            path_part
                .split(" -> ")
                .last()
                .unwrap_or(path_part)
                .trim()
                .to_string()
        } else {
            path_part.trim().to_string()
        };
        let path = normalize_porcelain_path(path_raw);
        if path.is_empty() {
            continue;
        }
        let c0 = xy.as_bytes().first().copied().unwrap_or(b' ');
        let c1 = xy.as_bytes().get(1).copied().unwrap_or(b' ');
        let untracked = xy == "??";
        let staged = if untracked {
            false
        } else {
            c0 != b' ' && c0 != b'?'
        };
        let unstaged = if untracked {
            true
        } else {
            c1 != b' ' && c1 != b'?'
        };
        files.push(GitFileEntry {
            path,
            xy: xy.to_string(),
            staged,
            unstaged,
            untracked,
        });
    }
    let dirty = !files.is_empty();
    GitStatusResponse { branch, dirty, files }
}

fn parse_branch_line(rest: &str, branch: &mut GitBranchInfo) {
    // Examples:
    // main...origin/main [ahead 2]
    // main...origin/main [ahead 1, behind 3]
    // HEAD (no branch)
    // main (no upstream yet - no ...)
    let head = rest.split_whitespace().next().unwrap_or("");
    if head.starts_with("HEAD (") {
        branch.detached = true;
        branch.branch = None;
        return;
    }
    if let Some((local, tail)) = head.split_once("...") {
        branch.branch = Some(local.to_string());
        let upstream = tail
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_end_matches(['[', ']']);
        if !upstream.is_empty() {
            branch.upstream = Some(upstream.to_string());
        }
    } else {
        branch.branch = Some(head.to_string());
    }
    if let Some(open) = rest.find('[') {
        if let Some(close) = rest[open..].find(']') {
            let inside = &rest[open + 1..open + close];
            for part in inside.split(',') {
                let p = part.trim();
                if let Some(n) = p.strip_prefix("ahead ") {
                    branch.ahead = n.trim().parse().unwrap_or(0);
                } else if let Some(n) = p.strip_prefix("behind ") {
                    branch.behind = n.trim().parse().unwrap_or(0);
                }
            }
        }
    }
}

pub fn git_status(cwd: &Path) -> Result<GitStatusResponse, GitWorkspaceError> {
    ensure_git_repo(cwd)?;
    let out = run_git(cwd, &["status", "--porcelain=v1", "-b"])?;
    let text = git_ok(out)?;
    Ok(parse_status_porcelain(&text))
}

fn git_untracked_paths(cwd: &Path) -> Result<Vec<String>, GitWorkspaceError> {
    let out = run_git(
        cwd,
        &["ls-files", "--others", "--exclude-standard", "-z"],
    )?;
    let text = git_ok(out)?;
    let mut v = Vec::new();
    for s in text.split('\0') {
        let t = s.trim();
        if t.is_empty() {
            continue;
        }
        if validate_repo_relative_path(t).is_ok() {
            v.push(t.to_string());
        }
    }
    Ok(v)
}

/// Maximum unified diff bytes returned by [`git_diff_head`] (tracked `git diff` + synthetic untracked hunks).
pub const GIT_DIFF_MAX_RESPONSE_BYTES: usize = 512 * 1024;

/// Lines of each untracked file included in the synthetic unified diff (avoids huge single-file payloads).
const GIT_DIFF_MAX_LINES_PER_UNTRACKED_FILE: usize = 400;

const GIT_DIFF_MAX_CHARS_PER_LINE: usize = 4096;

#[derive(Debug, Clone, serde::Serialize)]
pub struct GitSpokeDiff {
    pub unified_diff: String,
    pub truncated: bool,
    pub max_bytes: usize,
}

/// Run `git` with stdout capped at `max_bytes` (kills child if limit hit). Used for large diffs.
fn spawn_git_read_limited(
    cwd: &Path,
    max_bytes: usize,
    configure: impl FnOnce(&mut Command),
) -> Result<(String, bool), GitWorkspaceError> {
    let mut cmd = Command::new("git");
    configure(&mut cmd);
    cmd.current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("LANG", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| GitWorkspaceError::GitSpawn(e.to_string()))?;
    let mut stdout = child.stdout.take().ok_or_else(|| {
        GitWorkspaceError::GitSpawn("git diff: missing stdout".to_string())
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        GitWorkspaceError::GitSpawn("git diff: missing stderr".to_string())
    })?;

    let stderr_handle = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = std::io::BufReader::new(stderr).read_to_string(&mut s);
        s
    });

    let mut buf: Vec<u8> = Vec::new();
    let mut tmp = [0u8; 65536];
    let mut hit_byte_limit = false;
    loop {
        if buf.len() >= max_bytes {
            hit_byte_limit = true;
            break;
        }
        let n = stdout
            .read(&mut tmp)
            .map_err(|e| GitWorkspaceError::GitSpawn(e.to_string()))?;
        if n == 0 {
            break;
        }
        let room = max_bytes.saturating_sub(buf.len());
        if n <= room {
            buf.extend_from_slice(&tmp[..n]);
        } else {
            buf.extend_from_slice(&tmp[..room]);
            hit_byte_limit = true;
            break;
        }
    }
    drop(stdout);

    if hit_byte_limit {
        let _ = child.kill();
    }
    let status = child
        .wait()
        .map_err(|e| GitWorkspaceError::GitSpawn(e.to_string()))?;
    let stderr_text = stderr_handle.join().unwrap_or_default();

    if hit_byte_limit {
        truncate_diff_bytes_at_last_newline(&mut buf);
        let s = String::from_utf8_lossy(&buf).into_owned();
        return Ok((s, true));
    }

    if !status.success() {
        return Err(GitWorkspaceError::GitFailed {
            code: status.code(),
            stderr: stderr_text,
            stdout: String::from_utf8_lossy(&buf).into_owned(),
        });
    }

    Ok((String::from_utf8_lossy(&buf).into_owned(), false))
}

fn read_git_diff_head_streaming(
    cwd: &Path,
    max_bytes: usize,
) -> Result<(String, bool), GitWorkspaceError> {
    spawn_git_read_limited(cwd, max_bytes, |c| {
        c.args(["diff", "HEAD"]);
    })
}

pub fn validate_git_rev(rev: &str) -> Result<(), GitWorkspaceError> {
    let t = rev.trim();
    if !(4..=64).contains(&t.len()) {
        return Err(GitWorkspaceError::InvalidPathArg);
    }
    if !t.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(GitWorkspaceError::InvalidPathArg);
    }
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct GitLogEntry {
    pub oid: String,
    pub short_oid: String,
    pub subject: String,
    pub author: String,
    pub email: String,
    pub date: String,
}

/// Recent commits on `HEAD` (current branch / checked-out commit).
pub fn git_log_head(cwd: &Path, limit: usize) -> Result<Vec<GitLogEntry>, GitWorkspaceError> {
    ensure_git_repo(cwd)?;
    let n = limit.clamp(1, 200);
    let n_str = n.to_string();
    let pretty = "--pretty=format:%H%x1f%s%x1f%an%x1f%ae%x1f%ci%x00".to_string();
    let out = run_git(cwd, &["log", "-n", n_str.as_str(), pretty.as_str()])?;
    let text = git_ok(out)?;
    let mut entries = Vec::new();
    for record in text.split('\0') {
        if record.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = record.split('\x1f').collect();
        if parts.len() < 5 {
            continue;
        }
        let oid = parts[0].trim().to_string();
        if oid.len() < 4 {
            continue;
        }
        let short_oid: String = oid.chars().take(7).collect();
        entries.push(GitLogEntry {
            oid,
            short_oid,
            subject: parts[1].to_string(),
            author: parts[2].to_string(),
            email: parts[3].to_string(),
            date: parts[4].to_string(),
        });
    }
    Ok(entries)
}

/// Unified diff for a single commit (`git show` patch only), size-capped like [`git_diff_head`].
pub fn git_commit_patch(cwd: &Path, rev: &str, max_bytes: usize) -> Result<GitSpokeDiff, GitWorkspaceError> {
    ensure_git_repo(cwd)?;
    validate_git_rev(rev)?;
    let cap = max_bytes.max(4096);
    let rev_owned = rev.to_string();
    let (unified_diff, truncated) = spawn_git_read_limited(cwd, cap, move |c| {
        c.arg("show")
            .arg("--no-color")
            .arg("--pretty=format:")
            .arg(rev_owned);
    })?;
    Ok(GitSpokeDiff {
        unified_diff,
        truncated,
        max_bytes: cap,
    })
}

fn truncate_diff_bytes_at_last_newline(buf: &mut Vec<u8>) {
    if let Some(i) = buf.iter().rposition(|b| *b == b'\n') {
        buf.truncate(i + 1);
    } else {
        buf.clear();
    }
}

fn truncate_diff_line(line: &str) -> String {
    let mut it = line.chars();
    let head: String = it.by_ref().take(GIT_DIFF_MAX_CHARS_PER_LINE).collect();
    if it.next().is_some() {
        let mut h = head;
        h.push('…');
        h
    } else {
        head
    }
}

/// Append synthetic unified diffs for untracked files until `into.len()` reaches `max_total_bytes`.
fn append_untracked_unified_diffs_limited(
    cwd: &Path,
    into: &mut String,
    max_total_bytes: usize,
) -> Result<bool, GitWorkspaceError> {
    for rel in git_untracked_paths(cwd)? {
        if into.len() >= max_total_bytes {
            return Ok(true);
        }
        let full = cwd.join(&rel);
        let raw = std::fs::read_to_string(&full).unwrap_or_default();
        let total_lines = raw.lines().count();
        let lines: Vec<&str> = raw
            .lines()
            .take(GIT_DIFF_MAX_LINES_PER_UNTRACKED_FILE)
            .collect();
        let file_truncated = total_lines > lines.len();
        let n = if lines.is_empty() && raw.is_empty() {
            0usize
        } else {
            lines.len().max(1)
        };
        let mut chunk = String::new();
        chunk.push_str(&format!("diff --git a/{rel} b/{rel}\n", rel = rel));
        chunk.push_str("new file mode 100644\n");
        chunk.push_str("--- /dev/null\n");
        chunk.push_str(&format!("+++ b/{rel}\n", rel = rel));
        chunk.push_str(&format!("@@ -0,0 +1,{n} @@\n"));
        if lines.is_empty() {
            if !raw.is_empty() {
                chunk.push('+');
                let one = truncate_diff_line(raw.lines().next().unwrap_or(""));
                chunk.push_str(&one);
                chunk.push('\n');
            }
        } else {
            for line in &lines {
                chunk.push('+');
                chunk.push_str(&truncate_diff_line(line));
                chunk.push('\n');
            }
            if file_truncated {
                chunk.push_str(&format!(
                    "+... ({} lines omitted)\n",
                    total_lines.saturating_sub(lines.len())
                ));
            }
        }
        if into.len() + chunk.len() > max_total_bytes {
            return Ok(true);
        }
        into.push_str(&chunk);
    }
    Ok(false)
}

/// Unified diff vs `HEAD` (tracked) plus untracked file hunks, capped for API/UI use.
pub fn git_diff_head(cwd: &Path) -> Result<GitSpokeDiff, GitWorkspaceError> {
    git_diff_head_with_limit(cwd, GIT_DIFF_MAX_RESPONSE_BYTES)
}

pub fn git_diff_head_with_limit(cwd: &Path, max_bytes: usize) -> Result<GitSpokeDiff, GitWorkspaceError> {
    ensure_git_repo(cwd)?;
    let cap = max_bytes.max(4096);
    let (mut unified_diff, mut truncated) = read_git_diff_head_streaming(cwd, cap)?;
    let budget = cap.saturating_sub(unified_diff.len());
    if budget > 64 {
        if append_untracked_unified_diffs_limited(cwd, &mut unified_diff, cap)? {
            truncated = true;
        }
    } else if !git_untracked_paths(cwd)?.is_empty() {
        truncated = true;
    }
    if unified_diff.len() > cap {
        unified_diff.truncate(cap);
        while !unified_diff.is_char_boundary(unified_diff.len()) {
            unified_diff.pop();
        }
        if let Some(i) = unified_diff.rfind('\n') {
            unified_diff.truncate(i + 1);
        }
        truncated = true;
    }
    Ok(GitSpokeDiff {
        unified_diff,
        truncated,
        max_bytes: cap,
    })
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct GitBranchRow {
    pub name: String,
    pub current: bool,
}

pub fn git_branches(cwd: &Path) -> Result<Vec<GitBranchRow>, GitWorkspaceError> {
    ensure_git_repo(cwd)?;
    let out = run_git(cwd, &["branch", "-a"])?;
    let text = git_ok(out)?;
    let mut rows = Vec::new();
    for line in text.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        let (current, name) = if let Some(rest) = line.strip_prefix('*') {
            (true, rest.trim())
        } else {
            (false, line.trim())
        };
        if name.is_empty() || name.contains(" -> ") {
            continue;
        }
        rows.push(GitBranchRow {
            name: name.to_string(),
            current,
        });
    }
    Ok(rows)
}

pub fn validate_repo_relative_path(rel: &str) -> Result<(), GitWorkspaceError> {
    let t = rel.trim();
    if t.is_empty() || t.contains("..") {
        return Err(GitWorkspaceError::InvalidPathArg);
    }
    if t.starts_with('/') || t.starts_with('\\') {
        return Err(GitWorkspaceError::InvalidPathArg);
    }
    Ok(())
}

pub fn git_add_path(cwd: &Path, rel: &str) -> Result<(), GitWorkspaceError> {
    ensure_git_repo(cwd)?;
    validate_repo_relative_path(rel)?;
    let out = run_git(cwd, &["add", "--", rel])?;
    git_ok(out).map(|_| ())
}

pub fn git_add_all(cwd: &Path) -> Result<(), GitWorkspaceError> {
    ensure_git_repo(cwd)?;
    let out = run_git(cwd, &["add", "-A"])?;
    git_ok(out).map(|_| ())
}

pub fn git_reset_path(cwd: &Path, rel: &str) -> Result<(), GitWorkspaceError> {
    ensure_git_repo(cwd)?;
    validate_repo_relative_path(rel)?;
    // `git restore --staged` (2.23+); fall back to `reset` for older Git.
    let out = run_git(cwd, &["restore", "--staged", "--", rel])?;
    if out.status.success() {
        return Ok(());
    }
    let stderr_lossy = String::from_utf8_lossy(&out.stderr);
    let stderr = stderr_lossy.as_ref();
    let old_git = stderr.contains("is not a git command")
        || (stderr.contains("unknown") && stderr.contains("restore"));
    if old_git {
        let out2 = run_git(cwd, &["reset", "HEAD", "--", rel])?;
        return git_ok(out2).map(|_| ());
    }
    Err(GitWorkspaceError::GitFailed {
        code: out.status.code(),
        stderr: stderr_lossy.into_owned(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
    })
}

pub fn git_commit(cwd: &Path, message: &str) -> Result<(), GitWorkspaceError> {
    ensure_git_repo(cwd)?;
    let msg = message.trim();
    if msg.is_empty() {
        return Err(GitWorkspaceError::GitFailed {
            code: None,
            stderr: "commit message is empty".into(),
            stdout: String::new(),
        });
    }
    let out = run_git(cwd, &["commit", "-m", msg])?;
    git_ok(out).map(|_| ())
}

pub fn git_checkout(cwd: &Path, branch: &str, create: bool) -> Result<(), GitWorkspaceError> {
    ensure_git_repo(cwd)?;
    let b = branch.trim();
    if b.is_empty() || b.contains("..") {
        return Err(GitWorkspaceError::InvalidPathArg);
    }
    let args: Vec<&str> = if create {
        vec!["checkout", "-b", b]
    } else {
        vec!["checkout", b]
    };
    let out = run_git(cwd, &args)?;
    git_ok(out).map(|_| ())
}

pub fn git_push(cwd: &Path) -> Result<(), GitWorkspaceError> {
    ensure_git_repo(cwd)?;
    let out = run_git(cwd, &["push"])?;
    git_ok(out).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_branch_main_upstream_ahead() {
        let mut b = GitBranchInfo {
            branch: None,
            detached: false,
            upstream: None,
            ahead: 0,
            behind: 0,
        };
        parse_branch_line("main...origin/main [ahead 2, behind 1]", &mut b);
        assert_eq!(b.branch.as_deref(), Some("main"));
        assert_eq!(b.upstream.as_deref(), Some("origin/main"));
        assert_eq!(b.ahead, 2);
        assert_eq!(b.behind, 1);
    }

    #[test]
    fn parse_porcelain_files() {
        let raw = "## main...origin/main\n M a.txt\nM  b.txt\n?? new.txt\n";
        let s = parse_status_porcelain(raw);
        assert_eq!(s.files.len(), 3);
        assert!(s.dirty);
        assert!(s.files[2].untracked);
    }

    #[test]
    fn truncate_diff_bytes_keeps_last_full_line() {
        let mut v = b"aaa\nbbb\nccc".to_vec();
        v.truncate(6);
        super::truncate_diff_bytes_at_last_newline(&mut v);
        assert_eq!(v, b"aaa\n");
    }

    #[test]
    fn parse_porcelain_strips_quoted_path() {
        let raw = "## main\nA  \"a b.txt\"\n";
        let s = parse_status_porcelain(raw);
        assert_eq!(s.files.len(), 1);
        assert_eq!(s.files[0].path, "a b.txt");
        assert!(s.files[0].staged);
    }

    #[test]
    fn parse_porcelain_unescapes_c_style_quotes_in_path() {
        let raw = "## main\n?? \"foo\\\"bar.txt\"\n";
        let s = parse_status_porcelain(raw);
        assert_eq!(s.files.len(), 1);
        assert_eq!(s.files[0].path, "foo\"bar.txt");
    }
}
