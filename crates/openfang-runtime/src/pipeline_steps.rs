//! Shared subprocess steps for quality gate and Cursor worker (used by tools and `PipelineRunner`).

use crate::kernel_handle::KernelHandle;
use std::path::Path;

/// Timeout for full workspace build + test + clippy via mise.
pub(crate) const ENFORCE_QA_GATE_TIMEOUT_SECS: u64 = 3600;

/// Extra `cursor agent` argv tokens permitted via the `flags` tool parameter.
pub(crate) const CURSOR_AGENT_ALLOWLISTED_FLAGS: &[&str] = &["--yolo", "--force"];

/// Timeout for Cursor Agent CLI runs (build/test-scale work).
pub(crate) const TRIGGER_CURSOR_WORKER_TIMEOUT_SECS: u64 = 3600;

/// Default implementation contract for `trigger_cursor_worker` in `agent` mode.
const CURSOR_IMPLEMENT_CONTRACT_REF: &str = "Follow the implementation behavior contract in `.cursor/skills/implement/SKILL.md` in this workspace (read it at the start of the run). If that file is missing, still: read the task markdown from the path or task id in the prompt, implement until every acceptance criterion is satisfied, run `mise run 001-qa` from this workspace root before you finish, and honor `.cursorignore` and `.cursor/rules`.";

/// Agent-mode contract when `behavior` references `.cursor/skills/test-write/SKILL.md`.
const CURSOR_TEST_WRITE_CONTRACT_REF: &str = "Follow the test authoring contract in `.cursor/skills/test-write/SKILL.md` in this workspace (read it at the start of the run). If that file is missing, still: use the task prompt to write or update automated tests only (minimal production edits if required for testability), run `mise run 001-qa` from this workspace root when available or the repo’s documented test command, and honor `.cursorignore` and `.cursor/rules`.";

/// Substring that switches agent mode from the default implement skill to the test-write skill.
const TEST_WRITE_SKILL_PATH: &str = ".cursor/skills/test-write/SKILL.md";

/// Builds the `-p` payload for Cursor Agent CLI. In `agent` mode, appends the implementation contract
/// (or the test-write contract when `behavior` references `test-write/SKILL.md`);
/// optional `behavior` adds orchestrator notes after it.
pub(crate) fn compose_trigger_cursor_worker_prompt(
    prompt: &str,
    mode: &str,
    behavior: Option<&str>,
) -> String {
    let extra = behavior.map(str::trim).filter(|s| !s.is_empty());
    let use_test_write = extra
        .map(|b| b.contains(TEST_WRITE_SKILL_PATH))
        .unwrap_or(false);
    if mode == "agent" {
        let mut block = String::from("--- Behavior / expectations ---\n");
        if use_test_write {
            block.push_str(CURSOR_TEST_WRITE_CONTRACT_REF);
        } else {
            block.push_str(CURSOR_IMPLEMENT_CONTRACT_REF);
        }
        if let Some(b) = extra {
            block.push_str("\n\n--- Additional behavior ---\n");
            block.push_str(b);
        }
        format!("{prompt}\n\n{block}")
    } else if let Some(b) = extra {
        format!("{prompt}\n\n--- Behavior / expectations ---\n{b}")
    } else {
        prompt.to_string()
    }
}

pub(crate) fn parse_cursor_agent_extra_flags(
    input: &serde_json::Value,
) -> Result<Vec<String>, String> {
    match input.get("flags") {
        None => Ok(Vec::new()),
        Some(v) if v.is_null() => Ok(Vec::new()),
        Some(v) => {
            let arr = v
                .as_array()
                .ok_or_else(|| "flags must be a JSON array of strings".to_string())?;
            let mut out = Vec::with_capacity(arr.len());
            for item in arr {
                let s = item
                    .as_str()
                    .ok_or_else(|| "flags must be a JSON array of strings".to_string())?;
                if !CURSOR_AGENT_ALLOWLISTED_FLAGS.contains(&s) {
                    return Err(format!(
                        "flag '{s}' is not allowlisted; allowed: {}",
                        CURSOR_AGENT_ALLOWLISTED_FLAGS.join(", ")
                    ));
                }
                out.push(s.to_string());
            }
            Ok(out)
        }
    }
}

#[derive(Debug, Clone)]
pub struct PipelineGateOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone)]
pub struct PipelineCursorOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub structured_output: Option<serde_json::Value>,
}

