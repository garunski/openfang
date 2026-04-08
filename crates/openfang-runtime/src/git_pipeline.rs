//! Git CLI and GitHub PR helpers for pipeline native tools.

use std::path::Path;
use tokio::process::Command;

const DEFAULT_BRANCH_TEMPLATE: &str = "openfang/task-{task_id}";

pub fn default_git_branch_template() -> &'static str {
    DEFAULT_BRANCH_TEMPLATE
}

/// Resolve branch name from template or explicit override.
pub fn resolve_branch_name(template: &str, task_id: &str, override_name: Option<&str>) -> String {
    if let Some(b) = override_name.map(str::trim).filter(|s| !s.is_empty()) {
        return b.to_string();
    }
    let tid = task_id.trim();
    template.replace("{task_id}", tid)
}

/// Parse `owner`, `repo` from common GitHub remote URL forms.
pub fn parse_github_owner_repo(remote: &str) -> Result<(String, String), String> {
    let r = remote.trim();
    let path = if let Some(rest) = r.strip_prefix("git@github.com:") {
        rest
    } else {
        let needle = "github.com/";
        let idx = r
            .find(needle)
            .ok_or_else(|| "not a github.com remote".to_string())?;
        &r[idx + needle.len()..]
    };
    let path = path.split('?').next().unwrap_or(path);
    let path = path.split('#').next().unwrap_or(path);
    let path = path.strip_suffix(".git").unwrap_or(path);
    let mut parts = path.splitn(2, '/');
    let owner = parts
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "bad github remote (owner)".to_string())?;
    let repo = parts
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "bad github remote (repo)".to_string())?;
    Ok((owner.to_string(), repo.to_string()))
}

pub async fn git_stdout(cwd: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .await
        .map_err(|e| format!("git failed to start: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if !out.status.success() {
        if !stderr.is_empty() {
            return Err(stderr);
        }
        return Err(format!("git {:?} exited {}", args, out.status));
    }
    Ok(stdout)
}

pub async fn git_inside_work_tree(cwd: &Path) -> Result<(), String> {
    let s = git_stdout(cwd, &["rev-parse", "--is-inside-work-tree"]).await?;
    if s != "true" {
        return Err("not a git repository".to_string());
    }
    Ok(())
}

pub async fn git_create_branch(cwd: &Path, branch: &str, base_branch: &str) -> Result<(), String> {
    git_inside_work_tree(cwd).await?;
    // Best-effort checkout of base; caller may use main/master.
    let _ = git_stdout(cwd, &["checkout", base_branch]).await;
    git_stdout(cwd, &["checkout", "-b", branch]).await?;
    Ok(())
}

pub async fn git_commit_and_push(
    cwd: &Path,
    subject: &str,
    body: Option<&str>,
) -> Result<(String, String), String> {
    git_inside_work_tree(cwd).await?;
    git_stdout(cwd, &["add", "-A"]).await?;
    let staged = git_stdout(cwd, &["diff", "--cached", "--name-only"]).await?;
    if staged.trim().is_empty() {
        return Err("nothing to commit (no staged changes after add)".to_string());
    }
    let mut msg = subject.to_string();
    if let Some(b) = body.map(str::trim).filter(|s| !s.is_empty()) {
        msg.push_str("\n\n");
        msg.push_str(b);
    }
    git_stdout(cwd, &["commit", "-m", &msg]).await?;
    let sha = git_stdout(cwd, &["rev-parse", "HEAD"]).await?;
    let branch = git_stdout(cwd, &["rev-parse", "--abbrev-ref", "HEAD"]).await?;
    git_stdout(cwd, &["push", "-u", "origin", &branch]).await?;
    Ok((sha, branch))
}

pub async fn git_remote_origin_url(cwd: &Path) -> Result<String, String> {
    git_stdout(cwd, &["remote", "get-url", "origin"]).await
}

pub async fn github_create_pull_request(
    token: &str,
    owner: &str,
    repo: &str,
    title: &str,
    body: &str,
    head_branch: &str,
    base_branch: &str,
) -> Result<serde_json::Value, String> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}/pulls");
    let client = reqwest::Client::builder()
        .user_agent(crate::USER_AGENT)
        .build()
        .map_err(|e| e.to_string())?;
    let body_json = serde_json::json!({
        "title": title,
        "body": body,
        "head": head_branch,
        "base": base_branch,
    });
    let resp = client
        .post(&url)
        .header("Accept", "application/vnd.github+json")
        .header("Authorization", format!("Bearer {token}"))
        .json(&body_json)
        .send()
        .await
        .map_err(|e| format!("GitHub request failed: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("GitHub API {status}: {text}"));
    }
    serde_json::from_str(&text).map_err(|e| format!("GitHub JSON: {e}: {text}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_github_ssh() {
        let (o, r) = parse_github_owner_repo("git@github.com:acme/widget.git").unwrap();
        assert_eq!(o, "acme");
        assert_eq!(r, "widget");
    }

    #[test]
    fn parse_github_https() {
        let (o, r) = parse_github_owner_repo("https://github.com/foo/bar").unwrap();
        assert_eq!(o, "foo");
        assert_eq!(r, "bar");
    }

    #[test]
    fn branch_template() {
        assert_eq!(
            resolve_branch_name("feat/{task_id}", "TASK-1", None),
            "feat/TASK-1"
        );
        assert_eq!(resolve_branch_name("x", "TASK-1", Some("custom")), "custom");
    }
}
