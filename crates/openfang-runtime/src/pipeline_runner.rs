//! Hard-enforced implement → quality gate → retry loop (bounded by `max_retries`).

use crate::kernel_handle::KernelHandle;
use crate::pipeline_steps::{
    run_cursor_worker, run_quality_gate, PipelineCursorOutput, PipelineGateOutput,
};
use async_trait::async_trait;
use std::sync::Arc;

/// Successful run: last gate passed (`exit_code` 0).
#[derive(Debug, Clone)]
pub struct PipelineRunOk {
    pub gate: PipelineGateOutput,
    /// Cursor re-invocations after a failed gate (before the passing gate).
    pub retry_count: u32,
}

/// Exhausted retries, subprocess error, or unrecoverable gate error.
#[derive(Debug, Clone)]
pub struct PipelineRunErr {
    pub retry_count: u32,
    pub last_gate_stderr: String,
    pub last_gate_stdout: String,
    pub last_gate_exit_code: i32,
}

#[async_trait]
#[allow(clippy::too_many_arguments)]
pub(crate) trait PipelineExecutor: Send + Sync {
    async fn run_gate(
        &self,
        spoke_root: &str,
        task_id: &str,
        actor: &str,
    ) -> Result<PipelineGateOutput, String>;

    async fn run_cursor(
        &self,
        workspace: &str,
        task_id: &str,
        prompt: &str,
        mode: &str,
        behavior: Option<&str>,
        extra_flags: &[String],
        actor: &str,
    ) -> Result<PipelineCursorOutput, String>;
}

struct LivePipelineExecutor {
    kernel: Arc<dyn KernelHandle>,
}

#[async_trait]
impl PipelineExecutor for LivePipelineExecutor {
    async fn run_gate(
        &self,
        spoke_root: &str,
        task_id: &str,
        actor: &str,
    ) -> Result<PipelineGateOutput, String> {
        run_quality_gate(self.kernel.as_ref(), spoke_root, task_id, actor).await
    }

    async fn run_cursor(
        &self,
        workspace: &str,
        task_id: &str,
        prompt: &str,
        mode: &str,
        behavior: Option<&str>,
        extra_flags: &[String],
        actor: &str,
    ) -> Result<PipelineCursorOutput, String> {
        run_cursor_worker(
            self.kernel.as_ref(),
            workspace,
            task_id,
            prompt,
            mode,
            behavior,
            extra_flags,
            actor,
        )
        .await
    }
}

/// Runs Cursor then `mise run 001-qa` in a loop; gate failures trigger retries up to `max_retries`.
pub struct PipelineRunner {
    executor: Arc<dyn PipelineExecutor>,
    caller_agent_id: Option<String>,
}

impl PipelineRunner {
    pub fn new(kernel: Arc<dyn KernelHandle>, caller_agent_id: Option<String>) -> Self {
        Self {
            executor: Arc::new(LivePipelineExecutor { kernel }),
            caller_agent_id,
        }
    }

    #[cfg(test)]
    fn with_executor(executor: Arc<dyn PipelineExecutor>, caller_agent_id: Option<String>) -> Self {
        Self {
            executor,
            caller_agent_id,
        }
    }

    fn actor(&self) -> String {
        self.caller_agent_id
            .as_deref()
            .unwrap_or("unknown")
            .to_string()
    }

