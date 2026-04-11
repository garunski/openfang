//! Real HTTP integration tests for the OpenFang API.
//!
//! These tests boot a real kernel, start a real axum HTTP server on a random
//! port, and hit actual endpoints with reqwest.  No mocking.
//!
//! Tests that require an LLM API call are gated behind GROQ_API_KEY.
//!
//! Run: cargo test -p openfang-api --test api_integration_test -- --nocapture

use axum::Router;
use openfang_api::middleware;
use openfang_api::routes::{self, AppState};
use openfang_api::ws;
use openfang_kernel::{BacklogWatcherManager, OpenFangKernel};
use openfang_runtime::pipeline_audit;
use openfang_types::agent::AgentId;
use openfang_types::config::{DefaultModelConfig, KernelConfig};
use openfang_types::project::ProjectId;
use std::process::Command;
use std::sync::Arc;
use std::time::Instant;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Test infrastructure
// ---------------------------------------------------------------------------

struct TestServer {
    base_url: String,
    state: Arc<AppState>,
    _tmp: tempfile::TempDir,
}

/// `<project_root>/admin/backlog` — layout for integration tests with admin spoke.
fn test_admin_backlog(project_root: &std::path::Path) -> std::path::PathBuf {
    project_root.join("admin").join("backlog")
}

fn project_json_with_admin_spoke(name: &str, path_str: &str) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "path": path_str,
        "spokes": [{"name": "admin", "path": "admin", "labels": []}],
        "admin_spoke": "admin",
    })
}

fn test_backlog_ws_deps() -> (
    tokio::sync::broadcast::Sender<String>,
    Arc<BacklogWatcherManager>,
) {
    let (tx, _) = tokio::sync::broadcast::channel::<String>(32);
    let t2 = tx.clone();
    let sink: Arc<dyn Fn(ProjectId, &'static str) + Send + Sync> = Arc::new(move |pid, ent| {
        let _ = t2.send(
            serde_json::json!({
                "type": "backlog-updated",
                "project_id": pid.to_string(),
                "entity_type": ent,
            })
            .to_string(),
        );
    });
    (tx, Arc::new(BacklogWatcherManager::new(sink)))
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.state.backlog_watcher.stop_all();
        self.state.kernel.shutdown();
    }
}

/// Start a test server using ollama as default provider (no API key needed).
/// This lets the kernel boot without any real LLM credentials.
/// Tests that need actual LLM calls should use `start_test_server_with_llm()`.
async fn start_test_server() -> TestServer {
    start_test_server_with_provider("ollama", "test-model", "OLLAMA_API_KEY").await
}

/// Start a test server with Groq as the LLM provider (requires GROQ_API_KEY).
async fn start_test_server_with_llm() -> TestServer {
    start_test_server_with_provider("groq", "llama-3.3-70b-versatile", "GROQ_API_KEY").await
}

async fn start_test_server_with_provider(
    provider: &str,
    model: &str,
    api_key_env: &str,
) -> TestServer {
    let tmp = tempfile::tempdir().expect("Failed to create temp dir");

    let config = KernelConfig {
        home_dir: tmp.path().to_path_buf(),
        data_dir: tmp.path().join("data"),
        default_model: DefaultModelConfig {
            provider: provider.to_string(),
            model: model.to_string(),
            api_key_env: api_key_env.to_string(),
            base_url: None,
        },
        ..KernelConfig::default()
    };

    let kernel = OpenFangKernel::boot_with_config(config).expect("Kernel should boot");
    let kernel = Arc::new(kernel);
    kernel.set_self_handle();
    let backlog_store = kernel.backlog_store.clone();
    let (backlog_feed_tx, backlog_watcher) = test_backlog_ws_deps();

    let state = Arc::new(AppState {
        kernel,
        started_at: Instant::now(),
        peer_registry: None,
        bridge_manager: tokio::sync::Mutex::new(None),
        channels_config: tokio::sync::RwLock::new(Default::default()),
        shutdown_notify: Arc::new(tokio::sync::Notify::new()),
        clawhub_cache: dashmap::DashMap::new(),
        provider_probe_cache: openfang_runtime::provider_health::ProbeCache::new(),
        budget_config: Arc::new(tokio::sync::RwLock::new(Default::default())),
        backlog_store,
        backlog_feed_tx,
        backlog_watcher,
    });

    let app = Router::new()
        .route("/api/health", axum::routing::get(routes::health))
        .route("/api/status", axum::routing::get(routes::status))
        .route(
            "/api/agents",
            axum::routing::get(routes::list_agents).post(routes::spawn_agent),
        )
        .route(
            "/api/agents/{id}/message",
            axum::routing::post(routes::send_message),
        )
        .route(
            "/api/agents/{id}/session",
            axum::routing::get(routes::get_agent_session),
        )
        .route("/api/agents/{id}/ws", axum::routing::get(ws::agent_ws))
        .route(
            "/api/agents/{id}",
            axum::routing::delete(routes::kill_agent),
        )
        .route(
            "/api/triggers",
            axum::routing::get(routes::list_triggers).post(routes::create_trigger),
        )
        .route(
            "/api/triggers/{id}",
            axum::routing::delete(routes::delete_trigger),
        )
        .route(
            "/api/conduits",
            axum::routing::get(routes::list_conduits).post(routes::create_conduit),
        )
        .route(
            "/api/conduits/{id}/run",
            axum::routing::post(routes::run_conduit),
        )
        .route(
            "/api/conduits/{id}/runs/{run_id}",
            axum::routing::get(routes::get_conduit_run),
        )
        .route(
            "/api/conduits/{id}/runs",
            axum::routing::get(routes::list_conduit_runs),
        )
        .route(
            "/api/projects",
            axum::routing::get(routes::list_projects).post(routes::create_project),
        )
        .route(
            "/api/projects/{id}",
            axum::routing::get(routes::get_project)
                .put(routes::update_project)
                .delete(routes::delete_project),
        )
        .route(
            "/api/projects/{id}/mattermost/test-message",
            axum::routing::post(routes::post_project_mattermost_test_message),
        )
        .route(
            "/api/projects/{id}/discover",
            axum::routing::post(routes::discover_project_spokes),
        )
        .route(
            "/api/projects/{id}/tasks/{task_id}",
            axum::routing::get(routes::get_project_task),
        )
        .route(
            "/api/projects/{id}/tasks",
            axum::routing::get(routes::list_project_tasks),
        )
        .route(
            "/api/projects/{id}/docs",
            axum::routing::get(routes::list_project_docs),
        )
        .route(
            "/api/projects/{id}/agents",
            axum::routing::get(routes::list_project_agents).post(routes::bind_project_agent),
        )
        .route(
            "/api/projects/{id}/agents/management",
            axum::routing::get(routes::list_project_agents_management),
        )
        .route(
            "/api/projects/{id}/agents/{agent_id}",
            axum::routing::delete(routes::unbind_project_agent),
        )
        .route(
            "/api/projects/{id}/orchestrator/start",
            axum::routing::post(routes::start_project_orchestrator),
        )
        .route(
            "/api/projects/{id}/conduits/{conduit_id}/run",
            axum::routing::post(routes::run_project_conduit),
        )
        .route(
            "/api/projects/{id}/spokes",
            axum::routing::get(routes::list_project_spokes),
        )
        .route(
            "/api/projects/{id}/spokes/admin",
            axum::routing::put(routes::set_project_admin_spoke),
        )
        .route(
            "/api/projects/{id}/spokes/{spoke}/git/status",
            axum::routing::get(routes::get_project_spoke_git_status),
        )
        .route(
            "/api/projects/{id}/spokes/{spoke}/git/diff",
            axum::routing::get(routes::get_project_spoke_git_diff),
        )
        .route(
            "/api/projects/{id}/spokes/{spoke}/git/branches",
            axum::routing::get(routes::get_project_spoke_git_branches),
        )
        .route(
            "/api/projects/{id}/spokes/{spoke}/git/log",
            axum::routing::get(routes::get_project_spoke_git_log),
        )
        .route(
            "/api/projects/{id}/spokes/{spoke}/git/commit/{sha}/diff",
            axum::routing::get(routes::get_project_spoke_git_commit_diff),
        )
        .route(
            "/api/projects/{id}/spokes/{spoke}/git/stage",
            axum::routing::post(routes::post_project_spoke_git_stage),
        )
        .route(
            "/api/projects/{id}/spokes/{spoke}/git/unstage",
            axum::routing::post(routes::post_project_spoke_git_unstage),
        )
        .route(
            "/api/projects/{id}/spokes/{spoke}/git/stage-all",
            axum::routing::post(routes::post_project_spoke_git_stage_all),
        )
        .route(
            "/api/projects/{id}/spokes/{spoke}/git/commit",
            axum::routing::post(routes::post_project_spoke_git_commit),
        )
        .route(
            "/api/projects/{id}/spokes/{spoke}/git/checkout",
            axum::routing::post(routes::post_project_spoke_git_checkout),
        )
        .route(
            "/api/projects/{id}/spokes/{spoke}/git/push",
            axum::routing::post(routes::post_project_spoke_git_push),
        )
        .route(
            "/api/projects/{id}/spokes/{spoke}",
            axum::routing::get(routes::get_project_spoke_detail),
        )
        .route(
            "/api/projects/{id}/conduits",
            axum::routing::get(routes::list_project_conduits),
        )
        .route(
            "/api/projects/{id}/conduit-runs",
            axum::routing::get(routes::list_project_conduit_runs),
        )
        .route(
            "/api/projects/{id}/conduit-runs/{run_id}/trace",
            axum::routing::get(routes::project_conduit_run_trace),
        )
        .route(
            "/api/projects/{id}/backlog/config",
            axum::routing::get(routes::backlog_get_config),
        )
        .route(
            "/api/projects/{id}/backlog/tasks/reorder",
            axum::routing::post(routes::backlog_reorder_tasks),
        )
        .route(
            "/api/projects/{id}/backlog/tasks/cleanup/execute",
            axum::routing::post(routes::backlog_cleanup_tasks_execute),
        )
        .route(
            "/api/projects/{id}/backlog/tasks/cleanup",
            axum::routing::get(routes::backlog_cleanup_tasks_preview),
        )
        .route(
            "/api/projects/{id}/backlog/tasks/{task_id}/complete",
            axum::routing::post(routes::backlog_complete_task),
        )
        .route(
            "/api/projects/{id}/backlog/tasks/{task_id}/conduit-start",
            axum::routing::post(routes::backlog_start_task_conduit),
        )
        .route(
            "/api/projects/{id}/backlog/tasks/{task_id}",
            axum::routing::get(routes::backlog_get_task)
                .put(routes::backlog_put_task)
                .delete(routes::backlog_delete_task),
        )
        .route(
            "/api/projects/{id}/backlog/tasks",
            axum::routing::get(routes::backlog_list_tasks).post(routes::backlog_create_task),
        )
        .route(
            "/api/projects/{id}/backlog/docs/{doc_id}",
            axum::routing::get(routes::backlog_get_doc)
                .put(routes::backlog_update_doc)
                .delete(routes::backlog_delete_doc),
        )
        .route(
            "/api/projects/{id}/backlog/docs",
            axum::routing::get(routes::backlog_list_docs_tree).post(routes::backlog_create_doc),
        )
        .route(
            "/api/projects/{id}/backlog/completed",
            axum::routing::get(routes::backlog_list_completed),
        )
        .route(
            "/api/projects/{id}/backlog/milestones/archived",
            axum::routing::get(routes::backlog_list_archived_milestones),
        )
        .route(
            "/api/projects/{id}/backlog/milestones/{milestone_id}/archive",
            axum::routing::post(routes::backlog_archive_milestone),
        )
        .route(
            "/api/projects/{id}/backlog/milestones/{milestone_id}",
            axum::routing::put(routes::backlog_update_milestone)
                .delete(routes::backlog_delete_milestone),
        )
        .route(
            "/api/projects/{id}/backlog/milestones",
            axum::routing::get(routes::backlog_list_milestones)
                .post(routes::backlog_create_milestone),
        )
        .route(
            "/api/projects/{id}/backlog/decisions/{decision_id}",
            axum::routing::get(routes::backlog_get_decision)
                .put(routes::backlog_update_decision)
                .delete(routes::backlog_delete_decision),
        )
        .route(
            "/api/projects/{id}/backlog/decisions",
            axum::routing::get(routes::backlog_list_decisions)
                .post(routes::backlog_create_decision),
        )
        .route(
            "/api/projects/{id}/backlog/search",
            axum::routing::get(routes::backlog_search),
        )
        .route(
            "/api/projects/{id}/backlog/overview",
            axum::routing::get(routes::backlog_overview),
        )
        .route(
            "/api/projects/{id}/backlog/statistics",
            axum::routing::get(routes::backlog_statistics),
        )
        .route("/api/logs/files", axum::routing::get(routes::logs_files_list))
        .route(
            "/api/logs/files/{key}",
            axum::routing::get(routes::logs_file_read),
        )
        .route("/api/shutdown", axum::routing::post(routes::shutdown))
        .layer(axum::middleware::from_fn(middleware::request_logging))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind test server");
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    TestServer {
        base_url: format!("http://{}", addr),
        state,
        _tmp: tmp,
    }
}

