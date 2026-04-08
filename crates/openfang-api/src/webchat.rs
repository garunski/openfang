//! Embedded WebChat UI served as static HTML.
//!
//! The production dashboard is assembled at compile time from separate
//! HTML/CSS/JS files under `static/` using `include_str!()`. This keeps
//! single-binary deployment while allowing organized source files.
//!
//! Features:
//! - Alpine.js SPA with hash-based routing (10 panels)
//! - Dark/light theme toggle with system preference detection
//! - Responsive layout with collapsible sidebar
//! - Markdown rendering + syntax highlighting + Mermaid diagrams (bundled locally)
//! - WebSocket real-time chat with HTTP fallback
//! - Agent management, workflows, memory browser, audit log, and more

use axum::http::header;
use axum::http::StatusCode;
use axum::response::IntoResponse;

/// Nonce placeholder in compile-time HTML, replaced at request time.
const NONCE_PLACEHOLDER: &str = "__NONCE__";

/// Compile-time ETag based on the crate version.
/// Not used for the dashboard page (nonce prevents caching) but retained
/// for potential future use by static asset handlers.
#[allow(dead_code)]
const ETAG: &str = concat!("\"openfang-", env!("CARGO_PKG_VERSION"), "\"");

/// Embedded logo PNG for single-binary deployment.
const LOGO_PNG: &[u8] = include_bytes!("../static/logo.png");

/// Embedded favicon ICO for browser tabs.
const FAVICON_ICO: &[u8] = include_bytes!("../static/favicon.ico");

/// Font Awesome 4 webfonts (EasyMDE toolbar icons). CSS uses absolute `/vendor/fa4/…` URLs.
const FA4_WOFF2: &[u8] = include_bytes!("../static/vendor/fa4/fontawesome-webfont.woff2");
const FA4_WOFF: &[u8] = include_bytes!("../static/vendor/fa4/fontawesome-webfont.woff");

/// Diff2Html stylesheet — loaded inside a shadow root so dashboard CSS cannot break layout.
const DIFF2HTML_CSS: &[u8] = include_bytes!("../static/vendor/diff2html/diff2html.min.css");

/// GET /vendor/diff2html/diff2html.min.css — for spoke git diff (shadow DOM; not bundled in main HTML).
pub async fn diff2html_css() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/css; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=86400, immutable"),
        ],
        DIFF2HTML_CSS,
    )
}

/// GET /vendor/fa4/{name} — Font Awesome 4 webfonts for the dashboard (public; no auth header on @font-face fetches).
pub async fn fa4_font(
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let (bytes, ct): (&[u8], &'static str) = match name.as_str() {
        "fontawesome-webfont.woff2" => (FA4_WOFF2, "font/woff2"),
        "fontawesome-webfont.woff" => (FA4_WOFF, "font/woff"),
        _ => return Err(StatusCode::NOT_FOUND),
    };
    Ok((
        [
            (header::CONTENT_TYPE, ct),
            (header::CACHE_CONTROL, "public, max-age=86400, immutable"),
        ],
        bytes,
    ))
}

/// GET /logo.png — Serve the OpenFang logo.
pub async fn logo_png() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "public, max-age=86400, immutable"),
        ],
        LOGO_PNG,
    )
}

/// GET /favicon.ico — Serve the OpenFang favicon.
pub async fn favicon_ico() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "image/x-icon"),
            (header::CACHE_CONTROL, "public, max-age=86400, immutable"),
        ],
        FAVICON_ICO,
    )
}

/// Embedded PWA manifest for installable web app support.
const MANIFEST_JSON: &str = include_str!("../static/manifest.json");

/// Embedded service worker for PWA support.
const SW_JS: &str = include_str!("../static/sw.js");

/// GET /manifest.json — Serve the PWA web app manifest.
pub async fn manifest_json() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "application/manifest+json"),
            (header::CACHE_CONTROL, "public, max-age=86400, immutable"),
        ],
        MANIFEST_JSON,
    )
}

/// GET /sw.js — Serve the PWA service worker.
pub async fn sw_js() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "application/javascript"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        SW_JS,
    )
}