    /// `max_retries` is the number of **extra** Cursor rounds after a failed gate (same as pipeline-coordinator HAND).
    #[allow(clippy::too_many_arguments)]
    pub async fn run(
        &self,
        task_id: &str,
        workspace: &str,
        prompt: &str,
        max_retries: u32,
        mode: &str,
        behavior: Option<&str>,
        extra_flags: &[String],
        rollback_to_status: Option<&str>,
    ) -> Result<PipelineRunOk, PipelineRunErr> {
        let actor = self.actor();
        let rb = rollback_to_status.map(str::to_string);
        let mut retry_count: u32 = 0;
        let mut current_prompt = prompt.to_string();

        loop {
            if let Err(e) = self
                .executor
                .run_cursor(
                    workspace,
                    task_id,
                    &current_prompt,
                    mode,
                    behavior,
                    extra_flags,
                    &actor,
                )
                .await
            {
                crate::pipeline_audit::log_pipeline_run_outcome(
                    task_id,
                    false,
                    retry_count,
                    rb.clone(),
                    &actor,
                    "run_pipeline",
                );
                return Err(PipelineRunErr {
                    retry_count,
                    last_gate_stderr: e,
                    last_gate_stdout: String::new(),
                    last_gate_exit_code: -1,
                });
            }

            let gate = match self.executor.run_gate(workspace, task_id, &actor).await {
                Ok(g) => g,
                Err(e) => {
                    crate::pipeline_audit::log_pipeline_run_outcome(
                        task_id,
                        false,
                        retry_count,
                        rb.clone(),
                        &actor,
                        "run_pipeline",
                    );
                    return Err(PipelineRunErr {
                        retry_count,
                        last_gate_stderr: e,
                        last_gate_stdout: String::new(),
                        last_gate_exit_code: -1,
                    });
                }
            };

            if gate.exit_code == 0 {
                crate::pipeline_audit::log_pipeline_run_outcome(
                    task_id,
                    true,
                    retry_count,
                    None,
                    &actor,
                    "run_pipeline",
                );
                return Ok(PipelineRunOk { gate, retry_count });
            }

            if retry_count >= max_retries {
                crate::pipeline_audit::log_pipeline_run_outcome(
                    task_id,
                    false,
                    retry_count,
                    rb.clone(),
                    &actor,
                    "run_pipeline",
                );
                return Err(PipelineRunErr {
                    retry_count,
                    last_gate_stderr: gate.stderr,
                    last_gate_stdout: gate.stdout,
                    last_gate_exit_code: gate.exit_code,
                });
            }

            retry_count += 1;
            current_prompt = format!(
                "{}\n\n--- Previous quality gate failure (stderr) ---\n{}",
                current_prompt, gate.stderr
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    struct MockPipelineExecutor {
        gate_results: Mutex<VecDeque<PipelineGateOutput>>,
    }

    impl MockPipelineExecutor {
        fn new(gates: impl IntoIterator<Item = PipelineGateOutput>) -> Self {
            Self {
                gate_results: Mutex::new(VecDeque::from_iter(gates)),
            }
        }
    }

    #[async_trait]
    impl PipelineExecutor for MockPipelineExecutor {
        async fn run_gate(
            &self,
            _spoke_root: &str,
            _task_id: &str,
            _actor: &str,
        ) -> Result<PipelineGateOutput, String> {
            self.gate_results
                .lock()
                .unwrap()
                .pop_front()
                .ok_or_else(|| "exhausted mock gate results".to_string())
        }

        async fn run_cursor(
            &self,
            _workspace: &str,
            _task_id: &str,
            _prompt: &str,
            _mode: &str,
            _behavior: Option<&str>,
            _extra_flags: &[String],
            _actor: &str,
        ) -> Result<PipelineCursorOutput, String> {
            Ok(PipelineCursorOutput {
                exit_code: 0,
                stdout: String::new(),
                stderr: String::new(),
                structured_output: None,
            })
        }
    }

    #[tokio::test]
    async fn pipeline_passes_first_gate() {
        let mock = Arc::new(MockPipelineExecutor::new(VecDeque::from([
            PipelineGateOutput {
                exit_code: 0,
                stdout: "ok".into(),
                stderr: String::new(),
            },
        ])));
        let runner = PipelineRunner::with_executor(mock, Some("agent-1".into()));
        let ok = runner
            .run(
                "TASK-1",
                "/tmp/w",
                "do it",
                2,
                "agent",
                None,
                &[],
                Some("In Progress"),
            )
            .await
            .unwrap();
        assert_eq!(ok.retry_count, 0);
        assert_eq!(ok.gate.exit_code, 0);
    }

    #[tokio::test]
    async fn pipeline_fail_then_pass_on_retry() {
        let mock = Arc::new(MockPipelineExecutor::new(VecDeque::from([
            PipelineGateOutput {
                exit_code: 1,
                stdout: String::new(),
                stderr: "clippy".into(),
            },
            PipelineGateOutput {
                exit_code: 0,
                stdout: String::new(),
                stderr: String::new(),
            },
        ])));
        let runner = PipelineRunner::with_executor(mock, None);
        let ok = runner
            .run("TASK-2", "/w", "task", 2, "agent", None, &[], None)
            .await
            .unwrap();
        assert_eq!(ok.retry_count, 1);
    }

    #[tokio::test]
    async fn pipeline_exhausts_retries() {
        let mock = Arc::new(MockPipelineExecutor::new(VecDeque::from([
            PipelineGateOutput {
                exit_code: 1,
                stdout: String::new(),
                stderr: "e1".into(),
            },
            PipelineGateOutput {
                exit_code: 1,
                stdout: String::new(),
                stderr: "e2".into(),
            },
        ])));
        let runner = PipelineRunner::with_executor(mock, None);
        let err = runner
            .run(
                "TASK-3",
                "/w",
                "task",
                1,
                "agent",
                None,
                &[],
                Some("Ready for Dev"),
            )
            .await
            .unwrap_err();
        assert_eq!(err.retry_count, 1);
        assert_eq!(err.last_gate_stderr, "e2");
    }
}
