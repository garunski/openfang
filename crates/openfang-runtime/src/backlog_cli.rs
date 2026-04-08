//! Shared `backlog` CLI invocation (kernel + tool runner).

use std::path::Path;

/// Timeout for `backlog` subprocess (large repos / slow disks).
pub const BACKLOG_CLI_TIMEOUT_SECS: u64 = 300;

/// Run `backlog` with given args and working directory.
pub async fn run_backlog_cli(cwd: &Path, args: &[String]) -> Result<(i32, String, String), String> {
    let mut cmd = tokio::process::Command::new("backlog");
    for a in args {
        cmd.arg(a);
    }
    cmd.current_dir(cwd);
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(BACKLOG_CLI_TIMEOUT_SECS),
        cmd.output(),
    )
    .await
    .map_err(|_| format!("backlog CLI timed out after {BACKLOG_CLI_TIMEOUT_SECS}s"))?
    .map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "backlog CLI not found on PATH (install backlog.md / mise tool 'backlog')".to_string()
        } else {
            format!("Failed to run backlog: {e}")
        }
    })?;

    let code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    Ok((code, stdout, stderr))
}