/// GET / — Serve the OpenFang Dashboard single-page application.
///
/// Generates a unique CSP nonce on every request and injects it into both
/// the `<script>` tags and the `Content-Security-Policy` header. This
/// replaces `'unsafe-inline'` so only our own scripts execute.
pub async fn webchat_page() -> impl IntoResponse {
    let nonce = uuid::Uuid::new_v4().to_string();
    let html = WEBCHAT_HTML.replace(NONCE_PLACEHOLDER, &nonce);
    let csp = format!(
        "default-src 'self'; \
         script-src 'self' 'nonce-{nonce}' 'unsafe-eval'; \
         style-src 'self' 'unsafe-inline' https://fonts.googleapis.com https://fonts.gstatic.com; \
         img-src 'self' data: blob:; \
         connect-src 'self' ws://localhost:* ws://127.0.0.1:* wss://localhost:* wss://127.0.0.1:*; \
         font-src 'self' https://fonts.gstatic.com; \
         media-src 'self' blob:; \
         frame-src 'self' blob:; \
         object-src 'none'; \
         base-uri 'self'; \
         form-action 'self'"
    );
    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8".to_string()),
            (
                header::HeaderName::from_static("content-security-policy"),
                csp,
            ),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
        html,
    )
}

/// The embedded HTML/CSS/JS for the OpenFang Dashboard.
///
/// Assembled at compile time from organized static files.
/// All vendor libraries (Alpine.js, marked.js, highlight.js, Mermaid, Font Awesome 4, EasyMDE) are bundled
/// locally — no CDN dependency. Alpine.js is included LAST because it
/// immediately processes x-data directives and fires alpine:init on load.
const WEBCHAT_HTML: &str = concat!(
    include_str!("../static/index_head.html"),
    "<style>\n",
    include_str!("../static/vendor/bulma.min.css"),
    "\n",
    include_str!("../static/css/bulma-bridge.css"),
    "\n",
    include_str!("../static/css/theme.css"),
    "\n",
    include_str!("../static/css/layout.css"),
    "\n",
    include_str!("../static/css/components.css"),
    "\n",
    include_str!("../static/vendor/github-dark.min.css"),
    "\n",
    include_str!("../static/vendor/font-awesome.min.css"),
    "\n",
    include_str!("../static/vendor/easymde.min.css"),
    "\n</style>\n",
    include_str!("../static/index_body.html"),
    // Vendor libs: marked + highlight first (used by app.js), then Chart.js
    "<script nonce=\"__NONCE__\">\n",
    include_str!("../static/vendor/marked.min.js"),
    "\n</script>\n",
    "<script nonce=\"__NONCE__\">\n",
    include_str!("../static/vendor/highlight.min.js"),
    "\n</script>\n",
    "<script nonce=\"__NONCE__\">\n",
    include_str!("../static/vendor/easymde.min.js"),
    "\n</script>\n",
    "<script nonce=\"__NONCE__\">\n",
    include_str!("../static/vendor/chart.umd.min.js"),
    "\n</script>\n",
    "<script nonce=\"__NONCE__\">\n",
    include_str!("../static/vendor/mermaid.min.js"),
    "\n</script>\n",
    "<script nonce=\"__NONCE__\">\n",
    include_str!("../static/vendor/diff2html/diff2html-ui.min.js"),
    "\n</script>\n",
    // App code
    "<script nonce=\"__NONCE__\">\n",
    include_str!("../static/js/api.js"),
    "\n",
    include_str!("../static/js/app.js"),
    "\n",
    include_str!("../static/js/markdown-editor.js"),
    "\n",
    include_str!("../static/js/pages/overview.js"),
    "\n",
    include_str!("../static/js/pages/backlog-board.js"),
    "\n",
    include_str!("../static/js/pages/backlog-pert.js"),
    "\n",
    include_str!("../static/js/pages/backlog-list.js"),
    "\n",
    include_str!("../static/js/pages/task-detail-modal.js"),
    "\n",
    include_str!("../static/js/pages/backlog-docs.js"),
    "\n",
    include_str!("../static/js/pages/backlog-decisions.js"),
    "\n",
    include_str!("../static/js/pages/backlog-milestones.js"),
    "\n",
    include_str!("../static/js/pages/backlog-search.js"),
    "\n",
    include_str!("../static/js/pages/projects.js"),
    "\n",
    include_str!("../static/js/katex.js"),
    "\n",
    include_str!("../static/js/pages/chat.js"),
    "\n",
    include_str!("../static/js/pages/agents.js"),
    "\n",
    include_str!("../static/js/pages/workflows.js"),
    "\n",
    include_str!("../static/js/pages/workflow-builder.js"),
    "\n",
    include_str!("../static/js/pages/channels.js"),
    "\n",
    include_str!("../static/js/pages/skills.js"),
    "\n",
    include_str!("../static/js/pages/hands.js"),
    "\n",
    include_str!("../static/js/pages/scheduler.js"),
    "\n",
    include_str!("../static/js/pages/settings.js"),
    "\n",
    include_str!("../static/js/pages/usage.js"),
    "\n",
    include_str!("../static/js/pages/sessions.js"),
    "\n",
    include_str!("../static/js/pages/logs.js"),
    "\n",
    include_str!("../static/js/pages/wizard.js"),
    "\n",
    include_str!("../static/js/pages/approvals.js"),
    "\n",
    include_str!("../static/js/pages/comms.js"),
    "\n",
    include_str!("../static/js/pages/runtime.js"),
    "\n",
    include_str!("../static/js/mermaid-render.js"),
    "\n</script>\n",
    // Alpine.js MUST be last — it processes x-data and fires alpine:init
    "<script nonce=\"__NONCE__\">\n",
    include_str!("../static/vendor/alpine.min.js"),
    "\n</script>\n",
    "</body></html>"
);