/// Manifest that uses ollama (no API key required, won't make real LLM calls).
const TEST_MANIFEST: &str = r#"
name = "test-agent"
version = "0.1.0"
description = "Integration test agent"
author = "test"
module = "builtin:chat"

[model]
provider = "ollama"
model = "test-model"
system_prompt = "You are a test agent. Reply concisely."

[capabilities]
tools = ["file_read"]
memory_read = ["*"]
memory_write = ["self.*"]
"#;

/// Manifest that uses Groq for real LLM tests.
const LLM_MANIFEST: &str = r#"
name = "test-agent"
version = "0.1.0"
description = "Integration test agent"
author = "test"
module = "builtin:chat"

[model]
provider = "groq"
model = "llama-3.3-70b-versatile"
system_prompt = "You are a test agent. Reply concisely."

[capabilities]
tools = ["file_read"]
memory_read = ["*"]
memory_write = ["self.*"]
"#;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_health_endpoint() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/api/health", server.base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);

    // Middleware injects x-request-id
    assert!(resp.headers().contains_key("x-request-id"));

    let body: serde_json::Value = resp.json().await.unwrap();
    // Public health endpoint returns minimal info (redacted for security)
    assert_eq!(body["status"], "ok");
    assert!(body["version"].is_string());
    // Detailed fields should NOT appear in public health endpoint
    assert!(body["database"].is_null());
    assert!(body["agent_count"].is_null());
}