pub(crate) async fn run_quality_gate(
    kh: &dyn KernelHandle,
    spoke_root: &str,
    task_id: &str,
    actor: &str,
) -> Result<PipelineGateOutput, String> {
    let path = Path::new(spoke_root);
    let resolved = openfang_types::config::validate_spoke_root_allowlisted(
        &kh.automation_spoke_roots(),
        path,
    )?;

    let mut cmd = tokio::process::Command::new("mise");
    cmd.args(["run", "001-qa"]);
    cmd.current_dir(&resolved);
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(ENFORCE_QA_GATE_TIMEOUT_SECS),
        cmd.output(),
    )
    .await
    .map_err(|_| format!("enforce_quality_gate timed out after {ENFORCE_QA_GATE_TIMEOUT_SECS}s"))?
    .map_err(|e| format!("Failed to run mise: {e}"))?;

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    crate::pipeline_audit::log_quality_gate(
        task_id,
        resolved.display().to_string(),
        exit_code,
        actor,
    );

    Ok(PipelineGateOutput {
        exit_code,
        stdout,
        stderr,
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_cursor_worker(
    kh: &dyn KernelHandle,
    workspace: &str,
    task_id: &str,
    prompt: &str,
    mode: &str,
    behavior: Option<&str>,
    extra_flags: &[String],
    actor: &str,
) -> Result<PipelineCursorOutput, String> {
    if !matches!(mode, "agent" | "plan" | "ask") {
        return Err(format!("invalid mode '{mode}'; allowed: agent, plan, ask"));
    }
    let full_prompt = compose_trigger_cursor_worker_prompt(prompt, mode, behavior);

    let path = Path::new(workspace);
    let resolved = openfang_types::config::validate_spoke_root_allowlisted(
        &kh.automation_spoke_roots(),
        path,
    )?;

    crate::cursor_skills::deploy_cursor_skills(&resolved)?;

    let mut cmd = tokio::process::Command::new("cursor");
    cmd.args(["agent", "-d"])
        .arg(&resolved)
        .arg("-p")
        .arg(&full_prompt)
        .arg("--mode")
        .arg(mode)
        .args(["-o", "json"]);
    for f in extra_flags {
        cmd.arg(f);
    }
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(TRIGGER_CURSOR_WORKER_TIMEOUT_SECS),
        cmd.output(),
    )
    .await
    .map_err(|_| {
        format!("trigger_cursor_worker timed out after {TRIGGER_CURSOR_WORKER_TIMEOUT_SECS}s")
    })?
    .map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "cursor CLI not found on PATH (install Cursor and ensure `cursor` is available)"
                .to_string()
        } else {
            format!("Failed to run cursor agent: {e}")
        }
    })?;

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let structured_output: Option<serde_json::Value> = serde_json::from_str(stdout.trim()).ok();

    crate::pipeline_audit::log_cursor_worker(
        task_id,
        resolved.display().to_string(),
        mode.to_string(),
        exit_code,
        actor,
    );

    Ok(PipelineCursorOutput {
        exit_code,
        stdout,
        stderr,
        structured_output,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_trigger_cursor_worker_prompt_agent_includes_skill_ref() {
        let s = compose_trigger_cursor_worker_prompt("do the task", "agent", None);
        assert!(s.contains("do the task"));
        assert!(s.contains(".cursor/skills/implement/SKILL.md"));
        assert!(s.contains("mise run 001-qa"));
        assert!(s.contains(".cursor/rules"));
    }

    #[test]
    fn compose_trigger_cursor_worker_prompt_agent_appends_extra_behavior() {
        let s = compose_trigger_cursor_worker_prompt("p", "agent", Some("fix clippy"));
        assert!(s.contains(".cursor/skills/implement/SKILL.md"));
        assert!(s.contains("fix clippy"));
        assert!(s.contains("--- Additional behavior ---"));
    }

    #[test]
    fn compose_trigger_cursor_worker_prompt_agent_test_write_skill_replaces_implement() {
        let b = "Follow .cursor/skills/test-write/SKILL.md in this workspace.";
        let s = compose_trigger_cursor_worker_prompt("add tests for foo", "agent", Some(b));
        assert!(s.contains("add tests for foo"));
        assert!(s.contains(".cursor/skills/test-write/SKILL.md"));
        assert!(!s.contains(".cursor/skills/implement/SKILL.md"));
        assert!(s.contains("--- Additional behavior ---"));
        assert!(s.contains(b));
    }

    #[test]
    fn compose_trigger_cursor_worker_prompt_ask_explore_behavior() {
        let b = "Follow .cursor/skills/explore/SKILL.md in this workspace.";
        let s = compose_trigger_cursor_worker_prompt("map the api crate", "ask", Some(b));
        assert!(s.contains("map the api crate"));
        assert!(s.contains(".cursor/skills/explore/SKILL.md"));
        assert!(!s.contains("implement/SKILL.md"));
    }

    #[test]
    fn compose_trigger_cursor_worker_prompt_plan_no_default_contract() {
        let s = compose_trigger_cursor_worker_prompt("only", "plan", None);
        assert_eq!(s, "only");
    }

    #[test]
    fn compose_trigger_cursor_worker_prompt_plan_with_behavior() {
        let s = compose_trigger_cursor_worker_prompt("only", "plan", Some("extra"));
        assert!(s.contains("only"));
        assert!(s.contains("extra"));
        assert!(!s.contains("implement/SKILL.md"));
    }
}
