//! HTTP/WebSocket API server for the OpenFang Agent OS daemon.
//!
//! Exposes agent management, status, and chat via JSON REST endpoints.
//! The kernel runs in-process; the CLI connects over HTTP.

pub mod channel_bridge;
pub mod git_workspace;
pub mod middleware;
pub mod openai_compat;
pub mod project_backlog;
pub mod project_scoped;
pub mod rate_limiter;
pub mod routes;
pub mod server;
pub mod session_auth;
pub mod stream_chunker;
pub mod stream_dedup;
pub mod types;
pub mod webchat;
pub mod ws;

#[cfg(test)]
mod dashboard_static_tests {
    #[test]
    fn project_detail_includes_mattermost_tab() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let html =
            std::fs::read_to_string(root.join("static/index_body.html")).expect("read index_body");
        let js =
            std::fs::read_to_string(root.join("static/js/pages/projects.js")).expect("read projects.js");
        assert!(
            html.contains("detailTab === 'mattermost'"),
            "expected Mattermost panel markup"
        );
        assert!(
            js.contains("saveMattermostBinding"),
            "expected Mattermost save handler"
        );
        assert!(
            js.contains("sendMattermostTestMessage"),
            "expected Mattermost test message handler"
        );
        assert!(
            js.contains("mattermost: true"),
            "expected mattermost tab flag in tab set"
        );
    }

    #[test]
    fn project_conduit_runs_tab_uses_engine_endpoint() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let html =
            std::fs::read_to_string(root.join("static/index_body.html")).expect("read index_body");
        let js =
            std::fs::read_to_string(root.join("static/js/pages/projects.js")).expect("read projects.js");
        assert!(
            html.contains("detailConduitRuns"),
            "expected conduit runs table data source"
        );
        assert!(
            js.contains("conduit-runs?limit="),
            "expected project conduit-runs API path"
        );
        assert!(
            !js.contains("openWorkflowRunsPanel"),
            "removed per-project workflow definitions panel"
        );
    }
}