#[tokio::test]
async fn test_logs_files_allowlist_list_and_read() {
    let server = start_test_server().await;
    let home = server.state.kernel.config.home_dir.clone();
    let log_dir = home.join("logs");
    std::fs::create_dir_all(&log_dir).unwrap();
    std::fs::write(log_dir.join("daemon.log"), b"alpha\nbeta\n").unwrap();

    let client = reqwest::Client::new();

    let list = client
        .get(format!("{}/api/logs/files", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(list.status(), 200);
    let j: serde_json::Value = list.json().await.unwrap();
    let files = j["files"].as_array().unwrap();
    let daemon = files
        .iter()
        .find(|f| f["key"] == "daemon")
        .expect("daemon key");
    assert!(daemon["exists"].as_bool().unwrap());
    assert_eq!(daemon["size_bytes"].as_u64().unwrap(), 11);

    let read = client
        .get(format!("{}/api/logs/files/daemon", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(read.status(), 200);
    let body: serde_json::Value = read.json().await.unwrap();
    assert_eq!(body["content"], "alpha\nbeta\n");
    assert_eq!(body["file_size"], 11);
    assert_eq!(body["truncated"], false);

    let unknown = client
        .get(format!("{}/api/logs/files/evil", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(unknown.status(), 404);
}

#[tokio::test]
async fn test_projects_crud_api() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();
    let home = server.state.kernel.config.home_dir.clone();
    let proj_root = home.join("myproject");
    std::fs::create_dir_all(test_admin_backlog(&proj_root)).unwrap();
    let spoke_dir = proj_root.join("svc-a");
    std::fs::create_dir_all(&spoke_dir).unwrap();
    assert!(
        Command::new("git")
            .arg("init")
            .current_dir(&spoke_dir)
            .status()
            .map(|s| s.success())
            .unwrap_or(false),
        "git init required for project discover test"
    );

    let resp = client
        .post(format!("{}/api/projects", server.base_url))
        .json(&serde_json::json!({
            "name": "demo",
            "path": proj_root.to_str().unwrap(),
            "spokes": [{"name": "manual", "path": "rel", "labels": ["l1"]}, {"name": "admin", "path": "admin", "labels": []}],
            "admin_spoke": "admin",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = resp.json().await.unwrap();
    let pid = body["project_id"].as_str().unwrap().to_string();
    assert_eq!(body["name"], "demo");
    assert_eq!(body["spokes"].as_array().unwrap().len(), 2);
    assert!(body["mattermost_channel_id"].is_null());
    assert!(body["mattermost_team_name"].is_null());
    assert!(body["mattermost_channel_name"].is_null());

    let resp = client
        .get(format!("{}/api/projects", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let list: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["spoke_count"], 2);

    let resp = client
        .get(format!("{}/api/projects/{}", server.base_url, pid))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let detail: serde_json::Value = resp.json().await.unwrap();
    assert!(detail["admin_backlog_root"]
        .as_str()
        .unwrap()
        .contains("backlog"));
    assert_eq!(detail["admin_spoke"], "admin");
    assert!(detail["mattermost_channel_id"].is_null());
    assert!(detail["mattermost_team_name"].is_null());
    assert!(detail["mattermost_channel_name"].is_null());

    let resp = client
        .post(format!(
            "{}/api/projects/{}/mattermost/test-message",
            server.base_url, pid
        ))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let err: serde_json::Value = resp.json().await.unwrap();
    assert!(err["error"].as_str().unwrap().contains("no Mattermost"));

    let spokes = detail["spokes"].as_array().unwrap();
    assert_eq!(spokes.len(), 2);
    assert!(!spokes[0]["path_resolved"].as_str().unwrap().is_empty());

    let resp = client
        .post(format!("{}/api/projects/{}/discover", server.base_url, pid))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    let resp = client
        .put(format!(
            "{}/api/projects/{}/spokes/admin",
            server.base_url, pid
        ))
        .json(&serde_json::json!({ "name": "svc-a" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    let resp = client
        .put(format!("{}/api/projects/{}", server.base_url, pid))
        .json(&serde_json::json!({ "admin_spoke": null }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let resp = client
        .post(format!("{}/api/projects/{}/discover", server.base_url, pid))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let disc: serde_json::Value = resp.json().await.unwrap();
    let discovered = disc["spokes"].as_array().unwrap();
    assert_eq!(discovered.len(), 1);
    assert_eq!(discovered[0]["name"], "svc-a");

    let resp = client
        .put(format!(
            "{}/api/projects/{}/spokes/admin",
            server.base_url, pid
        ))
        .json(&serde_json::json!({ "name": "svc-a" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let adm: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(adm["admin_spoke"], "svc-a");

    let resp = client
        .put(format!("{}/api/projects/{}", server.base_url, pid))
        .json(&serde_json::json!({ "name": "demo2" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let up: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(up["name"], "demo2");
    assert!(up["mattermost_channel_id"].is_null());

    let resp = client
        .put(format!("{}/api/projects/{}", server.base_url, pid))
        .json(&serde_json::json!({
            "mattermost_channel_id": null,
            "mattermost_team_name": null,
            "mattermost_channel_name": null
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let mm: serde_json::Value = resp.json().await.unwrap();
    assert!(mm["mattermost_channel_id"].is_null());
    assert!(mm["mattermost_team_name"].is_null());
    assert!(mm["mattermost_channel_name"].is_null());

    let resp = client
        .delete(format!("{}/api/projects/{}", server.base_url, pid))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let resp = client
        .get(format!("{}/api/projects/{}", server.base_url, pid))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);

    let resp = client
        .get(format!("{}/api/projects/not-a-uuid", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn test_project_spoke_detail_and_git_workspace() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();
    let home = server.state.kernel.config.home_dir.clone();
    let proj_root = home.join("spoke-git-proj");
    let admin_bl = test_admin_backlog(&proj_root);
    std::fs::create_dir_all(&admin_bl).unwrap();
    let plain = proj_root.join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    let gitted = proj_root.join("gitted");
    std::fs::create_dir_all(&gitted).unwrap();

    assert!(
        Command::new("git")
            .args(["init"])
            .current_dir(&gitted)
            .status()
            .map(|s| s.success())
            .unwrap_or(false),
        "git init required for spoke git integration test"
    );

    std::fs::write(gitted.join("tracked.txt"), "a\n").unwrap();
    Command::new("git")
        .args(["config", "user.email", "t@t"])
        .current_dir(&gitted)
        .status()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "t"])
        .current_dir(&gitted)
        .status()
        .unwrap();
    Command::new("git")
        .args(["add", "tracked.txt"])
        .current_dir(&gitted)
        .status()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(&gitted)
        .status()
        .unwrap();
    std::fs::write(gitted.join("wip.txt"), "wip\n").unwrap();

    let resp = client
        .post(format!("{}/api/projects", server.base_url))
        .json(&serde_json::json!({
            "name": "spoke-git",
            "path": proj_root.to_str().unwrap(),
            "spokes": [
                {"name": "admin", "path": "admin", "labels": []},
                {"name": "plain", "path": "plain", "labels": []},
                {"name": "gitted", "path": "gitted", "labels": []},
            ],
            "admin_spoke": "admin",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = resp.json().await.unwrap();
    let pid = body["project_id"].as_str().unwrap();

    let r = client
        .get(format!(
            "{}/api/projects/{}/spokes/nope",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 404);

    let r = client
        .get(format!(
            "{}/api/projects/{}/spokes/plain",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let d: serde_json::Value = r.json().await.unwrap();
    assert_eq!(d["name"], "plain");
    assert!(d["resolved_path"].as_str().unwrap().contains("plain"));

    let r = client
        .get(format!(
            "{}/api/projects/{}/spokes/plain/git/status",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let st: serde_json::Value = r.json().await.unwrap();
    assert_eq!(st["is_git_repo"], false);

    let r = client
        .get(format!(
            "{}/api/projects/{}/spokes/gitted/git/status",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let st: serde_json::Value = r.json().await.unwrap();
    assert_eq!(st["is_git_repo"], true);
    let files = st["status"]["files"].as_array().unwrap();
    assert!(!files.is_empty());

    let r = client
        .get(format!(
            "{}/api/projects/{}/spokes/gitted/git/diff",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let diff: serde_json::Value = r.json().await.unwrap();
    assert_eq!(diff["is_git_repo"], true);
    assert_eq!(diff["diff_truncated"], false);
    let u = diff["unified_diff"].as_str().unwrap();
    assert!(u.contains("wip.txt") || u.contains("wip"));

    let r = client
        .post(format!(
            "{}/api/projects/{}/spokes/gitted/git/stage",
            server.base_url, pid
        ))
        .json(&serde_json::json!({ "path": "wip.txt" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);

    let r = client
        .post(format!(
            "{}/api/projects/{}/spokes/gitted/git/unstage",
            server.base_url, pid
        ))
        .json(&serde_json::json!({ "path": "wip.txt" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let r = client
        .get(format!(
            "{}/api/projects/{}/spokes/gitted/git/status",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let st2: serde_json::Value = r.json().await.unwrap();
    let files2 = st2["status"]["files"].as_array().unwrap();
    let wip = files2
        .iter()
        .find(|f| f["path"].as_str() == Some("wip.txt"))
        .expect("wip.txt in status");
    assert_eq!(wip["staged"], false);
    assert_eq!(wip["untracked"], true);

    let r = client
        .post(format!(
            "{}/api/projects/{}/spokes/gitted/git/stage",
            server.base_url, pid
        ))
        .json(&serde_json::json!({ "path": "wip.txt" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);

    let r = client
        .post(format!(
            "{}/api/projects/{}/spokes/gitted/git/commit",
            server.base_url, pid
        ))
        .json(&serde_json::json!({ "message": "add wip" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);

    let r = client
        .post(format!(
            "{}/api/projects/{}/spokes/gitted/git/checkout",
            server.base_url, pid
        ))
        .json(&serde_json::json!({ "branch": "feat", "create": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);

    let r = client
        .get(format!(
            "{}/api/projects/{}/spokes/gitted/git/branches",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let br: serde_json::Value = r.json().await.unwrap();
    assert_eq!(br["is_git_repo"], true);
    let names: Vec<String> = br["branches"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|b| b["name"].as_str().map(String::from))
        .collect();
    assert!(names.iter().any(|n| n.contains("feat")));

    let r = client
        .get(format!(
            "{}/api/projects/{}/spokes/gitted/git/log",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let log: serde_json::Value = r.json().await.unwrap();
    assert_eq!(log["is_git_repo"], true);
    let commits = log["commits"].as_array().unwrap();
    assert!(!commits.is_empty());
    let oid = commits[0]["oid"].as_str().unwrap();
    assert!(oid.len() >= 4);

    let r = client
        .get(format!(
            "{}/api/projects/{}/spokes/gitted/git/commit/{}/diff",
            server.base_url, pid, oid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let cdiff: serde_json::Value = r.json().await.unwrap();
    assert_eq!(cdiff["is_git_repo"], true);
    assert!(cdiff["unified_diff"].as_str().is_some());
}

#[tokio::test]
async fn test_project_backlog_endpoints() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();
    let home = server.state.kernel.config.home_dir.clone();
    let proj = home.join("backlog-proj");
    let backlog = test_admin_backlog(&proj);
    let tasks_dir = backlog.join("tasks");
    let docs_dir = backlog.join("docs").join("guides");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    std::fs::create_dir_all(&docs_dir).unwrap();
    std::fs::write(
        tasks_dir.join("task-1 - Alpha.md"),
        "---\nid: TASK-1\ntitle: Alpha\nstatus: Open\npriority: high\nlabels:\n  - rust\ncreated_date: '2026-01-02'\n---\n\n## Description\n\n<!-- SECTION:DESCRIPTION:BEGIN -->\nHello backlog\n<!-- SECTION:DESCRIPTION:END -->\n\n<!-- AC:BEGIN -->\n- [ ] #1 step\n<!-- AC:END -->\n",
    )
    .unwrap();
    std::fs::write(
        docs_dir.join("doc-1 - Overview.md"),
        "---\nid: doc-1\ntitle: Overview\ntype: guide\ncreated_date: '2026-01-02'\n---\n\n# Hi\n",
    )
    .unwrap();

    let resp = client
        .post(format!("{}/api/projects", server.base_url))
        .json(&project_json_with_admin_spoke("bp", proj.to_str().unwrap()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = resp.json().await.unwrap();
    let pid = body["project_id"].as_str().unwrap();

    let resp = client
        .get(format!("{}/api/projects/{}/tasks", server.base_url, pid))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let task_list: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(task_list.len(), 1);
    assert_eq!(task_list[0]["id"], "TASK-1");
    assert_eq!(task_list[0]["title"], "Alpha");

    let resp = client
        .get(format!(
            "{}/api/projects/{}/tasks?status=Open",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.json::<Vec<serde_json::Value>>().await.unwrap().len(),
        1
    );

    let resp = client
        .get(format!(
            "{}/api/projects/{}/tasks/TASK-1",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let detail: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(detail["description"], "Hello backlog");
    assert_eq!(detail["acceptance_criteria"][0]["text"], "#1 step");

    let resp = client
        .get(format!("{}/api/projects/{}/docs", server.base_url, pid))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let docs: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(docs.len(), 1);
    assert!(docs[0]["path"].as_str().unwrap().contains("guides/"));

    let resp = client
        .get(format!(
            "{}/api/projects/{}/backlog/overview",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let ov: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(ov["activeTaskCount"], 1);
    assert_eq!(ov["completedTaskCount"], 0);
    assert_eq!(ov["documentCount"], 1);
    assert_eq!(ov["decisionCount"], 0);
    assert_eq!(ov["milestoneCount"], 0);
    assert_eq!(ov["taskStatistics"]["total"], 1);

    let resp = client
        .get(format!(
            "{}/api/projects/00000000-0000-0000-0000-000000000099/tasks",
            server.base_url
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
    let err: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(err["error"], "Project not found");

    let no_backlog = home.join("no-backlog-dir");
    std::fs::create_dir_all(&no_backlog).unwrap();
    let resp = client
        .post(format!("{}/api/projects", server.base_url))
        .json(&serde_json::json!({
            "name": "nb",
            "path": no_backlog.to_str().unwrap(),
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let nb: serde_json::Value = resp.json().await.unwrap();
    let nb_id = nb["project_id"].as_str().unwrap();
    let resp = client
        .get(format!("{}/api/projects/{}/tasks", server.base_url, nb_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let err = resp.json::<serde_json::Value>().await.unwrap();
    assert!(
        err["error"]
            .as_str()
            .unwrap()
            .contains("Admin spoke is required"),
        "{err:?}"
    );
}

#[tokio::test]
async fn test_backlog_store_task_api() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();
    let home = server.state.kernel.config.home_dir.clone();
    let proj = home.join("backlog-api-proj");
    let backlog = test_admin_backlog(&proj);
    let tasks_dir = backlog.join("tasks");
    let milestones_dir = backlog.join("milestones");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    std::fs::create_dir_all(&milestones_dir).unwrap();
    std::fs::write(
        milestones_dir.join("milestone-1 - ship.md"),
        "---\nid: MS-1\ntitle: Ship\n---\n\n## Description\n\nx\n",
    )
    .unwrap();
    std::fs::write(
        tasks_dir.join("task-1 - Alpha.md"),
        "---\nid: TASK-1\ntitle: Alpha\nstatus: Open\npriority: high\nlabels:\n  - rust\nassignee:\n  - alice\ncreated_date: '2026-01-02'\n---\n\n",
    )
    .unwrap();

    let resp = client
        .post(format!("{}/api/projects", server.base_url))
        .json(&project_json_with_admin_spoke(
            "bapi",
            proj.to_str().unwrap(),
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = resp.json().await.unwrap();
    let pid = body["project_id"].as_str().unwrap();

    let resp = client
        .get(format!(
            "{}/api/projects/{}/backlog/config",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let resp = client
        .get(format!(
            "{}/api/projects/{}/backlog/tasks",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let list: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(list.len(), 1);

    let resp = client
        .get(format!(
            "{}/api/projects/{}/backlog/tasks?priority=high&label=rust&assignee=alice&status=Open",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.json::<Vec<serde_json::Value>>().await.unwrap().len(),
        1
    );

    let resp = client
        .get(format!(
            "{}/api/projects/{}/backlog/tasks/TASK-1",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let resp = client
        .post(format!(
            "{}/api/projects/{}/backlog/tasks/TASK-1/conduit-start",
            server.base_url, pid
        ))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    let resp = client
        .post(format!(
            "{}/api/projects/{}/backlog/tasks",
            server.base_url, pid
        ))
        .json(&serde_json::json!({
            "title": "Beta",
            "status": "Open",
            "labels": ["api"],
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let created: serde_json::Value = resp.json().await.unwrap();
    let tid2 = created["id"].as_str().unwrap().to_string();
    assert!(tid2.starts_with("TASK-"));

    let resp = client
        .put(format!(
            "{}/api/projects/{}/backlog/tasks/TASK-1",
            server.base_url, pid
        ))
        .json(&serde_json::json!({ "status": "In Progress" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let up: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(up["status"], "In Progress");

    let resp = client
        .put(format!(
            "{}/api/projects/{}/backlog/tasks/TASK-1",
            server.base_url, pid
        ))
        .json(&serde_json::json!({ "milestone": "MS-1" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let m1: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(m1["milestone"], "MS-1");

    let resp = client
        .put(format!(
            "{}/api/projects/{}/backlog/tasks/TASK-1",
            server.base_url, pid
        ))
        .json(&serde_json::json!({ "milestone": "" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let m0: serde_json::Value = resp.json().await.unwrap();
    assert!(m0["milestone"].is_null());

    let resp = client
        .post(format!(
            "{}/api/projects/{}/backlog/tasks/reorder",
            server.base_url, pid
        ))
        .json(&serde_json::json!({
            "taskId": tid2,
            "targetStatus": "Open",
            "orderedTaskIds": [tid2, "TASK-1"],
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);

    let resp = client
        .delete(format!(
            "{}/api/projects/{}/backlog/tasks/{}",
            server.base_url, pid, tid2
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);

    let resp = client
        .post(format!(
            "{}/api/projects/{}/backlog/tasks/TASK-1/complete",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);

    let resp = client
        .get(format!(
            "{}/api/projects/{}/backlog/tasks/TASK-1",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);

    let resp = client
        .get(format!(
            "{}/api/projects/00000000-0000-0000-0000-000000000099/backlog/tasks",
            server.base_url
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_backlog_cleanup_done_tasks_api() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();
    let home = server.state.kernel.config.home_dir.clone();
    let proj = home.join("backlog-cleanup-proj");
    let backlog = test_admin_backlog(&proj);
    let tasks_dir = backlog.join("tasks");
    let completed_dir = backlog.join("completed");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    std::fs::create_dir_all(&completed_dir).unwrap();
    std::fs::write(
        tasks_dir.join("task-99 - old done.md"),
        "---\nid: TASK-OLD\ntitle: Stale done\nstatus: Done\ncreated_date: 2020-01-01\nupdated_date: 2020-01-02\n---\n\n",
    )
    .unwrap();
    std::fs::write(
        tasks_dir.join("task-98 - recent done.md"),
        "---\nid: TASK-NEW\ntitle: Recent done\nstatus: Done\ncreated_date: 2026-04-01\n---\n\n",
    )
    .unwrap();

    let resp = client
        .post(format!("{}/api/projects", server.base_url))
        .json(&project_json_with_admin_spoke(
            "bcln",
            proj.to_str().unwrap(),
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = resp.json().await.unwrap();
    let pid = body["project_id"].as_str().unwrap();

    let resp = client
        .get(format!(
            "{}/api/projects/{}/backlog/tasks/cleanup?age=30",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let prev: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(prev["count"], 1);
    assert_eq!(prev["tasks"][0]["id"], "TASK-OLD");

    let resp = client
        .post(format!(
            "{}/api/projects/{}/backlog/tasks/cleanup/execute",
            server.base_url, pid
        ))
        .json(&serde_json::json!({ "age": 30 }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let done: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(done["movedCount"], 1);
    assert_eq!(done["totalCount"], 1);

    let resp = client
        .get(format!(
            "{}/api/projects/{}/backlog/tasks",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let active: Vec<serde_json::Value> = resp.json().await.unwrap();
    let ids: Vec<&str> = active.iter().filter_map(|t| t["id"].as_str()).collect();
    assert!(ids.contains(&"TASK-NEW"));
    assert!(!ids.contains(&"TASK-OLD"));
}

#[tokio::test]
async fn test_backlog_toggle_ac_api() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();
    let home = server.state.kernel.config.home_dir.clone();
    let proj = home.join("backlog-toggle-ac-proj");
    let backlog = test_admin_backlog(&proj);
    let tasks_dir = backlog.join("tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    std::fs::write(
        tasks_dir.join("task-1 - Ac.md"),
        "---\nid: TASK-AC1\ntitle: AC toggle\nstatus: Open\ncreated_date: 2026-01-01\n---\n\n<!-- AC:BEGIN -->\n- [ ] One\n- [ ] Two\n<!-- AC:END -->\n",
    )
    .unwrap();

    let resp = client
        .post(format!("{}/api/projects", server.base_url))
        .json(&project_json_with_admin_spoke(
            "tac",
            proj.to_str().unwrap(),
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = resp.json().await.unwrap();
    let pid = body["project_id"].as_str().unwrap();

    let resp = client
        .put(format!(
            "{}/api/projects/{}/backlog/tasks/TASK-AC1",
            server.base_url, pid
        ))
        .json(&serde_json::json!({ "toggleAc": 0 }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let up: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(up["acceptanceCriteriaItems"][0]["checked"], true);
    assert_eq!(up["acceptanceCriteriaItems"][1]["checked"], false);
}

#[tokio::test]
async fn test_backlog_docs_api() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();
    let home = server.state.kernel.config.home_dir.clone();
    let proj = home.join("backlog-docs-proj");
    let docs = test_admin_backlog(&proj)
        .join("docs")
        .join("overview")
        .join("architecture");
    std::fs::create_dir_all(&docs).unwrap();
    std::fs::write(
        docs.join("doc-1.md"),
        "---\nid: doc-1\ntitle: Arch\ntype: guide\ncreated_date: 2026-01-01\n---\n\n# Body\n",
    )
    .unwrap();

    let resp = client
        .post(format!("{}/api/projects", server.base_url))
        .json(&project_json_with_admin_spoke(
            "docsproj",
            proj.to_str().unwrap(),
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = resp.json().await.unwrap();
    let pid = body["project_id"].as_str().unwrap();

    let resp = client
        .get(format!(
            "{}/api/projects/{}/backlog/docs",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let tree: serde_json::Value = resp.json().await.unwrap();
    let over = tree["children"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "overview")
        .unwrap();
    let arch = over["children"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "architecture")
        .unwrap();
    let d0 = &arch["docs"].as_array().unwrap()[0];
    assert_eq!(d0["id"], "doc-1");
    assert_eq!(d0["path"], "overview/architecture");

    let resp = client
        .get(format!(
            "{}/api/projects/{}/backlog/docs/doc-1",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let one: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(one["title"], "Arch");
    assert!(one["rawContent"].as_str().unwrap().contains("Body"));

    let resp = client
        .post(format!(
            "{}/api/projects/{}/backlog/docs",
            server.base_url, pid
        ))
        .json(&serde_json::json!({
            "title": "New page",
            "type": "reference",
            "categoryPath": "overview/architecture",
            "content": "# X\n",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let new_doc: serde_json::Value = resp.json().await.unwrap();
    let new_id = new_doc["id"].as_str().unwrap();

    let resp = client
        .delete(format!(
            "{}/api/projects/{}/backlog/docs/{}",
            server.base_url, pid, new_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);

    let resp = client
        .put(format!(
            "{}/api/projects/{}/backlog/docs/doc-1",
            server.base_url, pid
        ))
        .json(&serde_json::json!({ "title": "Arch2", "content": "# Z\n" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let up: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(up["title"], "Arch2");
    assert!(up["rawContent"].as_str().unwrap().contains('Z'));
}

#[tokio::test]
async fn test_backlog_decisions_milestones_completed_api() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();
    let home = server.state.kernel.config.home_dir.clone();
    let proj = home.join("backlog-ddmc-proj");
    let backlog = test_admin_backlog(&proj);
    let decisions = backlog.join("decisions");
    let milestones = backlog.join("milestones");
    let tasks = backlog.join("tasks");
    let completed = backlog.join("completed");
    std::fs::create_dir_all(&decisions).unwrap();
    std::fs::create_dir_all(&milestones).unwrap();
    std::fs::create_dir_all(&tasks).unwrap();
    std::fs::create_dir_all(&completed).unwrap();
    std::fs::write(
        decisions.join("decision-1 - existing.md"),
        r"---
id: DEC-1
title: Existing ADR
date: 2026-03-01
status: proposed
---

## Context

c

## Decision

d

## Consequences

e
",
    )
    .unwrap();
    std::fs::write(
        tasks.join("task-3 - extra.md"),
        "---\nid: TASK-3\ntitle: Extra open\nstatus: Open\ncreated_date: 2026-01-01\n---\n\n",
    )
    .unwrap();
    std::fs::write(
        milestones.join("milestone-1 - ship.md"),
        "---\nid: MS-1\ntitle: Ship\n---\n\n## Description\n\nShip it.\n",
    )
    .unwrap();
    std::fs::write(
        tasks.join("task-1 - open.md"),
        "---\nid: TASK-1\ntitle: Open\nstatus: Open\nmilestone: MS-1\ncreated_date: 2026-01-01\n---\n\n",
    )
    .unwrap();
    std::fs::write(
        completed.join("task-2 - done.md"),
        "---\nid: TASK-2\ntitle: Done\nstatus: Done\nmilestone: MS-1\ncreated_date: 2026-01-01\n---\n\n",
    )
    .unwrap();

    let resp = client
        .post(format!("{}/api/projects", server.base_url))
        .json(&project_json_with_admin_spoke(
            "ddmc",
            proj.to_str().unwrap(),
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = resp.json().await.unwrap();
    let pid = body["project_id"].as_str().unwrap();

    let resp = client
        .get(format!(
            "{}/api/projects/{}/backlog/decisions",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let decs: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(decs.len(), 1);
    assert_eq!(decs[0]["id"], "DEC-1");

    let resp = client
        .post(format!(
            "{}/api/projects/{}/backlog/decisions",
            server.base_url, pid
        ))
        .json(&serde_json::json!({ "title": "Second" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let created: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(created["id"], "DEC-2");
    assert_eq!(created["title"], "Second");

    let resp = client
        .put(format!(
            "{}/api/projects/{}/backlog/decisions/DEC-2",
            server.base_url, pid
        ))
        .json(&serde_json::json!({ "status": "accepted", "context": "ctx" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let upd: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(upd["status"], "accepted");
    assert_eq!(upd["context"], "ctx");

    let resp = client
        .delete(format!(
            "{}/api/projects/{}/backlog/decisions/DEC-2",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);
    let resp = client
        .get(format!(
            "{}/api/projects/{}/backlog/decisions",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let decs_after: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(decs_after.len(), 1);
    assert_eq!(decs_after[0]["id"], "DEC-1");

    let resp = client
        .get(format!(
            "{}/api/projects/{}/backlog/tasks",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let active: Vec<serde_json::Value> = resp.json().await.unwrap();
    let ids: Vec<&str> = active.iter().filter_map(|t| t["id"].as_str()).collect();
    assert!(ids.contains(&"TASK-1"));
    assert!(ids.contains(&"TASK-3"));

    let resp = client
        .get(format!(
            "{}/api/projects/{}/backlog/milestones",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let ms: Vec<serde_json::Value> = resp.json().await.unwrap();
    let m1 = ms.iter().find(|m| m["id"] == "MS-1").unwrap();
    assert_eq!(m1["taskTotal"], 2);
    assert_eq!(m1["taskDone"], 1);
    assert_eq!(m1["progressPercent"].as_f64().unwrap(), 50.0);

    let resp = client
        .post(format!(
            "{}/api/projects/{}/backlog/milestones",
            server.base_url, pid
        ))
        .json(&serde_json::json!({ "title": "Beta", "description": "d" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let m2: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(m2["id"], "MS-2");
    assert_eq!(m2["taskTotal"], 0);

    let resp = client
        .put(format!(
            "{}/api/projects/{}/backlog/milestones/MS-2",
            server.base_url, pid
        ))
        .json(&serde_json::json!({ "title": "Beta renamed", "description": "updated body" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let m2u: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(m2u["title"], "Beta renamed");
    assert_eq!(m2u["description"], "updated body");

    let resp = client
        .post(format!(
            "{}/api/projects/{}/backlog/milestones/MS-2/archive",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);

    let resp = client
        .get(format!(
            "{}/api/projects/{}/backlog/milestones",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert!(!resp
        .json::<Vec<serde_json::Value>>()
        .await
        .unwrap()
        .iter()
        .any(|m| m["id"] == "MS-2"));

    let resp = client
        .get(format!(
            "{}/api/projects/{}/backlog/milestones/archived",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let arch: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert!(arch.iter().any(|m| m["id"] == "MS-2"));

    let resp = client
        .delete(format!(
            "{}/api/projects/{}/backlog/milestones/MS-2",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);
    let resp = client
        .get(format!(
            "{}/api/projects/{}/backlog/milestones/archived",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let arch2: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert!(!arch2.iter().any(|m| m["id"] == "MS-2"));

    let resp = client
        .get(format!(
            "{}/api/projects/{}/backlog/completed",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let done: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(done.len(), 1);
    assert_eq!(done[0]["id"], "TASK-2");
}

#[tokio::test]
async fn test_backlog_search_and_statistics_api() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();
    let home = server.state.kernel.config.home_dir.clone();
    let proj = home.join("backlog-search-stat-proj");
    let backlog = test_admin_backlog(&proj);
    let tasks = backlog.join("tasks");
    let docs = backlog.join("docs").join("guides");
    let decisions = backlog.join("decisions");
    std::fs::create_dir_all(&tasks).unwrap();
    std::fs::create_dir_all(&docs).unwrap();
    std::fs::create_dir_all(&decisions).unwrap();
    std::fs::write(
        tasks.join("task-1 - hi.md"),
        "---\nid: TASK-S1\ntitle: Zebra stripe\nstatus: Open\npriority: high\nlabels:\n  - api\ncreated_date: 2026-01-01\n---\n\n## Description\n\n<!-- SECTION:DESCRIPTION:BEGIN -->\nneedleBodyS1\n<!-- SECTION:DESCRIPTION:END -->\n",
    )
    .unwrap();
    std::fs::write(
        tasks.join("task-2 - lo.md"),
        "---\nid: TASK-S2\ntitle: Low prio\nstatus: Done\npriority: low\ncreated_date: 2026-01-02\n---\n\n",
    )
    .unwrap();
    std::fs::write(
        docs.join("doc-s1.md"),
        "---\nid: doc-s1\ntitle: Guide\ntype: guide\ncreated_date: 2026-01-01\n---\n\n# needleDocBody\n",
    )
    .unwrap();
    std::fs::write(
        decisions.join("decision-1 - adr.md"),
        "---\nid: DEC-S1\ntitle: Pick X\ndate: 2026-03-01\nstatus: proposed\n---\n\n## Context\n\nneedleCtx\n\n## Decision\n\nGo\n\n## Consequences\n\nOk\n",
    )
    .unwrap();

    let resp = client
        .post(format!("{}/api/projects", server.base_url))
        .json(&project_json_with_admin_spoke(
            "searchstat",
            proj.to_str().unwrap(),
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = resp.json().await.unwrap();
    let pid = body["project_id"].as_str().unwrap();

    let resp = client
        .get(format!(
            "{}/api/projects/{}/backlog/statistics",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let st: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(st["total"], 2);
    assert_eq!(st["statusCounts"]["Open"], 1);
    assert_eq!(st["statusCounts"]["Done"], 1);
    assert_eq!(st["priorityCounts"]["high"], 1);
    assert_eq!(st["priorityCounts"]["low"], 1);

    let resp = client
        .get(format!(
            "{}/api/projects/{}/backlog/search?q=zebra&type=task",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let hits: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["type"], "task");
    assert_eq!(hits[0]["task"]["id"], "TASK-S1");
    assert!(hits[0]["score"].as_f64().unwrap() >= 0.8);

    let resp = client
        .get(format!(
            "{}/api/projects/{}/backlog/search?q=needleDocBody&type=document",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let hits: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["type"], "document");
    assert_eq!(hits[0]["document"]["id"], "doc-s1");

    let resp = client
        .get(format!(
            "{}/api/projects/{}/backlog/search?q=needleCtx&type=decision",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let hits: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["type"], "decision");
    assert_eq!(hits[0]["decision"]["id"], "DEC-S1");

    let resp = client
        .get(format!(
            "{}/api/projects/{}/backlog/search?type=task&priority=high",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let hits: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["task"]["id"], "TASK-S1");

    let resp = client
        .get(format!(
            "{}/api/projects/{}/backlog/search?q=needleBodyS1&type=task",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let hits: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["task"]["id"], "TASK-S1");
    assert_eq!(hits[0]["score"].as_f64().unwrap(), 0.5);
}

#[tokio::test]
async fn test_project_scoped_agents_spokes_workflows() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();
    let home = server.state.kernel.config.home_dir.clone();
    let proj_root = home.join("projscoped");
    let spoke = proj_root.join("spoke-int");
    std::fs::create_dir_all(&spoke).unwrap();
    assert!(
        Command::new("git")
            .arg("init")
            .current_dir(&spoke)
            .status()
            .map(|s| s.success())
            .unwrap_or(false),
        "git init required for scoped spokes test"
    );
    std::fs::write(spoke.join("mise.toml"), "[tools]\n").unwrap();
    let ab = test_admin_backlog(&proj_root);
    std::fs::create_dir_all(ab.join("tasks")).unwrap();
    std::fs::write(
        ab.join("tasks/task-77 - T.md"),
        "---\nid: TASK-INT-SCOPED-77\ntitle: T\nstatus: Open\n---\n",
    )
    .unwrap();

    let resp = client
        .post(format!("{}/api/projects", server.base_url))
        .json(&serde_json::json!({
            "name": "scoped",
            "path": proj_root.to_str().unwrap(),
            "spokes": [
                {"name": "mainspoke", "path": "spoke-int", "labels": []},
                {"name": "admin", "path": "admin", "labels": []},
            ],
            "admin_spoke": "admin",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = resp.json().await.unwrap();
    let pid = body["project_id"].as_str().unwrap();

    let resp = client
        .post(format!("{}/api/agents", server.base_url))
        .json(&serde_json::json!({"manifest_toml": TEST_MANIFEST}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    let ag_list = client
        .get(format!("{}/api/agents", server.base_url))
        .send()
        .await
        .unwrap();
    let agents: Vec<serde_json::Value> = ag_list.json().await.unwrap();
    let first_id = agents[0]["id"].as_str().unwrap();
    let aid: AgentId = first_id.parse().unwrap();
    server
        .state
        .kernel
        .registry
        .update_workspace(aid, Some(spoke.join("agent-ws")))
        .unwrap();
    std::fs::create_dir_all(spoke.join("agent-ws")).unwrap();

    let resp = client
        .get(format!(
            "{}/api/projects/{}/agents/management",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let mgmt: serde_json::Value = resp.json().await.unwrap();
    assert!(mgmt["current"].is_array());
    assert!(mgmt["historical"].is_array());

    let resp = client
        .post(format!(
            "{}/api/projects/{}/orchestrator/start",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let err_body: serde_json::Value = resp.json().await.unwrap();
    assert!(err_body["error"].as_str().unwrap().contains("Mattermost"));

    let resp = client
        .get(format!("{}/api/projects/{}/agents", server.base_url, pid))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let scoped: Vec<serde_json::Value> = resp.json().await.unwrap();
    let implicit_row = scoped
        .iter()
        .find(|a| a["spoke_name"] == "mainspoke")
        .expect("implicit workspace match");
    assert_eq!(implicit_row["binding"], "implicit");

    let resp = client
        .post(format!("{}/api/projects/{}/agents", server.base_url, pid))
        .json(&serde_json::json!({ "agent_id": first_id }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let bound: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(bound["binding"], "explicit");
    assert_eq!(bound["agent_id"], first_id);

    let resp = client
        .post(format!("{}/api/projects/{}/agents", server.base_url, pid))
        .json(&serde_json::json!({ "agent_id": first_id }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);

    let resp = client
        .post(format!("{}/api/projects/{}/agents", server.base_url, pid))
        .json(&serde_json::json!({ "agent_id": Uuid::nil().to_string() }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);

    let projects_json = std::fs::read_to_string(home.join("projects.json")).unwrap();
    assert!(
        projects_json.contains("bound_agents") && projects_json.contains(first_id),
        "{projects_json}"
    );

    let resp = client
        .get(format!("{}/api/projects/{}/agents", server.base_url, pid))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let scoped2: Vec<serde_json::Value> = resp.json().await.unwrap();
    let ex = scoped2
        .iter()
        .find(|a| a["agent_id"].as_str() == Some(first_id))
        .unwrap();
    assert_eq!(ex["binding"], "explicit");

    let resp = client
        .delete(format!(
            "{}/api/projects/{}/agents/{}",
            server.base_url, pid, first_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let resp = client
        .get(format!("{}/api/projects/{}/agents", server.base_url, pid))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let scoped3: Vec<serde_json::Value> = resp.json().await.unwrap();
    let im = scoped3
        .iter()
        .find(|a| a["agent_id"].as_str() == Some(first_id))
        .expect("still implicit via workspace");
    assert_eq!(im["binding"], "implicit");

    let resp = client
        .delete(format!(
            "{}/api/projects/{}/agents/{}",
            server.base_url, pid, first_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);

    let resp = client
        .get(format!("{}/api/projects/{}/spokes", server.base_url, pid))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let sp: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(sp.len(), 2);
    let mainspoke = sp
        .iter()
        .find(|r| r["name"] == "mainspoke")
        .expect("mainspoke row");
    assert_eq!(mainspoke["mise_toml_exists"], true);
    assert_eq!(mainspoke["is_git_repo"], true);

    pipeline_audit::clear_pipeline_audit_ring();
    pipeline_audit::emit(pipeline_audit::PipelineAuditEvent::QualityGate {
        timestamp: "2026-01-01T00:00:00Z".into(),
        task_id: "TASK-INT-SCOPED-77".into(),
        spoke_root: spoke.canonicalize().unwrap().to_string_lossy().into(),
        exit_code: 0,
        actor: "itest".into(),
        tool: "enforce_quality_gate".into(),
    });

    let resp = client
        .get(format!(
            "{}/api/projects/{}/conduits?limit=5",
            server.base_url, pid
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let pipes: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert!(
        pipes.iter().any(|p| p["task_id"] == "TASK-INT-SCOPED-77"),
        "{pipes:?}"
    );

    let resp = client
        .get(format!(
            "{}/api/projects/00000000-0000-0000-0000-000000000001/agents",
            server.base_url
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_status_endpoint() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/api/status", server.base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "running");
    assert_eq!(body["agent_count"], 0);
    assert!(body["uptime_seconds"].is_number());
    assert_eq!(body["default_provider"], "ollama");
    assert_eq!(body["agents"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_spawn_list_kill_agent() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    // --- Spawn ---
    let resp = client
        .post(format!("{}/api/agents", server.base_url))
        .json(&serde_json::json!({"manifest_toml": TEST_MANIFEST}))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["name"], "test-agent");
    let agent_id = body["agent_id"].as_str().unwrap().to_string();
    assert!(!agent_id.is_empty());

    // --- List (test-agent only) ---
    let resp = client
        .get(format!("{}/api/agents", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let agents: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(agents.len(), 1);
    let test_agent = agents.iter().find(|a| a["name"] == "test-agent").unwrap();
    assert_eq!(test_agent["id"], agent_id);
    assert_eq!(test_agent["model_provider"], "ollama");

    // --- Kill ---
    let resp = client
        .delete(format!("{}/api/agents/{}", server.base_url, agent_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "killed");

    // --- List (empty) ---
    let resp = client
        .get(format!("{}/api/agents", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let agents: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(agents.len(), 0);
}

#[tokio::test]
async fn test_agent_session_empty() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    // Spawn agent
    let resp = client
        .post(format!("{}/api/agents", server.base_url))
        .json(&serde_json::json!({"manifest_toml": TEST_MANIFEST}))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    let agent_id = body["agent_id"].as_str().unwrap();

    // Session should be empty — no messages sent yet
    let resp = client
        .get(format!(
            "{}/api/agents/{}/session",
            server.base_url, agent_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["message_count"], 0);
    assert_eq!(body["messages"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_send_message_with_llm() {
    if std::env::var("GROQ_API_KEY").is_err() {
        eprintln!("GROQ_API_KEY not set, skipping LLM integration test");
        return;
    }

    let server = start_test_server_with_llm().await;
    let client = reqwest::Client::new();

    // Spawn
    let resp = client
        .post(format!("{}/api/agents", server.base_url))
        .json(&serde_json::json!({"manifest_toml": LLM_MANIFEST}))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    let agent_id = body["agent_id"].as_str().unwrap().to_string();

    // Send message through the real HTTP endpoint → kernel → Groq LLM
    let resp = client
        .post(format!(
            "{}/api/agents/{}/message",
            server.base_url, agent_id
        ))
        .json(&serde_json::json!({"message": "Say hello in exactly 3 words."}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let response_text = body["response"].as_str().unwrap();
    assert!(
        !response_text.is_empty(),
        "LLM response should not be empty"
    );
    assert!(body["input_tokens"].as_u64().unwrap() > 0);
    assert!(body["output_tokens"].as_u64().unwrap() > 0);

    // Session should now have messages
    let resp = client
        .get(format!(
            "{}/api/agents/{}/session",
            server.base_url, agent_id
        ))
        .send()
        .await
        .unwrap();
    let session: serde_json::Value = resp.json().await.unwrap();
    assert!(session["message_count"].as_u64().unwrap() > 0);
}

#[tokio::test]
async fn test_workflow_crud() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    // Spawn agent for workflow
    let resp = client
        .post(format!("{}/api/agents", server.base_url))
        .json(&serde_json::json!({"manifest_toml": TEST_MANIFEST}))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    let agent_name = body["name"].as_str().unwrap().to_string();

    // Create workflow
    let resp = client
        .post(format!("{}/api/conduits", server.base_url))
        .json(&serde_json::json!({
            "name": "test-workflow",
            "description": "Integration test workflow",
            "steps": [
                {
                    "name": "step1",
                    "agent_name": agent_name,
                    "prompt": "Echo: {{input}}",
                    "mode": "sequential",
                    "timeout_secs": 30
                }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = resp.json().await.unwrap();
    let conduit_id = body["conduit_id"].as_str().unwrap().to_string();
    assert!(!conduit_id.is_empty());

    // List workflows
    let resp = client
        .get(format!("{}/api/conduits", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let workflows: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(workflows.len(), 1);
    assert_eq!(workflows[0]["name"], "test-workflow");
    assert_eq!(workflows[0]["steps"], 1);
    assert!(workflows[0]["project_id"].is_null());
}

#[tokio::test]
async fn test_project_workflow_requires_assigned_agents() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();
    let home = server.state.kernel.config.home_dir.clone();
    let proj_root = home.join("wfproj");
    std::fs::create_dir_all(test_admin_backlog(&proj_root)).unwrap();
    let spoke = proj_root.join("spoke1");
    std::fs::create_dir_all(&spoke).unwrap();

    let resp = client
        .post(format!("{}/api/projects", server.base_url))
        .json(&serde_json::json!({
            "name": "wfproj",
            "path": proj_root.to_str().unwrap(),
            "spokes": [
                {"name": "s1", "path": "spoke1", "labels": []},
                {"name": "admin", "path": "admin", "labels": []},
            ],
            "admin_spoke": "admin",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = resp.json().await.unwrap();
    let pid = body["project_id"].as_str().unwrap().to_string();

    let resp = client
        .post(format!("{}/api/agents", server.base_url))
        .json(&serde_json::json!({"manifest_toml": TEST_MANIFEST}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = resp.json().await.unwrap();
    let agent_id = body["agent_id"].as_str().unwrap().to_string();
    let agent_name = body["name"].as_str().unwrap().to_string();

    let resp = client
        .post(format!("{}/api/conduits", server.base_url))
        .json(&serde_json::json!({
            "name": "proj-wf",
            "description": "",
            "project_id": pid,
            "steps": [{
                "name": "s1",
                "agent_name": agent_name,
                "prompt": "{{input}}",
                "mode": "sequential",
                "timeout_secs": 5
            }]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = resp.json().await.unwrap();
    let wf_id = body["conduit_id"].as_str().unwrap().to_string();

    let resp = client
        .post(format!(
            "{}/api/projects/{}/conduits/{}/run",
            server.base_url, pid, wf_id
        ))
        .json(&serde_json::json!({"input": "hi"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let err: serde_json::Value = resp.json().await.unwrap();
    assert!(
        err["error"].as_str().unwrap().contains("not assigned"),
        "{}",
        err["error"].as_str().unwrap()
    );

    let resp = client
        .post(format!("{}/api/projects/{}/agents", server.base_url, pid))
        .json(&serde_json::json!({"agent_id": agent_id}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let resp = client
        .post(format!(
            "{}/api/projects/{}/conduits/{}/run",
            server.base_url, pid, wf_id
        ))
        .json(&serde_json::json!({"input": "hi"}))
        .send()
        .await
        .unwrap();
    assert_ne!(
        resp.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "validation should pass after bind"
    );
    let run_status = resp.status();
    let run_body: serde_json::Value = resp.json().await.unwrap();
    let run_id = if run_status == reqwest::StatusCode::OK {
        run_body["run_id"]
            .as_str()
            .expect("run_id in success body")
            .to_string()
    } else {
        let resp = client
            .get(format!(
                "{}/api/projects/{}/conduit-runs?limit=10",
                server.base_url, pid
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let list: Vec<serde_json::Value> = resp.json().await.unwrap();
        list.iter()
            .find(|r| r["conduit_id"].as_str() == Some(wf_id.as_str()))
            .and_then(|r| r["id"].as_str())
            .expect("expected a project conduit run row after POST /run")
            .to_string()
    };

    let resp = client
        .get(format!(
            "{}/api/projects/{}/conduit-runs/{}/trace",
            server.base_url, pid, run_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let trace: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(trace["run_id"], run_id);
    let content = trace["content"].as_str().unwrap_or("");
    assert!(
        content.contains("run_id=") && content.contains(&run_id),
        "expected trace content to mention run_id, got: {content:?}"
    );
}

/// TASK-46: `conduit-full-cycle` template registers via POST /api/conduits and runs via project route.
#[tokio::test]
async fn test_workflow_full_cycle_workflow_registers_and_project_run() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();
    let home = server.state.kernel.config.home_dir.clone();
    let proj_root = home.join("pfcycle");
    std::fs::create_dir_all(test_admin_backlog(&proj_root)).unwrap();
    let spoke = proj_root.join("code-spoke");
    std::fs::create_dir_all(&spoke).unwrap();

    let resp = client
        .post(format!("{}/api/projects", server.base_url))
        .json(&serde_json::json!({
            "name": "pfcycle",
            "path": proj_root.to_str().unwrap(),
            "spokes": [
                {"name": "code-spoke", "path": "code-spoke", "labels": ["repo:dev"]},
                {"name": "admin", "path": "admin", "labels": []},
            ],
            "admin_spoke": "admin",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = resp.json().await.unwrap();
    let pid = body["project_id"].as_str().unwrap();

    const COORD_MANIFEST: &str = r#"
name = "conduit-coordinator-hand"
version = "0.1.0"
description = "Test stand-in for workflow coordinator workflow steps"
author = "test"
module = "builtin:chat"

[model]
provider = "ollama"
model = "test-model"
system_prompt = "You are a test workflow orchestrator. Reply briefly."

[capabilities]
tools = ["file_read", "backlog_task_view", "query_project_status", "read_project_context", "resolve_conduit_context", "update_project_context", "backlog_task_edit", "trigger_cursor_worker", "enforce_quality_gate", "record_git_action", "git_create_branch", "git_commit_and_push", "git_create_pr"]
memory_read = ["*"]
memory_write = ["self.*"]
"#;

    let resp = client
        .post(format!("{}/api/agents", server.base_url))
        .json(&serde_json::json!({"manifest_toml": COORD_MANIFEST}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = resp.json().await.unwrap();
    let agent_id = body["agent_id"].as_str().unwrap();

    let wf_template: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../openfang-kernel/bundled/conduits/conduit-full-cycle.json"
    )))
    .unwrap();

    let resp = client
        .post(format!("{}/api/conduits", server.base_url))
        .json(&wf_template)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let wf_body: serde_json::Value = resp.json().await.unwrap();
    let wf_id = wf_body["conduit_id"].as_str().unwrap();

    let resp = client
        .post(format!("{}/api/projects/{}/agents", server.base_url, pid))
        .json(&serde_json::json!({"agent_id": agent_id}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let resp = client
        .post(format!(
            "{}/api/projects/{}/conduits/{}/run",
            server.base_url, pid, wf_id
        ))
        .json(&serde_json::json!({
            "input": r#"{"task_id":"TASK-46","repo_spoke_label":"repo:dev"}"#
        }))
        .send()
        .await
        .unwrap();
    let status = resp.status();
    let err_body = resp.text().await.unwrap_or_default();
    assert_ne!(
        status,
        reqwest::StatusCode::BAD_REQUEST,
        "unexpected 400: {err_body}"
    );
}

#[tokio::test]
async fn test_trigger_crud() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    // Spawn agent for trigger
    let resp = client
        .post(format!("{}/api/agents", server.base_url))
        .json(&serde_json::json!({"manifest_toml": TEST_MANIFEST}))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    let agent_id = body["agent_id"].as_str().unwrap().to_string();

    // Create trigger (Lifecycle pattern — simplest variant)
    let resp = client
        .post(format!("{}/api/triggers", server.base_url))
        .json(&serde_json::json!({
            "agent_id": agent_id,
            "pattern": "lifecycle",
            "prompt_template": "Handle: {{event}}",
            "max_fires": 5
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = resp.json().await.unwrap();
    let trigger_id = body["trigger_id"].as_str().unwrap().to_string();
    assert_eq!(body["agent_id"], agent_id);

    // List triggers (unfiltered)
    let resp = client
        .get(format!("{}/api/triggers", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggers: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(triggers.len(), 1);
    assert_eq!(triggers[0]["agent_id"], agent_id);
    assert_eq!(triggers[0]["enabled"], true);
    assert_eq!(triggers[0]["max_fires"], 5);

    // List triggers (filtered by agent_id)
    let resp = client
        .get(format!(
            "{}/api/triggers?agent_id={}",
            server.base_url, agent_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggers: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(triggers.len(), 1);

    // Delete trigger
    let resp = client
        .delete(format!("{}/api/triggers/{}", server.base_url, trigger_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // List triggers (should be empty)
    let resp = client
        .get(format!("{}/api/triggers", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggers: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(triggers.len(), 0);
}

#[tokio::test]
async fn test_invalid_agent_id_returns_400() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    // Send message to invalid ID
    let resp = client
        .post(format!("{}/api/agents/not-a-uuid/message", server.base_url))
        .json(&serde_json::json!({"message": "hello"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["error"].as_str().unwrap().contains("Invalid"));

    // Kill invalid ID
    let resp = client
        .delete(format!("{}/api/agents/not-a-uuid", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    // Session for invalid ID
    let resp = client
        .get(format!("{}/api/agents/not-a-uuid/session", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn test_kill_nonexistent_agent_returns_404() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let fake_id = uuid::Uuid::new_v4();
    let resp = client
        .delete(format!("{}/api/agents/{}", server.base_url, fake_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_spawn_invalid_manifest_returns_400() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/api/agents", server.base_url))
        .json(&serde_json::json!({"manifest_toml": "this is {{ not valid toml"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["error"].as_str().unwrap().contains("Invalid manifest"));
}

#[tokio::test]
async fn test_request_id_header_is_uuid() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/api/health", server.base_url))
        .send()
        .await
        .unwrap();

    let request_id = resp
        .headers()
        .get("x-request-id")
        .expect("x-request-id header should be present");
    let id_str = request_id.to_str().unwrap();
    assert!(
        uuid::Uuid::parse_str(id_str).is_ok(),
        "x-request-id should be a valid UUID, got: {}",
        id_str
    );
}

#[tokio::test]
async fn test_multiple_agents_lifecycle() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    // Spawn 3 agents
    let mut ids = Vec::new();
    for i in 0..3 {
        let manifest = format!(
            r#"
name = "agent-{i}"
version = "0.1.0"
description = "Multi-agent test {i}"
author = "test"
module = "builtin:chat"

[model]
provider = "ollama"
model = "test-model"
system_prompt = "Agent {i}."

[capabilities]
memory_read = ["*"]
memory_write = ["self.*"]
"#
        );

        let resp = client
            .post(format!("{}/api/agents", server.base_url))
            .json(&serde_json::json!({"manifest_toml": manifest}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 201);
        let body: serde_json::Value = resp.json().await.unwrap();
        ids.push(body["agent_id"].as_str().unwrap().to_string());
    }

    // List should show 3 spawned agents
    let resp = client
        .get(format!("{}/api/agents", server.base_url))
        .send()
        .await
        .unwrap();
    let agents: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(agents.len(), 3);

    // Status should agree
    let resp = client
        .get(format!("{}/api/status", server.base_url))
        .send()
        .await
        .unwrap();
    let status: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(status["agent_count"], 3);

    // Kill one
    let resp = client
        .delete(format!("{}/api/agents/{}", server.base_url, ids[1]))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // List should show 2 spawned agents
    let resp = client
        .get(format!("{}/api/agents", server.base_url))
        .send()
        .await
        .unwrap();
    let agents: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(agents.len(), 2);

    // Kill the rest
    for id in [&ids[0], &ids[2]] {
        client
            .delete(format!("{}/api/agents/{}", server.base_url, id))
            .send()
            .await
            .unwrap();
    }

    // List should be empty
    let resp = client
        .get(format!("{}/api/agents", server.base_url))
        .send()
        .await
        .unwrap();
    let agents: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(agents.len(), 0);
}

// ---------------------------------------------------------------------------
// Auth integration tests
// ---------------------------------------------------------------------------

/// Start a test server with Bearer-token authentication enabled.
async fn start_test_server_with_auth(api_key: &str) -> TestServer {
    let tmp = tempfile::tempdir().expect("Failed to create temp dir");

    let config = KernelConfig {
        home_dir: tmp.path().to_path_buf(),
        data_dir: tmp.path().join("data"),
        api_key: api_key.to_string(),
        default_model: DefaultModelConfig {
            provider: "ollama".to_string(),
            model: "test-model".to_string(),
            api_key_env: "OLLAMA_API_KEY".to_string(),
            base_url: None,
        },
        ..KernelConfig::default()
    };

    let kernel = OpenFangKernel::boot_with_config(config).expect("Kernel should boot");
    let kernel = Arc::new(kernel);
    kernel.set_self_handle();
    let backlog_store = kernel.backlog_store.clone();
    let (backlog_feed_tx, backlog_watcher) = test_backlog_ws_deps();

    let state = Arc::new(AppState {
        kernel,
        started_at: Instant::now(),
        peer_registry: None,
        bridge_manager: tokio::sync::Mutex::new(None),
        channels_config: tokio::sync::RwLock::new(Default::default()),
        shutdown_notify: Arc::new(tokio::sync::Notify::new()),
        clawhub_cache: dashmap::DashMap::new(),
        provider_probe_cache: openfang_runtime::provider_health::ProbeCache::new(),
        budget_config: Arc::new(tokio::sync::RwLock::new(Default::default())),
        backlog_store,
        backlog_feed_tx,
        backlog_watcher,
    });

    let api_key = state.kernel.config.api_key.trim().to_string();
    let auth_state = middleware::AuthState {
        api_key: api_key.clone(),
        auth_enabled: state.kernel.config.auth.enabled,
        session_secret: if !api_key.is_empty() {
            api_key.clone()
        } else if state.kernel.config.auth.enabled {
            state.kernel.config.auth.password_hash.clone()
        } else {
            String::new()
        },
    };

    let app = Router::new()
        .route("/api/health", axum::routing::get(routes::health))
        .route("/api/status", axum::routing::get(routes::status))
        .route(
            "/api/agents",
            axum::routing::get(routes::list_agents).post(routes::spawn_agent),
        )
        .route(
            "/api/agents/{id}/message",
            axum::routing::post(routes::send_message),
        )
        .route(
            "/api/agents/{id}/session",
            axum::routing::get(routes::get_agent_session),
        )
        .route("/api/agents/{id}/ws", axum::routing::get(ws::agent_ws))
        .route(
            "/api/agents/{id}",
            axum::routing::delete(routes::kill_agent),
        )
        .route(
            "/api/triggers",
            axum::routing::get(routes::list_triggers).post(routes::create_trigger),
        )
        .route(
            "/api/triggers/{id}",
            axum::routing::delete(routes::delete_trigger),
        )
        .route(
            "/api/conduits",
            axum::routing::get(routes::list_conduits).post(routes::create_conduit),
        )
        .route(
            "/api/conduits/{id}/run",
            axum::routing::post(routes::run_conduit),
        )
        .route(
            "/api/conduits/{id}/runs/{run_id}",
            axum::routing::get(routes::get_conduit_run),
        )
        .route(
            "/api/conduits/{id}/runs",
            axum::routing::get(routes::list_conduit_runs),
        )
        .route("/api/shutdown", axum::routing::post(routes::shutdown))
        .layer(axum::middleware::from_fn_with_state(
            auth_state,
            middleware::auth,
        ))
        .layer(axum::middleware::from_fn(middleware::request_logging))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind test server");
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    TestServer {
        base_url: format!("http://{}", addr),
        state,
        _tmp: tmp,
    }
}

#[tokio::test]
async fn test_auth_health_is_public() {
    let server = start_test_server_with_auth("secret-key-123").await;
    let client = reqwest::Client::new();

    // /api/health should be accessible without auth
    let resp = client
        .get(format!("{}/api/health", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn test_auth_rejects_no_token() {
    let server = start_test_server_with_auth("secret-key-123").await;
    let client = reqwest::Client::new();

    // Protected endpoint without auth header → 401
    // Note: /api/status is public (dashboard needs it), so use a protected endpoint
    let resp = client
        .get(format!("{}/api/commands", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["error"].as_str().unwrap().contains("Missing"));
}

#[tokio::test]
async fn test_auth_rejects_wrong_token() {
    let server = start_test_server_with_auth("secret-key-123").await;
    let client = reqwest::Client::new();

    // Wrong bearer token → 401
    // Note: /api/status is public (dashboard needs it), so use a protected endpoint
    let resp = client
        .get(format!("{}/api/commands", server.base_url))
        .header("authorization", "Bearer wrong-key")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["error"].as_str().unwrap().contains("Invalid"));
}

#[tokio::test]
async fn test_auth_accepts_correct_token() {
    let server = start_test_server_with_auth("secret-key-123").await;
    let client = reqwest::Client::new();

    // Correct bearer token → 200
    let resp = client
        .get(format!("{}/api/status", server.base_url))
        .header("authorization", "Bearer secret-key-123")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "running");
}

#[tokio::test]
async fn test_auth_disabled_when_no_key() {
    // Empty API key = auth disabled
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    // Protected endpoint accessible without auth when no key is configured
    let resp = client
        .get(format!("{}/api/status", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn test_workflow_runs_filtered_list_and_run_detail() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/api/agents", server.base_url))
        .json(&serde_json::json!({"manifest_toml": TEST_MANIFEST}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    let resp = client
        .post(format!("{}/api/conduits", server.base_url))
        .json(&serde_json::json!({
            "name": "wf-runs-test-a",
            "description": "test",
            "steps": [{
                "name": "only",
                "agent_name": "test-agent",
                "mode": "sequential",
                "prompt": "Echo: {{input}}"
            }]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let wf_a: serde_json::Value = resp.json().await.unwrap();
    let wf_id_a = wf_a["conduit_id"].as_str().unwrap();

    let resp = client
        .post(format!("{}/api/conduits", server.base_url))
        .json(&serde_json::json!({
            "name": "wf-runs-test-b",
            "description": "other",
            "steps": [{
                "name": "x",
                "agent_name": "test-agent",
                "mode": "sequential",
                "prompt": "{{input}}"
            }]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let wf_b: serde_json::Value = resp.json().await.unwrap();
    let wf_id_b = wf_b["conduit_id"].as_str().unwrap();

    let resp = client
        .get(format!(
            "{}/api/conduits/{}/runs",
            server.base_url, wf_id_a
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let runs_a0: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert!(runs_a0.is_empty());

    let _ = client
        .post(format!(
            "{}/api/conduits/{}/run",
            server.base_url, wf_id_a
        ))
        .json(&serde_json::json!({"input": "hello-runs-test"}))
        .send()
        .await
        .unwrap();

    let resp = client
        .get(format!(
            "{}/api/conduits/{}/runs",
            server.base_url, wf_id_a
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let runs_a: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(runs_a.len(), 1);
    assert_eq!(runs_a[0]["conduit_id"], wf_id_a);
    assert_ne!(runs_a[0]["conduit_id"], wf_id_b);
    assert!(runs_a[0]["state"].is_string());
    assert!(runs_a[0]["input_preview"].is_string());
    assert!(
        runs_a[0]["duration_ms"].is_u64() || runs_a[0]["duration_ms"].is_i64(),
        "duration_ms: {:?}",
        runs_a[0]["duration_ms"]
    );
    assert_eq!(runs_a[0]["step_count"], 1);
    let run_id = runs_a[0]["id"].as_str().unwrap();

    let resp = client
        .get(format!(
            "{}/api/conduits/{}/runs",
            server.base_url, wf_id_b
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let runs_b: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert!(runs_b.is_empty());

    let resp = client
        .get(format!(
            "{}/api/conduits/{}/runs/{}",
            server.base_url, wf_id_a, run_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let detail: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(detail["id"], run_id);
    assert_eq!(detail["conduit_id"], wf_id_a);
    assert!(detail["step_results"].is_array());

    let resp = client
        .get(format!(
            "{}/api/conduits/{}/runs/{}",
            server.base_url, wf_id_b, run_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);

    let resp = client
        .get(format!(
            "{}/api/conduits/{}/runs/not-a-uuid",
            server.base_url, wf_id_a
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}
