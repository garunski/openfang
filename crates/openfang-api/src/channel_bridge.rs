//! Channel bridge wiring — connects the OpenFang kernel to channel adapters.
//!
//! Implements `ChannelBridgeHandle` on `OpenFangKernel` and provides the
//! `start_channel_bridge()` entry point called by the daemon.

use async_trait::async_trait;
use openfang_channels::bridge::{BridgeManager, ChannelBridgeHandle};
use openfang_channels::mattermost::MattermostAdapter;
use openfang_channels::router::AgentRouter;
use openfang_channels::signal::SignalAdapter;
use openfang_channels::types::ChannelAdapter;
use openfang_kernel::OpenFangKernel;
use openfang_types::agent::AgentId;
use std::sync::Arc;
use std::time::Instant;
use tracing::{error, info, warn};

use openfang_runtime::str_utils::safe_truncate_str;

/// Wraps `OpenFangKernel` to implement `ChannelBridgeHandle`.
pub struct KernelBridgeAdapter {
    kernel: Arc<OpenFangKernel>,
    started_at: Instant,
}

#[async_trait]
impl ChannelBridgeHandle for KernelBridgeAdapter {
    async fn send_message(&self, agent_id: AgentId, message: &str) -> Result<String, String> {
        let result = self
            .kernel
            .send_message(agent_id, message)
            .await
            .map_err(|e| format!("{e}"))?;
        // Silent/NO_REPLY responses should not be forwarded to channels
        if result.silent {
            return Ok(String::new());
        }
        Ok(result.response)
    }