#[cfg(test)]
mod dashboard_embed_tests {
    #[test]
    fn projects_page_includes_detail_view() {
        assert!(
            super::WEBCHAT_HTML.contains("selectedProject"),
            "expected Projects detail split (selectedProject)"
        );
        assert!(
            super::WEBCHAT_HTML.contains("loadDetailTab"),
            "expected projects.js detail tab loader"
        );
        assert!(
            super::WEBCHAT_HTML.contains("discoverSpokes"),
            "expected spokes discover action"
        );
        assert!(
            super::WEBCHAT_HTML.contains("refreshSpokeGitPanel")
                && super::WEBCHAT_HTML.contains("spokeDiffContainer")
                && super::WEBCHAT_HTML.contains("openSpokeHistory")
                && super::WEBCHAT_HTML.contains("spokeHistoryDiffContainer")
                && super::WEBCHAT_HTML.contains("hubSpokeFromList")
                && super::WEBCHAT_HTML.contains("of-hub-spoke-map")
                && super::WEBCHAT_HTML.contains("of-topology-viewport")
                && super::WEBCHAT_HTML.contains("topologySurfaceStyle")
                && super::WEBCHAT_HTML.contains("spokeTopologyEdges")
                && super::WEBCHAT_HTML.contains("project-spokes-tab"),
            "expected spoke git + diff + history + hub topology wiring"
        );
        assert!(
            super::WEBCHAT_HTML.contains("openBacklogTaskDetail"),
            "expected backlog task drill-down"
        );
        assert!(
            super::WEBCHAT_HTML.contains("kanban") && super::WEBCHAT_HTML.contains("board"),
            "expected kanban board in dashboard HTML"
        );
        assert!(
            super::WEBCHAT_HTML.contains("pert-chart-stage")
                && super::WEBCHAT_HTML.contains("loadPertTab"),
            "expected PERT chart tab wiring"
        );
        assert!(
            super::WEBCHAT_HTML.contains("task-list-table")
                && super::WEBCHAT_HTML.contains("openBacklogTaskDetail")
                && super::WEBCHAT_HTML.contains("openBacklogCleanupModal"),
            "expected task list + backlog detail modal wiring"
        );
        assert!(
            super::WEBCHAT_HTML.contains("loadDocsTab")
                && super::WEBCHAT_HTML.contains("docSidebarRows")
                && super::WEBCHAT_HTML.contains("openDocCreateModal")
                && super::WEBCHAT_HTML.contains("submitDocEditor")
                && super::WEBCHAT_HTML.contains("decisionsSelectRow")
                && super::WEBCHAT_HTML.contains("openDecisionCreateModal")
                && super::WEBCHAT_HTML.contains("submitDecisionEditor")
                && super::WEBCHAT_HTML.contains("confirmDeleteDoc")
                && super::WEBCHAT_HTML.contains("confirmDeleteDecision")
                && super::WEBCHAT_HTML.contains("confirmDeleteMilestone"),
            "expected backlog docs + decisions dashboard wiring"
        );
        assert!(
            super::WEBCHAT_HTML.contains("milestoneArchive")
                && super::WEBCHAT_HTML.contains("openMilestoneCreateModal")
                && super::WEBCHAT_HTML.contains("submitMilestoneEditor")
                && super::WEBCHAT_HTML.contains("openMilestoneAddTaskModal")
                && super::WEBCHAT_HTML.contains("submitMilestoneAddTask")
                && super::WEBCHAT_HTML.contains("onBacklogSearchInput"),
            "expected backlog milestones and search wiring"
        );
        assert!(
            super::WEBCHAT_HTML.contains("/api/backlog/ws")
                && super::WEBCHAT_HTML.contains("openfang-backlog-updated"),
            "expected backlog live WebSocket client wiring"
        );
        assert!(
            super::WEBCHAT_HTML.contains("of-mermaid-pending")
                && super::WEBCHAT_HTML.contains("renderMermaidIn"),
            "expected Mermaid markdown wiring"
        );
        assert!(
            super::WEBCHAT_HTML.contains("nonce=\"__NONCE__\">if('serviceWorker'"),
            "expected PWA service worker bootstrap script to carry CSP nonce"
        );
    }
}