    async fn send_message_with_blocks(
        &self,
        agent_id: AgentId,
        blocks: Vec<openfang_types::message::ContentBlock>,
    ) -> Result<String, String> {
        // Extract text for the message parameter (used for memory recall / logging)
        let text: String = blocks
            .iter()
            .filter_map(|b| match b {
                openfang_types::message::ContentBlock::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        let text = if text.is_empty() {
            "[Image]".to_string()
        } else {
            text
        };
        let result = self
            .kernel
            .send_message_with_blocks(agent_id, &text, blocks)
            .await
            .map_err(|e| format!("{e}"))?;
        Ok(result.response)
    }

    async fn find_agent_by_name(&self, name: &str) -> Result<Option<AgentId>, String> {
        Ok(self.kernel.registry.find_by_name(name).map(|e| e.id))
    }

    async fn list_agents(&self) -> Result<Vec<(AgentId, String)>, String> {
        Ok(self
            .kernel
            .registry
            .list()
            .iter()
            .map(|e| (e.id, e.name.clone()))
            .collect())
    }

    async fn spawn_agent_by_name(&self, manifest_name: &str) -> Result<AgentId, String> {
        // Look for manifest at ~/.openfang/agents/{name}/agent.toml
        let manifest_path = self
            .kernel
            .config
            .home_dir
            .join("agents")
            .join(manifest_name)
            .join("agent.toml");

        if !manifest_path.exists() {
            return Err(format!("Manifest not found: {}", manifest_path.display()));
        }

        let contents = std::fs::read_to_string(&manifest_path)
            .map_err(|e| format!("Failed to read manifest: {e}"))?;

        let manifest: openfang_types::agent::AgentManifest =
            toml::from_str(&contents).map_err(|e| format!("Invalid manifest TOML: {e}"))?;

        let agent_id = self
            .kernel
            .spawn_agent(manifest)
            .map_err(|e| format!("Failed to spawn agent: {e}"))?;

        Ok(agent_id)
    }

    async fn uptime_info(&self) -> String {
        let uptime = self.started_at.elapsed();
        let agents = self.list_agents().await.unwrap_or_default();
        let secs = uptime.as_secs();
        let hours = secs / 3600;
        let mins = (secs % 3600) / 60;
        if hours > 0 {
            format!(
                "OpenFang status: {}h {}m uptime, {} agent(s)",
                hours,
                mins,
                agents.len()
            )
        } else {
            format!(
                "OpenFang status: {}m uptime, {} agent(s)",
                mins,
                agents.len()
            )
        }
    }

    async fn list_models_text(&self) -> String {
        let available = self.kernel.available_model_catalog_entries_cloned();
        if available.is_empty() {
            return "No models available. Configure API keys to enable providers.".to_string();
        }
        let mut msg = format!("Available models ({}):\n", available.len());
        // Group by provider
        let catalog = self
            .kernel
            .model_catalog
            .read()
            .unwrap_or_else(|e| e.into_inner());
        let mut by_provider: std::collections::HashMap<
            &str,
            Vec<&openfang_types::model_catalog::ModelCatalogEntry>,
        > = std::collections::HashMap::new();
        for m in &available {
            by_provider.entry(m.provider.as_str()).or_default().push(m);
        }
        let mut providers: Vec<&&str> = by_provider.keys().collect();
        providers.sort();
        for provider in providers {
            let provider_name = catalog
                .get_provider(provider)
                .map(|p| p.display_name.as_str())
                .unwrap_or(provider);
            msg.push_str(&format!("\n{}:\n", provider_name));
            for m in &by_provider[provider] {
                let cost = if m.input_cost_per_m > 0.0 {
                    format!(
                        " (${:.2}/${:.2} per M)",
                        m.input_cost_per_m, m.output_cost_per_m
                    )
                } else {
                    " (free/local)".to_string()
                };
                msg.push_str(&format!("  {} — {}{}\n", m.id, m.display_name, cost));
            }
        }
        msg
    }

    async fn list_providers_text(&self) -> String {
        let catalog = self
            .kernel
            .model_catalog
            .read()
            .unwrap_or_else(|e| e.into_inner());
        let mut msg = "Providers:\n".to_string();
        for p in catalog.list_providers() {
            let status = match p.auth_status {
                openfang_types::model_catalog::AuthStatus::Configured => "configured",
                openfang_types::model_catalog::AuthStatus::Missing => "not configured",
                openfang_types::model_catalog::AuthStatus::NotRequired => "local (no key needed)",
            };
            msg.push_str(&format!(
                "  {} — {} [{}, {} model(s)]\n",
                p.id, p.display_name, status, p.model_count
            ));
        }
        msg
    }

    async fn list_skills_text(&self) -> String {
        let skills = self
            .kernel
            .skill_registry
            .read()
            .unwrap_or_else(|e| e.into_inner());
        let skills = skills.list();
        if skills.is_empty() {
            return "No skills installed. Place skills in ~/.openfang/skills/ or install from the marketplace.".to_string();
        }
        let mut msg = format!("Installed skills ({}):\n", skills.len());
        for skill in &skills {
            let runtime = format!("{:?}", skill.manifest.runtime.runtime_type);
            let tools_count = skill.manifest.tools.provided.len();
            let enabled = if skill.enabled { "" } else { " [disabled]" };
            msg.push_str(&format!(
                "  {} — {} ({}, {} tool(s)){}\n",
                skill.manifest.skill.name,
                skill.manifest.skill.description,
                runtime,
                tools_count,
                enabled,
            ));
        }
        msg
    }

    async fn list_hands_text(&self) -> String {
        let defs = self.kernel.hand_registry.list_definitions();
        if defs.is_empty() {
            return "No hands available.".to_string();
        }
        let instances = self.kernel.hand_registry.list_instances();
        let mut msg = format!("Available hands ({}):\n", defs.len());
        for d in &defs {
            let reqs_met = self
                .kernel
                .hand_registry
                .check_requirements(&d.id)
                .map(|r| r.iter().all(|(_, ok)| *ok))
                .unwrap_or(false);
            let badge = if reqs_met { "Ready" } else { "Setup needed" };
            msg.push_str(&format!(
                "  {} {} — {} [{}]\n",
                d.icon, d.name, d.description, badge
            ));
        }
        if !instances.is_empty() {
            msg.push_str(&format!("\nActive ({}):\n", instances.len()));
            for i in &instances {
                msg.push_str(&format!(
                    "  {} — {} ({})\n",
                    i.agent_name, i.hand_id, i.status
                ));
            }
        }
        msg
    }

    // ── Automation: workflows, triggers, schedules, approvals ──

    async fn list_workflows_text(&self) -> String {
        let workflows = self.kernel.workflows.list_workflows().await;
        if workflows.is_empty() {
            return "No workflows defined.".to_string();
        }
        let mut msg = format!("Workflows ({}):\n", workflows.len());
        for wf in &workflows {
            let steps = wf.steps.len();
            let desc = if wf.description.is_empty() {
                String::new()
            } else {
                format!(" — {}", wf.description)
            };
            msg.push_str(&format!("  {} ({} step(s)){}\n", wf.name, steps, desc));
        }
        msg
    }

    async fn run_workflow_text(&self, name: &str, input: &str) -> String {
        let workflows = self.kernel.workflows.list_workflows().await;
        let wf = match workflows.iter().find(|w| w.name.eq_ignore_ascii_case(name)) {
            Some(w) => w.clone(),
            None => return format!("Workflow '{name}' not found. Use /workflows to list."),
        };

        let run_id = match self
            .kernel
            .workflows
            .create_run(wf.id, input.to_string(), None)
            .await
        {
            Some(id) => id,
            None => return "Failed to create workflow run.".to_string(),
        };

        let kernel = self.kernel.clone();
        let registry_ref = &self.kernel.registry;
        let result = self
            .kernel
            .workflows
            .execute_run(
                run_id,
                |step_agent| match step_agent {
                    openfang_kernel::workflow::StepAgent::ById { id } => {
                        let aid: AgentId = id.parse().ok()?;
                        let entry = registry_ref.get(aid)?;
                        Some((aid, entry.name.clone()))
                    }
                    openfang_kernel::workflow::StepAgent::ByName { name } => {
                        let entry = registry_ref.find_by_name(name)?;
                        Some((entry.id, entry.name.clone()))
                    }
                },
                |agent_id, message| {
                    let k = kernel.clone();
                    async move {
                        let result = k
                            .send_message(agent_id, &message)
                            .await
                            .map_err(|e| format!("{e}"))?;
                        Ok((
                            result.response,
                            result.total_usage.input_tokens,
                            result.total_usage.output_tokens,
                        ))
                    }
                },
            )
            .await;

        match result {
            Ok(output) => format!("Workflow '{}' completed:\n{}", wf.name, output),
            Err(e) => format!("Workflow '{}' failed: {}", wf.name, e),
        }
    }

    async fn list_triggers_text(&self) -> String {
        let triggers = self.kernel.triggers.list_all();
        if triggers.is_empty() {
            return "No triggers configured.".to_string();
        }
        let mut msg = format!("Triggers ({}):\n", triggers.len());
        for t in &triggers {
            let agent_name = self
                .kernel
                .registry
                .get(t.agent_id)
                .map(|e| e.name.clone())
                .unwrap_or_else(|| t.agent_id.to_string());
            let status = if t.enabled { "on" } else { "off" };
            let id_str = t.id.0.to_string();
            let id_short = safe_truncate_str(&id_str, 8);
            msg.push_str(&format!(
                "  [{}] {} -> {} ({:?}) fires:{} [{}]\n",
                id_short,
                agent_name,
                t.prompt_template.chars().take(40).collect::<String>(),
                t.pattern,
                t.fire_count,
                status,
            ));
        }
        msg
    }

    async fn create_trigger_text(
        &self,
        agent_name: &str,
        pattern_str: &str,
        prompt: &str,
    ) -> String {
        let agent = match self.kernel.registry.find_by_name(agent_name) {
            Some(e) => e,
            None => return format!("Agent '{agent_name}' not found."),
        };

        let pattern = match parse_trigger_pattern(pattern_str) {
            Some(p) => p,
            None => {
                return format!(
                "Unknown pattern '{pattern_str}'. Valid: lifecycle, spawned:<name>, terminated, \
                 system, system:<keyword>, memory, memory:<key>, match:<text>, all"
            )
            }
        };

        let trigger_id = self
            .kernel
            .triggers
            .register(agent.id, pattern, prompt.to_string(), 0);
        let id_str = trigger_id.0.to_string();
        let id_short = safe_truncate_str(&id_str, 8);
        format!("Trigger created [{id_short}] for agent '{agent_name}'.")
    }

    async fn delete_trigger_text(&self, id_prefix: &str) -> String {
        let triggers = self.kernel.triggers.list_all();
        let matched: Vec<_> = triggers
            .iter()
            .filter(|t| t.id.0.to_string().starts_with(id_prefix))
            .collect();
        match matched.len() {
            0 => format!("No trigger found matching '{id_prefix}'."),
            1 => {
                let t = matched[0];
                if self.kernel.triggers.remove(t.id) {
                    let id_str = t.id.0.to_string();
                    format!("Trigger [{}] removed.", safe_truncate_str(&id_str, 8))
                } else {
                    "Failed to remove trigger.".to_string()
                }
            }
            n => format!("{n} triggers match '{id_prefix}'. Be more specific."),
        }
    }

    async fn list_schedules_text(&self) -> String {
        let jobs = self.kernel.cron_scheduler.list_all_jobs();
        if jobs.is_empty() {
            return "No scheduled jobs.".to_string();
        }
        let mut msg = format!("Cron jobs ({}):\n", jobs.len());
        for job in &jobs {
            let agent_name = self
                .kernel
                .registry
                .get(job.agent_id)
                .map(|e| e.name.clone())
                .unwrap_or_else(|| job.agent_id.to_string());
            let status = if job.enabled { "on" } else { "off" };
            let id_str = job.id.0.to_string();
            let id_short = safe_truncate_str(&id_str, 8);
            let sched = match &job.schedule {
                openfang_types::scheduler::CronSchedule::Cron { expr, .. } => expr.clone(),
                openfang_types::scheduler::CronSchedule::Every { every_secs } => {
                    format!("every {every_secs}s")
                }
                openfang_types::scheduler::CronSchedule::At { at } => {
                    format!("at {}", at.format("%Y-%m-%d %H:%M"))
                }
            };
            let last = job
                .last_run
                .map(|t| t.format("%m-%d %H:%M").to_string())
                .unwrap_or_else(|| "never".to_string());
            msg.push_str(&format!(
                "  [{}] {} — {} ({}) last:{} [{}]\n",
                id_short, job.name, sched, agent_name, last, status,
            ));
        }
        msg
    }

    #[allow(dead_code)]
    async fn manage_schedule_text(&self, action: &str, args: &[String]) -> String {
        match action {
            "add" => {
                // Expected: <agent> <f1> <f2> <f3> <f4> <f5> <message...>
                // 5 cron fields: min hour dom month dow
                if args.len() < 7 {
                    return "Usage: /schedule add <agent> <min> <hour> <dom> <month> <dow> <message>".to_string();
                }
                let agent_name = &args[0];
                let agent = match self.kernel.registry.find_by_name(agent_name) {
                    Some(e) => e,
                    None => return format!("Agent '{agent_name}' not found."),
                };
                let cron_expr = args[1..6].join(" ");
                let message = args[6..].join(" ");

                let job = openfang_types::scheduler::CronJob {
                    id: openfang_types::scheduler::CronJobId::new(),
                    agent_id: agent.id,
                    name: format!("chat-{}", &agent.name),
                    enabled: true,
                    schedule: openfang_types::scheduler::CronSchedule::Cron {
                        expr: cron_expr.clone(),
                        tz: None,
                    },
                    action: openfang_types::scheduler::CronAction::AgentTurn {
                        message: message.clone(),
                        model_override: None,
                        timeout_secs: None,
                    },
                    delivery: openfang_types::scheduler::CronDelivery::None,
                    created_at: chrono::Utc::now(),
                    last_run: None,
                    next_run: None,
                };

                match self.kernel.cron_scheduler.add_job(job, false) {
                    Ok(id) => {
                        let id_str = id.0.to_string();
                        let id_short = safe_truncate_str(&id_str, 8);
                        format!("Job [{id_short}] created: '{cron_expr}' -> {agent_name}: \"{message}\"")
                    }
                    Err(e) => format!("Failed to create job: {e}"),
                }
            }
            "del" => {
                if args.is_empty() {
                    return "Usage: /schedule del <id-prefix>".to_string();
                }
                let prefix = &args[0];
                let jobs = self.kernel.cron_scheduler.list_all_jobs();
                let matched: Vec<_> = jobs
                    .iter()
                    .filter(|j| j.id.0.to_string().starts_with(prefix.as_str()))
                    .collect();
                match matched.len() {
                    0 => format!("No job found matching '{prefix}'."),
                    1 => {
                        let j = matched[0];
                        match self.kernel.cron_scheduler.remove_job(j.id) {
                            Ok(_) => {
                                let id_str = j.id.0.to_string();
                                format!(
                                    "Job [{}] '{}' removed.",
                                    safe_truncate_str(&id_str, 8),
                                    j.name
                                )
                            }
                            Err(e) => format!("Failed to remove job: {e}"),
                        }
                    }
                    n => format!("{n} jobs match '{prefix}'. Be more specific."),
                }
            }
            "run" => {
                if args.is_empty() {
                    return "Usage: /schedule run <id-prefix>".to_string();
                }
                let prefix = &args[0];
                let jobs = self.kernel.cron_scheduler.list_all_jobs();
                let matched: Vec<_> = jobs
                    .iter()
                    .filter(|j| j.id.0.to_string().starts_with(prefix.as_str()))
                    .collect();
                match matched.len() {
                    0 => format!("No job found matching '{prefix}'."),
                    1 => {
                        let j = matched[0];
                        let message = match &j.action {
                            openfang_types::scheduler::CronAction::AgentTurn {
                                message, ..
                            } => message.clone(),
                            openfang_types::scheduler::CronAction::SystemEvent { text } => {
                                text.clone()
                            }
                            openfang_types::scheduler::CronAction::WorkflowRun {
                                workflow_id,
                                input,
                                ..
                            } => {
                                format!(
                                    "Run workflow {workflow_id}{}",
                                    input
                                        .as_deref()
                                        .map(|i| format!(" with input: {i}"))
                                        .unwrap_or_default()
                                )
                            }
                        };
                        match self.kernel.send_message(j.agent_id, &message).await {
                            Ok(result) => {
                                let id_str = j.id.0.to_string();
                                let id_short = safe_truncate_str(&id_str, 8);
                                format!("Job [{id_short}] ran:\n{}", result.response)
                            }
                            Err(e) => format!("Failed to run job: {e}"),
                        }
                    }
                    n => format!("{n} jobs match '{prefix}'. Be more specific."),
                }
            }
            _ => "Unknown schedule action. Use: add, del, run".to_string(),
        }
    }

    async fn list_approvals_text(&self) -> String {
        let pending = self.kernel.approval_manager.list_pending();
        if pending.is_empty() {
            return "No pending approvals.".to_string();
        }
        let mut msg = format!("Pending approvals ({}):\n", pending.len());
        for req in &pending {
            let id_str = req.id.to_string();
            let id_short = safe_truncate_str(&id_str, 8);
            let age_secs = (chrono::Utc::now() - req.requested_at).num_seconds();
            let age = if age_secs >= 60 {
                format!("{}m", age_secs / 60)
            } else {
                format!("{age_secs}s")
            };
            msg.push_str(&format!(
                "  [{}] {} — {} ({:?}) age:{}\n",
                id_short, req.agent_id, req.tool_name, req.risk_level, age,
            ));
            if !req.action_summary.is_empty() {
                msg.push_str(&format!("    {}\n", req.action_summary));
            }
        }
        msg.push_str("\nUse /approve <id> or /reject <id>");
        msg
    }

    async fn resolve_approval_text(&self, id_prefix: &str, approve: bool) -> String {
        let pending = self.kernel.approval_manager.list_pending();
        let matched: Vec<_> = pending
            .iter()
            .filter(|r| r.id.to_string().starts_with(id_prefix))
            .collect();
        match matched.len() {
            0 => format!("No pending approval matching '{id_prefix}'."),
            1 => {
                let req = matched[0];
                let decision = if approve {
                    openfang_types::approval::ApprovalDecision::Approved
                } else {
                    openfang_types::approval::ApprovalDecision::Denied
                };
                match self.kernel.approval_manager.resolve(
                    req.id,
                    decision,
                    Some("channel".to_string()),
                ) {
                    Ok(_) => {
                        let verb = if approve { "Approved" } else { "Rejected" };
                        let id_str = req.id.to_string();
                        format!(
                            "{} [{}] {} — {}",
                            verb,
                            safe_truncate_str(&id_str, 8),
                            req.tool_name,
                            req.agent_id
                        )
                    }
                    Err(e) => format!("Failed to resolve approval: {e}"),
                }
            }
            n => format!("{n} approvals match '{id_prefix}'. Be more specific."),
        }
    }

    async fn reset_session(&self, agent_id: AgentId) -> Result<String, String> {
        self.kernel
            .reset_session(agent_id)
            .map_err(|e| format!("{e}"))?;
        Ok("Session reset. Chat history cleared.".to_string())
    }

    async fn compact_session(&self, agent_id: AgentId) -> Result<String, String> {
        self.kernel
            .compact_agent_session(agent_id)
            .await
            .map_err(|e| format!("{e}"))
    }

    async fn set_model(&self, agent_id: AgentId, model: &str) -> Result<String, String> {
        if model.is_empty() {
            // Show current model
            let entry = self
                .kernel
                .registry
                .get(agent_id)
                .ok_or_else(|| "Agent not found".to_string())?;
            return Ok(format!(
                "Current model: {} (provider: {})",
                entry.manifest.model.model, entry.manifest.model.provider
            ));
        }
        self.kernel
            .set_agent_model(agent_id, model, None)
            .map_err(|e| format!("{e}"))?;
        // Read back resolved model+provider from registry
        let entry = self
            .kernel
            .registry
            .get(agent_id)
            .ok_or_else(|| "Agent not found after model switch".to_string())?;
        Ok(format!(
            "Model switched to: {} (provider: {})",
            entry.manifest.model.model, entry.manifest.model.provider
        ))
    }

    async fn stop_run(&self, agent_id: AgentId) -> Result<String, String> {
        let cancelled = self
            .kernel
            .stop_agent_run(agent_id)
            .map_err(|e| format!("{e}"))?;
        if cancelled {
            Ok("Run cancelled.".to_string())
        } else {
            Ok("No active run to cancel.".to_string())
        }
    }

    async fn session_usage(&self, agent_id: AgentId) -> Result<String, String> {
        let (input, output, cost) = self
            .kernel
            .session_usage_cost(agent_id)
            .map_err(|e| format!("{e}"))?;
        let total = input + output;
        let mut msg = format!("Session usage:\n  Input: ~{input} tokens\n  Output: ~{output} tokens\n  Total: ~{total} tokens");
        if cost > 0.0 {
            msg.push_str(&format!("\n  Estimated cost: ${cost:.4}"));
        }
        Ok(msg)
    }

    async fn set_thinking(&self, _agent_id: AgentId, on: bool) -> Result<String, String> {
        // Future-ready: stores preference but doesn't affect model behavior yet
        let state = if on { "enabled" } else { "disabled" };
        Ok(format!(
            "Extended thinking {state}. (This will take effect when supported by the model.)"
        ))
    }

    async fn channel_overrides(
        &self,
        channel_type: &str,
    ) -> Option<openfang_types::config::ChannelOverrides> {
        let channels = &self.kernel.config.channels;
        match channel_type {
            "signal" => channels.signal.as_ref().map(|c| c.overrides.clone()),
            "mattermost" => channels.mattermost.as_ref().map(|c| c.overrides.clone()),
            _ => None,
        }
    }

    async fn authorize_channel_user(
        &self,
        channel_type: &str,
        platform_id: &str,
        action: &str,
    ) -> Result<(), String> {
        if !self.kernel.auth.is_enabled() {
            return Ok(()); // RBAC not configured — allow all
        }

        let user_id = self
            .kernel
            .auth
            .identify(channel_type, platform_id)
            .ok_or_else(|| "Unrecognized user. Contact an admin to get access.".to_string())?;

        let auth_action = match action {
            "chat" => openfang_kernel::auth::Action::ChatWithAgent,
            "spawn" => openfang_kernel::auth::Action::SpawnAgent,
            "kill" => openfang_kernel::auth::Action::KillAgent,
            "install_skill" => openfang_kernel::auth::Action::InstallSkill,
            _ => openfang_kernel::auth::Action::ChatWithAgent,
        };

        self.kernel
            .auth
            .authorize(user_id, &auth_action)
            .map_err(|e| e.to_string())
    }

    async fn record_delivery(
        &self,
        agent_id: AgentId,
        channel: &str,
        recipient: &str,
        success: bool,
        error: Option<&str>,
        thread_id: Option<&str>,
    ) {
        let receipt = if success {
            openfang_kernel::DeliveryTracker::sent_receipt(channel, recipient)
        } else {
            openfang_kernel::DeliveryTracker::failed_receipt(
                channel,
                recipient,
                error.unwrap_or("Unknown error"),
            )
        };
        self.kernel.delivery_tracker.record(agent_id, receipt);

        // Persist last channel for cron CronDelivery::LastChannel.
        // Include thread_id when present so forum-topic context survives restarts.
        if success {
            let mut kv_val = serde_json::json!({"channel": channel, "recipient": recipient});
            if let Some(tid) = thread_id {
                kv_val["thread_id"] = serde_json::json!(tid);
            }
            let _ = self
                .kernel
                .memory
                .structured_set(agent_id, "delivery.last_channel", kv_val);
        }
    }

    async fn check_auto_reply(&self, agent_id: AgentId, message: &str) -> Option<String> {
        // Check if auto-reply should fire for this message
        let channel_type = "bridge"; // Generic; the bridge layer handles specifics
        self.kernel
            .auto_reply_engine
            .should_reply(message, channel_type, agent_id)?;
        // Fire auto-reply synchronously (bridge already runs in background task)
        match self.kernel.send_message(agent_id, message).await {
            Ok(result) => Some(result.response),
            Err(e) => {
                tracing::warn!(error = %e, "Auto-reply failed");
                None
            }
        }
    }

    async fn resolve_mattermost_project_route(
        &self,
        mattermost_channel_id: &str,
    ) -> Option<(AgentId, String)> {
        self.kernel
            .project_store
            .mattermost_project_route(mattermost_channel_id)
            .map(|(aid, pid)| (aid, pid.to_string()))
    }

    // ── Budget, Network, A2A ──

    async fn budget_text(&self) -> String {
        let budget = &self.kernel.config.budget;
        let status = self.kernel.metering.budget_status(budget);

        let fmt_limit = |v: f64| -> String {
            if v > 0.0 {
                format!("${v:.2}")
            } else {
                "unlimited".to_string()
            }
        };
        let fmt_pct = |pct: f64, limit: f64| -> String {
            if limit > 0.0 {
                format!(" ({:.1}%)", pct * 100.0)
            } else {
                String::new()
            }
        };

        format!(
            "Budget Status:\n\
             \n\
             Hourly:  ${:.4} / {}{}\n\
             Daily:   ${:.4} / {}{}\n\
             Monthly: ${:.4} / {}{}\n\
             \n\
             Alert threshold: {}%",
            status.hourly_spend,
            fmt_limit(status.hourly_limit),
            fmt_pct(status.hourly_pct, status.hourly_limit),
            status.daily_spend,
            fmt_limit(status.daily_limit),
            fmt_pct(status.daily_pct, status.daily_limit),
            status.monthly_spend,
            fmt_limit(status.monthly_limit),
            fmt_pct(status.monthly_pct, status.monthly_limit),
            (status.alert_threshold * 100.0) as u32,
        )
    }

    async fn peers_text(&self) -> String {
        if !self.kernel.config.network_enabled {
            return "OFP peer network is disabled. Set network_enabled = true in config.toml."
                .to_string();
        }
        match self.kernel.peer_registry.get() {
            Some(registry) => {
                let peers = registry.all_peers();
                if peers.is_empty() {
                    "OFP network enabled but no peers connected.".to_string()
                } else {
                    let mut msg = format!("OFP Peers ({} connected):\n", peers.len());
                    for p in &peers {
                        msg.push_str(&format!(
                            "  {} — {} ({:?})\n",
                            p.node_id, p.address, p.state
                        ));
                    }
                    msg
                }
            }
            None => "OFP peer node not started.".to_string(),
        }
    }

    async fn a2a_agents_text(&self) -> String {
        let agents = self
            .kernel
            .a2a_external_agents
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if agents.is_empty() {
            return "No external A2A agents discovered.\nUse the dashboard or API to discover agents.".to_string();
        }
        let mut msg = format!("External A2A Agents ({}):\n", agents.len());
        for (url, card) in agents.iter() {
            msg.push_str(&format!("  {} — {}\n", card.name, url));
            let desc = &card.description;
            if !desc.is_empty() {
                let short = openfang_types::truncate_str(desc, 60);
                msg.push_str(&format!("    {short}\n"));
            }
        }
        msg
    }
}

/// Parse a trigger pattern string from chat into a `TriggerPattern`.
fn parse_trigger_pattern(s: &str) -> Option<openfang_kernel::triggers::TriggerPattern> {
    use openfang_kernel::triggers::TriggerPattern;
    if let Some(rest) = s.strip_prefix("spawned:") {
        return Some(TriggerPattern::AgentSpawned {
            name_pattern: rest.to_string(),
        });
    }
    if let Some(rest) = s.strip_prefix("system:") {
        return Some(TriggerPattern::SystemKeyword {
            keyword: rest.to_string(),
        });
    }
    if let Some(rest) = s.strip_prefix("memory:") {
        return Some(TriggerPattern::MemoryKeyPattern {
            key_pattern: rest.to_string(),
        });
    }
    if let Some(rest) = s.strip_prefix("match:") {
        return Some(TriggerPattern::ContentMatch {
            substring: rest.to_string(),
        });
    }
    match s {
        "lifecycle" => Some(TriggerPattern::Lifecycle),
        "terminated" => Some(TriggerPattern::AgentTerminated),
        "system" => Some(TriggerPattern::System),
        "memory" => Some(TriggerPattern::MemoryUpdate),
        "all" => Some(TriggerPattern::All),
        _ => None,
    }
}

/// Resolve a token: if the value looks like an actual secret (contains `:`,
/// starts with `xoxb-`, `xapp-`, `sk-`, etc.), use it directly.
/// Otherwise treat it as an env var name and look it up.
fn read_token(env_var_or_token: &str, adapter_name: &str) -> Option<String> {
    // Heuristic: Mattermost-style tokens are long opaque strings; placeholders are short.
    // known prefixes. Env var names are uppercase ASCII identifiers.
    let looks_like_token = env_var_or_token.contains(':')
        || env_var_or_token.starts_with("xoxb-")
        || env_var_or_token.starts_with("xapp-")
        || env_var_or_token.starts_with("sk-")
        || env_var_or_token.starts_with("Bearer ")
        || env_var_or_token.len() > 80; // Long random strings are tokens, not env var names

    if looks_like_token {
        warn!(
            "{adapter_name}: config field contains what looks like an actual token \
             rather than an env var name — using it directly. \
             Tip: store the token in an env var and use the var name instead for security."
        );
        return Some(env_var_or_token.to_string());
    }

    match std::env::var(env_var_or_token) {
        Ok(t) if !t.is_empty() => Some(t),
        Ok(_) => {
            warn!("{adapter_name} token env var '{env_var_or_token}' is set but empty, skipping");
            None
        }
        Err(_) => {
            warn!(
                "{adapter_name} token env var '{env_var_or_token}' not set, skipping. \
                 Set it with: export {env_var_or_token}=<your-token>"
            );
            None
        }
    }
}

/// Start the channel bridge for all configured channels based on kernel config.
///
/// Returns `Some(BridgeManager)` if any channels were configured and started,
/// or `None` if no channels are configured.
pub async fn start_channel_bridge(kernel: Arc<OpenFangKernel>) -> Option<BridgeManager> {
    let channels = kernel.config.channels.clone();
    let (bridge, _names) = start_channel_bridge_with_config(kernel, &channels).await;
    bridge
}

/// Start channels from an explicit `ChannelsConfig` (used by hot-reload).
///
/// Returns `(Option<BridgeManager>, Vec<started_channel_names>)`.
pub async fn start_channel_bridge_with_config(
    kernel: Arc<OpenFangKernel>,
    config: &openfang_types::config::ChannelsConfig,
) -> (Option<BridgeManager>, Vec<String>) {
    let has_any = config.signal.is_some() || config.mattermost.is_some();

    if !has_any {
        return (None, Vec::new());
    }

    let handle = KernelBridgeAdapter {
        kernel: kernel.clone(),
        started_at: Instant::now(),
    };

    // Collect all adapters to start
    let mut adapters: Vec<(Arc<dyn ChannelAdapter>, Option<String>)> = Vec::new();

    // Signal
    if let Some(ref sig_config) = config.signal {
        if !sig_config.phone_number.is_empty() {
            let adapter = Arc::new(SignalAdapter::new(
                sig_config.api_url.clone(),
                sig_config.phone_number.clone(),
                sig_config.allowed_users.clone(),
            ));
            adapters.push((adapter, sig_config.default_agent.clone()));
        } else {
            warn!("Signal configured but phone_number is empty, skipping");
        }
    }

    // Mattermost
    if let Some(ref mm_config) = config.mattermost {
        if let Some(token) = read_token(&mm_config.token_env, "Mattermost") {
            let adapter = Arc::new(MattermostAdapter::new(
                mm_config.server_url.clone(),
                token,
                mm_config.allowed_channels.clone(),
            ));
            adapters.push((adapter, mm_config.default_agent.clone()));
        }
    }

    if adapters.is_empty() {
        return (None, Vec::new());
    }

    // Resolve per-channel default agents AND set the first one as system-wide fallback
    let mut router = AgentRouter::new();
    let mut system_default_set = false;
    for (adapter, default_agent) in &adapters {
        if let Some(ref name) = default_agent {
            // Resolve agent name to ID
            let agent_id = match handle.find_agent_by_name(name).await {
                Ok(Some(id)) => Some(id),
                _ => match handle.spawn_agent_by_name(name).await {
                    Ok(id) => Some(id),
                    Err(e) => {
                        warn!(
                            "{}: could not find or spawn default agent '{}': {e}",
                            adapter.name(),
                            name
                        );
                        None
                    }
                },
            };
            if let Some(agent_id) = agent_id {
                // Register per-channel default
                let channel_key = format!("{:?}", adapter.channel_type());
                info!(
                    "{} default agent: {name} ({agent_id}) [channel: {channel_key}]",
                    adapter.name()
                );
                router.set_channel_default_with_name(channel_key, agent_id, name.clone());
                // First configured default also becomes system-wide fallback
                if !system_default_set {
                    router.set_default(agent_id);
                    system_default_set = true;
                }
            }
        }
    }

    // Load bindings and broadcast config from kernel
    let bindings = kernel.list_bindings();
    if !bindings.is_empty() {
        // Register all known agents in the router's name cache for binding resolution
        for entry in kernel.registry.list() {
            router.register_agent(entry.name.clone(), entry.id);
        }
        router.load_bindings(&bindings);
        info!(count = bindings.len(), "Loaded agent bindings into router");
    }
    router.load_broadcast(kernel.broadcast.clone());

    let bridge_handle: Arc<dyn ChannelBridgeHandle> = Arc::new(KernelBridgeAdapter {
        kernel: kernel.clone(),
        started_at: Instant::now(),
    });
    let router = Arc::new(router);
    let mut manager = BridgeManager::new(bridge_handle, router);

    let mut started_names = Vec::new();
    for (adapter, _) in adapters {
        let name = adapter.name().to_string();
        // Register adapter in kernel so agents can use `channel_send` tool
        kernel
            .channel_adapters
            .insert(name.clone(), adapter.clone());
        match manager.start_adapter(adapter).await {
            Ok(()) => {
                info!("{name} channel bridge started");
                started_names.push(name);
            }
            Err(e) => {
                // Remove from kernel map if start failed
                kernel.channel_adapters.remove(&name);
                error!("Failed to start {name} bridge: {e}");
            }
        }
    }

    if started_names.is_empty() {
        (None, Vec::new())
    } else {
        (Some(manager), started_names)
    }
}

/// Reload channels from disk config — stops old bridge, starts new one.
///
/// Reads `config.toml` fresh, rebuilds the channel bridge, and stores it
/// in `AppState.bridge_manager`. Returns the list of started channel names.
pub async fn reload_channels_from_disk(
    state: &crate::routes::AppState,
) -> Result<Vec<String>, String> {
    // Stop existing bridge
    {
        let mut guard = state.bridge_manager.lock().await;
        if let Some(ref mut bridge) = *guard {
            bridge.stop().await;
        }
        *guard = None;
    }

    // Re-read secrets.env so new API tokens are available in std::env
    let secrets_path = state.kernel.config.home_dir.join("secrets.env");
    if secrets_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&secrets_path) {
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }
                if let Some(eq_pos) = trimmed.find('=') {
                    let key = trimmed[..eq_pos].trim();
                    let mut value = trimmed[eq_pos + 1..].trim().to_string();
                    if !key.is_empty() {
                        // Strip matching quotes
                        if ((value.starts_with('"') && value.ends_with('"'))
                            || (value.starts_with('\'') && value.ends_with('\'')))
                            && value.len() >= 2
                        {
                            value = value[1..value.len() - 1].to_string();
                        }
                        // Always overwrite — the file is the source of truth after dashboard edits
                        std::env::set_var(key, &value);
                    }
                }
            }
            info!("Reloaded secrets.env for channel hot-reload");
        }
    }

    // Re-read config from disk
    let config_path = state.kernel.config.home_dir.join("config.toml");
    let fresh_config = openfang_kernel::config::load_config(Some(&config_path));

    // Update the live channels config so list_channels() reflects reality
    *state.channels_config.write().await = fresh_config.channels.clone();

    // Start new bridge with fresh channel config
    let (new_bridge, started) =
        start_channel_bridge_with_config(state.kernel.clone(), &fresh_config.channels).await;

    // Store the new bridge
    *state.bridge_manager.lock().await = new_bridge;

    info!(
        started = started.len(),
        channels = ?started,
        "Channel hot-reload complete"
    );

    Ok(started)
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_bridge_skips_when_no_config() {
        let config = openfang_types::config::KernelConfig::default();
        assert!(config.channels.signal.is_none());
        assert!(config.channels.mattermost.is_none());
    }
}
