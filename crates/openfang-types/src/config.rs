//! Configuration types for the OpenFang kernel.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Deserialize a `Vec<String>` that tolerates both string and integer elements.
///
/// When channel configs are saved from the web dashboard, numeric IDs (e.g. Mattermost
/// channel ids) may be stored as TOML integers. This helper
/// transparently converts integers back to strings so deserialization never fails.
fn deserialize_string_or_int_vec<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let values: Vec<serde_json::Value> = serde::Deserialize::deserialize(deserializer)?;
    Ok(values
        .into_iter()
        .map(|v| match v {
            serde_json::Value::String(s) => s,
            serde_json::Value::Number(n) => n.to_string(),
            other => other.to_string(),
        })
        .collect())
}

/// DM (direct message) policy for a channel.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DmPolicy {
    /// Respond to all DMs.
    #[default]
    Respond,
    /// Only respond to DMs from allowed users.
    AllowedOnly,
    /// Ignore all DMs.
    Ignore,
}

/// Group message policy for a channel.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupPolicy {
    /// Respond to all group messages.
    All,
    /// Only respond when mentioned (@bot).
    #[default]
    MentionOnly,
    /// Only respond to slash commands.
    CommandsOnly,
    /// Ignore all group messages.
    Ignore,
}

/// Output format hint for channel-specific message formatting.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    /// Standard Markdown (default).
    #[default]
    Markdown,
    /// Telegram-style HTML subset (format name; suitable for HTML-capable chat APIs).
    TelegramHtml,
    /// Slack-style mrkdwn (format name; often used for Mattermost-like clients).
    SlackMrkdwn,
    /// Plain text (no formatting).
    PlainText,
}

/// Per-channel behavior overrides.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ChannelOverrides {
    /// Model override (uses agent's default if None).
    pub model: Option<String>,
    /// System prompt override.
    pub system_prompt: Option<String>,
    /// DM policy.
    pub dm_policy: DmPolicy,
    /// Group message policy.
    pub group_policy: GroupPolicy,
    /// Per-user rate limit (messages per minute, 0 = unlimited).
    pub rate_limit_per_user: u32,
    /// Enable thread replies.
    pub threading: bool,
    /// Output format override.
    pub output_format: Option<OutputFormat>,
    /// Usage footer mode override.
    pub usage_footer: Option<UsageFooterMode>,
    /// Typing indicator mode override.
    pub typing_mode: Option<TypingMode>,
    /// Whether to send lifecycle emoji reactions (⏳🤔✅❌) on messages.
    /// Defaults to true. Set to false to suppress automatic reactions on the channel.
    #[serde(default = "default_true")]
    pub lifecycle_reactions: bool,
}

impl Default for ChannelOverrides {
    fn default() -> Self {
        Self {
            model: None,
            system_prompt: None,
            dm_policy: DmPolicy::default(),
            group_policy: GroupPolicy::default(),
            rate_limit_per_user: 0,
            threading: false,
            output_format: None,
            usage_footer: None,
            typing_mode: None,
            lifecycle_reactions: true,
        }
    }
}

/// Controls what usage info appears in response footers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageFooterMode {
    /// Don't show usage info.
    Off,
    /// Show token counts only.
    Tokens,
    /// Show estimated cost only.
    Cost,
    /// Show tokens + cost (default).
    #[default]
    Full,
}

/// Kernel operating mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KernelMode {
    /// Conservative mode — no auto-updates, pinned models, stability-first.
    Stable,
    /// Default balanced mode.
    #[default]
    Default,
    /// Developer mode — experimental features enabled.
    Dev,
}

/// User configuration for RBAC multi-user support.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserConfig {
    /// User display name.
    pub name: String,
    /// User role (owner, admin, user, viewer).
    #[serde(default = "default_role")]
    pub role: String,
    /// Channel bindings: maps channel platform IDs to this user.
    /// e.g., `{"signal": "+15551212", "mattermost": "userid"}`
    #[serde(default)]
    pub channel_bindings: HashMap<String, String>,
    /// Optional API key hash for API authentication.
    #[serde(default)]
    pub api_key_hash: Option<String>,
}

fn default_role() -> String {
    "user".to_string()
}

/// Web search provider selection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchProvider {
    /// Brave Search API.
    Brave,
    /// Tavily AI-agent-native search.
    Tavily,
    /// Perplexity AI search.
    Perplexity,
    /// DuckDuckGo HTML (no API key needed).
    DuckDuckGo,
    /// SearXNG self-hosted search (no API key needed).
    Searxng,
    /// Auto-select based on available API keys (Tavily → Brave → Perplexity → Searxng → DuckDuckGo).
    #[default]
    Auto,
}

/// Web tools configuration (search + fetch).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WebConfig {
    /// Which search provider to use.
    pub search_provider: SearchProvider,
    /// Cache TTL in minutes (0 = disabled).
    pub cache_ttl_minutes: u64,
    /// Brave Search configuration.
    pub brave: BraveSearchConfig,
    /// Tavily Search configuration.
    pub tavily: TavilySearchConfig,
    /// Perplexity Search configuration.
    pub perplexity: PerplexitySearchConfig,
    /// SearXNG Search configuration.
    pub searxng: SearxngSearchConfig,
    /// Web fetch configuration.
    pub fetch: WebFetchConfig,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            search_provider: SearchProvider::default(),
            cache_ttl_minutes: 15,
            brave: BraveSearchConfig::default(),
            tavily: TavilySearchConfig::default(),
            perplexity: PerplexitySearchConfig::default(),
            searxng: SearxngSearchConfig::default(),
            fetch: WebFetchConfig::default(),
        }
    }
}

/// Brave Search API configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BraveSearchConfig {
    /// Env var name holding the API key.
    pub api_key_env: String,
    /// Maximum results to return.
    pub max_results: usize,
    /// Country code for search localization (e.g., "US").
    pub country: String,
    /// Search language (e.g., "en").
    pub search_lang: String,
    /// Freshness filter (e.g., "pd" = past day, "pw" = past week).
    pub freshness: String,
}

impl Default for BraveSearchConfig {
    fn default() -> Self {
        Self {
            api_key_env: "BRAVE_API_KEY".to_string(),
            max_results: 5,
            country: String::new(),
            search_lang: String::new(),
            freshness: String::new(),
        }
    }
}

/// Tavily Search API configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TavilySearchConfig {
    /// Env var name holding the API key.
    pub api_key_env: String,
    /// Search depth: "basic" or "advanced".
    pub search_depth: String,
    /// Maximum results to return.
    pub max_results: usize,
    /// Include AI-generated answer summary.
    pub include_answer: bool,
}

impl Default for TavilySearchConfig {
    fn default() -> Self {
        Self {
            api_key_env: "TAVILY_API_KEY".to_string(),
            search_depth: "basic".to_string(),
            max_results: 5,
            include_answer: true,
        }
    }
}

/// Perplexity Search API configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PerplexitySearchConfig {
    /// Env var name holding the API key.
    pub api_key_env: String,
    /// Model to use for search (e.g., "sonar").
    pub model: String,
}

impl Default for PerplexitySearchConfig {
    fn default() -> Self {
        Self {
            api_key_env: "PERPLEXITY_API_KEY".to_string(),
            model: "sonar".to_string(),
        }
    }
}

/// SearXNG self-hosted search configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SearxngSearchConfig {
    /// Base URL of the SearXNG instance (e.g., "https://search.example.com").
    pub url: String,
}

/// Web fetch configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WebFetchConfig {
    /// Maximum characters to return in content.
    pub max_chars: usize,
    /// Maximum response body size in bytes.
    pub max_response_bytes: usize,
    /// HTTP request timeout in seconds.
    pub timeout_secs: u64,
    /// Enable HTML→Markdown readability extraction.
    pub readability: bool,
    /// SSRF allowlist for self-hosted environments.
    ///
    /// Entries can be exact hostnames (`"n8n.local"`), wildcard domains
    /// (`"*.olares.com"`), or CIDR ranges (`"10.0.0.0/8"`).
    ///
    /// Allowlisted hosts bypass the private-IP check but **never** bypass
    /// cloud metadata endpoint blocking (169.254.169.254, metadata.google.internal, etc.).
    #[serde(default)]
    pub ssrf_allowed_hosts: Vec<String>,
}

impl Default for WebFetchConfig {
    fn default() -> Self {
        Self {
            max_chars: 50_000,
            max_response_bytes: 10 * 1024 * 1024, // 10 MB
            timeout_secs: 30,
            readability: true,
            ssrf_allowed_hosts: Vec::new(),
        }
    }
}

/// Browser automation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BrowserConfig {
    /// Enable the built-in CDP browser tools (browser_navigate, browser_click,
    /// etc.).  Set to `false` when using an external browser MCP server such as
    /// CamoFox, which replaces these tools with its own set.
    pub enabled: bool,
    /// Run browser in headless mode (no visible window).
    pub headless: bool,
    /// Viewport width in pixels.
    pub viewport_width: u32,
    /// Viewport height in pixels.
    pub viewport_height: u32,
    /// Per-action timeout in seconds.
    pub timeout_secs: u64,
    /// Idle timeout — auto-close session after this many seconds of inactivity.
    pub idle_timeout_secs: u64,
    /// Maximum concurrent browser sessions.
    pub max_sessions: usize,
    /// Path to Chromium/Chrome binary. Auto-detected if None.
    pub chromium_path: Option<String>,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            headless: true,
            viewport_width: 1280,
            viewport_height: 720,
            timeout_secs: 30,
            idle_timeout_secs: 300,
            max_sessions: 5,
            chromium_path: None,
        }
    }
}

/// Config hot-reload mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReloadMode {
    /// No automatic reloading.
    Off,
    /// Full restart on config change.
    Restart,
    /// Hot-reload safe sections only (channels, skills, heartbeat).
    Hot,
    /// Hot-reload where possible, flag restart-required otherwise.
    #[default]
    Hybrid,
}

/// Configuration for config file watching and hot-reload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ReloadConfig {
    /// Reload mode. Default: hybrid.
    pub mode: ReloadMode,
    /// Debounce window in milliseconds. Default: 500.
    pub debounce_ms: u64,
}

impl Default for ReloadConfig {
    fn default() -> Self {
        Self {
            mode: ReloadMode::default(),
            debounce_ms: 500,
        }
    }
}

/// Webhook trigger authentication configuration.
///
/// Controls the `/hooks/wake` and `/hooks/agent` endpoints for external
/// systems to trigger agent actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WebhookTriggerConfig {
    /// Enable webhook trigger endpoints. Default: false.
    pub enabled: bool,
    /// Env var name holding the bearer token (NOT the token itself).
    /// MUST be set if enabled=true. Token must be >= 32 chars.
    pub token_env: String,
    /// Max payload size in bytes. Default: 65536.
    pub max_payload_bytes: usize,
    /// Rate limit: max requests per minute per IP. Default: 30.
    pub rate_limit_per_minute: u32,
}

impl Default for WebhookTriggerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            token_env: "OPENFANG_WEBHOOK_TOKEN".to_string(),
            max_payload_bytes: 65536,
            rate_limit_per_minute: 30,
        }
    }
}

/// Fallback provider chain — tried in order if the primary provider fails.
///
/// Configurable in `config.toml` under `[[fallback_providers]]`:
/// ```toml
/// [[fallback_providers]]
/// provider = "ollama"
/// model = "llama3.2:latest"
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FallbackProviderConfig {
    /// Provider name (e.g., "ollama", "groq").
    pub provider: String,
    /// Model to use from this provider.
    pub model: String,
    /// Environment variable for API key (empty for local providers).
    #[serde(default)]
    pub api_key_env: String,
    /// Base URL override (uses catalog default if None).
    #[serde(default)]
    pub base_url: Option<String>,
}

/// Text-to-speech configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TtsConfig {
    /// Enable TTS. Default: false.
    pub enabled: bool,
    /// Default provider: "openai" or "elevenlabs".
    pub provider: Option<String>,
    /// OpenAI TTS settings.
    pub openai: TtsOpenAiConfig,
    /// ElevenLabs TTS settings.
    pub elevenlabs: TtsElevenLabsConfig,
    /// Max text length for TTS (chars). Default: 4096.
    pub max_text_length: usize,
    /// Timeout per TTS request in seconds. Default: 30.
    pub timeout_secs: u64,
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: None,
            openai: TtsOpenAiConfig::default(),
            elevenlabs: TtsElevenLabsConfig::default(),
            max_text_length: 4096,
            timeout_secs: 30,
        }
    }
}

/// OpenAI TTS settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TtsOpenAiConfig {
    /// Voice: alloy, echo, fable, onyx, nova, shimmer. Default: "alloy".
    pub voice: String,
    /// Model: "tts-1" or "tts-1-hd". Default: "tts-1".
    pub model: String,
    /// Output format: "mp3", "opus", "aac", "flac". Default: "mp3".
    pub format: String,
    /// Speed: 0.25 to 4.0. Default: 1.0.
    pub speed: f32,
}

impl Default for TtsOpenAiConfig {
    fn default() -> Self {
        Self {
            voice: "alloy".to_string(),
            model: "tts-1".to_string(),
            format: "mp3".to_string(),
            speed: 1.0,
        }
    }
}

/// ElevenLabs TTS settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TtsElevenLabsConfig {
    /// Voice ID. Default: "21m00Tcm4TlvDq8ikWAM" (Rachel).
    pub voice_id: String,
    /// Model ID. Default: "eleven_monolingual_v1".
    pub model_id: String,
    /// Stability (0.0-1.0). Default: 0.5.
    pub stability: f32,
    /// Similarity boost (0.0-1.0). Default: 0.75.
    pub similarity_boost: f32,
}

impl Default for TtsElevenLabsConfig {
    fn default() -> Self {
        Self {
            voice_id: "21m00Tcm4TlvDq8ikWAM".to_string(),
            model_id: "eleven_monolingual_v1".to_string(),
            stability: 0.5,
            similarity_boost: 0.75,
        }
    }
}

/// Docker container sandbox configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DockerSandboxConfig {
    /// Enable Docker sandbox. Default: false.
    pub enabled: bool,
    /// Docker image for exec sandbox. Default: "python:3.12-slim".
    pub image: String,
    /// Container name prefix. Default: "openfang-sandbox".
    pub container_prefix: String,
    /// Working directory inside container. Default: "/workspace".
    pub workdir: String,
    /// Network mode: "none", "bridge", or custom. Default: "none".
    pub network: String,
    /// Memory limit (e.g., "256m", "1g"). Default: "512m".
    pub memory_limit: String,
    /// CPU limit (e.g., 0.5, 1.0, 2.0). Default: 1.0.
    pub cpu_limit: f64,
    /// Max execution time in seconds. Default: 60.
    pub timeout_secs: u64,
    /// Read-only root filesystem. Default: true.
    pub read_only_root: bool,
    /// Additional capabilities to add. Default: empty (drop all).
    pub cap_add: Vec<String>,
    /// tmpfs mounts. Default: ["/tmp:size=64m"].
    pub tmpfs: Vec<String>,
    /// PID limit. Default: 100.
    pub pids_limit: u32,
    /// Docker sandbox mode: off, non_main, all. Default: off.
    #[serde(default)]
    pub mode: DockerSandboxMode,
    /// Container lifecycle scope. Default: session.
    #[serde(default)]
    pub scope: DockerScope,
    /// Cooldown before reusing a released container (seconds). Default: 300.
    #[serde(default = "default_reuse_cool_secs")]
    pub reuse_cool_secs: u64,
    /// Idle timeout — destroy containers after N seconds of inactivity. Default: 86400 (24h).
    #[serde(default = "default_docker_idle_timeout")]
    pub idle_timeout_secs: u64,
    /// Maximum age before forced destruction (seconds). Default: 604800 (7 days).
    #[serde(default = "default_docker_max_age")]
    pub max_age_secs: u64,
    /// Paths blocked from bind mounting.
    #[serde(default)]
    pub blocked_mounts: Vec<String>,
}

fn default_reuse_cool_secs() -> u64 {
    300
}
fn default_docker_idle_timeout() -> u64 {
    86400
}
fn default_docker_max_age() -> u64 {
    604800
}

impl Default for DockerSandboxConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            image: "python:3.12-slim".to_string(),
            container_prefix: "openfang-sandbox".to_string(),
            workdir: "/workspace".to_string(),
            network: "none".to_string(),
            memory_limit: "512m".to_string(),
            cpu_limit: 1.0,
            timeout_secs: 60,
            read_only_root: true,
            cap_add: Vec::new(),
            tmpfs: vec!["/tmp:size=64m".to_string()],
            pids_limit: 100,
            mode: DockerSandboxMode::Off,
            scope: DockerScope::Session,
            reuse_cool_secs: default_reuse_cool_secs(),
            idle_timeout_secs: default_docker_idle_timeout(),
            max_age_secs: default_docker_max_age(),
            blocked_mounts: Vec::new(),
        }
    }
}

/// Device pairing configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PairingConfig {
    /// Enable device pairing. Default: false.
    pub enabled: bool,
    /// Max paired devices. Default: 10.
    pub max_devices: usize,
    /// Pairing token expiry in seconds. Default: 300 (5 min).
    pub token_expiry_secs: u64,
    /// Push notification provider: "none", "ntfy", "gotify".
    pub push_provider: String,
    /// Ntfy server URL (if push_provider = "ntfy").
    pub ntfy_url: Option<String>,
    /// Ntfy topic (if push_provider = "ntfy").
    pub ntfy_topic: Option<String>,
}

impl Default for PairingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_devices: 10,
            token_expiry_secs: 300,
            push_provider: "none".to_string(),
            ntfy_url: None,
            ntfy_topic: None,
        }
    }
}

/// Extensions & integrations configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ExtensionsConfig {
    /// Enable auto-reconnect for MCP integrations.
    pub auto_reconnect: bool,
    /// Maximum reconnect attempts before giving up.
    pub reconnect_max_attempts: u32,
    /// Maximum backoff duration in seconds.
    pub reconnect_max_backoff_secs: u64,
    /// Health check interval in seconds.
    pub health_check_interval_secs: u64,
}

impl Default for ExtensionsConfig {
    fn default() -> Self {
        Self {
            auto_reconnect: true,
            reconnect_max_attempts: 10,
            reconnect_max_backoff_secs: 300,
            health_check_interval_secs: 60,
        }
    }
}

/// Credential vault configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VaultConfig {
    /// Whether the vault is enabled (auto-detected if vault.enc exists).
    pub enabled: bool,
    /// Custom vault file path (default: ~/.openfang/vault.enc).
    pub path: Option<PathBuf>,
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            path: None,
        }
    }
}

/// Agent binding — routes specific channel/account/peer patterns to agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentBinding {
    /// Target agent name or ID.
    pub agent: String,
    /// Match criteria (all specified fields must match).
    pub match_rule: BindingMatchRule,
}

/// Match rule for agent bindings. All specified (non-None) fields must match.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BindingMatchRule {
    /// Channel type (e.g. `"signal"`, `"mattermost"`).
    pub channel: Option<String>,
    /// Specific account/bot ID within the channel.
    pub account_id: Option<String>,
    /// Peer/user ID for DM routing.
    pub peer_id: Option<String>,
    /// Team/server scope when the channel uses it (e.g. Mattermost).
    pub guild_id: Option<String>,
    /// Role-based routing (user must have at least one).
    #[serde(default)]
    pub roles: Vec<String>,
}

impl BindingMatchRule {
    /// Calculate specificity score for binding priority ordering.
    /// Higher = more specific = checked first.
    pub fn specificity(&self) -> u32 {
        let mut score = 0u32;
        if self.peer_id.is_some() {
            score += 8;
        }
        if self.guild_id.is_some() {
            score += 4;
        }
        if !self.roles.is_empty() {
            score += 2;
        }
        if self.account_id.is_some() {
            score += 2;
        }
        if self.channel.is_some() {
            score += 1;
        }
        score
    }
}

/// Broadcast config — send same message to multiple agents.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BroadcastConfig {
    /// Broadcast strategy.
    pub strategy: BroadcastStrategy,
    /// Map of peer_id -> list of agent names to receive the message.
    pub routes: HashMap<String, Vec<String>>,
}

/// Broadcast delivery strategy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BroadcastStrategy {
    /// Send to all agents simultaneously.
    #[default]
    Parallel,
    /// Send to agents one at a time in order.
    Sequential,
}

/// Auto-reply engine configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AutoReplyConfig {
    /// Enable auto-reply engine. Default: false.
    pub enabled: bool,
    /// Max concurrent auto-reply tasks. Default: 3.
    pub max_concurrent: usize,
    /// Default timeout per reply in seconds. Default: 120.
    pub timeout_secs: u64,
    /// Patterns that suppress auto-reply (e.g., "/stop", "/pause").
    pub suppress_patterns: Vec<String>,
}

impl Default for AutoReplyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_concurrent: 3,
            timeout_secs: 120,
            suppress_patterns: vec!["/stop".to_string(), "/pause".to_string()],
        }
    }
}

/// Canvas (Agent-to-UI) configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CanvasConfig {
    /// Enable canvas tool. Default: false.
    pub enabled: bool,
    /// Max HTML size in bytes. Default: 512KB.
    pub max_html_bytes: usize,
    /// Allowed HTML tags (empty = all safe tags allowed).
    #[serde(default)]
    pub allowed_tags: Vec<String>,
}

impl Default for CanvasConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_html_bytes: 512 * 1024,
            allowed_tags: Vec::new(),
        }
    }
}

/// Shell/exec security mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExecSecurityMode {
    /// Block all shell execution.
    #[serde(alias = "none", alias = "disabled")]
    Deny,
    /// Only allow commands in safe_bins or allowed_commands.
    #[default]
    #[serde(alias = "restricted")]
    Allowlist,
    /// Allow all commands (unsafe, dev only).
    #[serde(alias = "allow", alias = "all", alias = "unrestricted")]
    Full,
}

/// Shell/exec security policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ExecPolicy {
    /// Security mode: "deny" blocks all, "allowlist" only allows listed,
    /// "full" allows all (unsafe, dev only).
    pub mode: ExecSecurityMode,
    /// Commands that bypass allowlist (stdin-only utilities).
    pub safe_bins: Vec<String>,
    /// Global command allowlist (when mode = allowlist).
    pub allowed_commands: Vec<String>,
    /// Max execution timeout in seconds. Default: 30.
    pub timeout_secs: u64,
    /// Max output size in bytes. Default: 100KB.
    pub max_output_bytes: usize,
    /// No-output idle timeout in seconds. When > 0, kills processes that
    /// produce no stdout/stderr output for this duration. Default: 30.
    #[serde(default = "default_no_output_timeout")]
    pub no_output_timeout_secs: u64,
}

fn default_no_output_timeout() -> u64 {
    30
}

impl Default for ExecPolicy {
    fn default() -> Self {
        Self {
            mode: ExecSecurityMode::default(),
            safe_bins: vec![
                "sleep", "true", "false", "cat", "sort", "uniq", "cut", "tr", "head", "tail", "wc",
                "date", "echo", "printf", "basename", "dirname", "pwd", "env",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            allowed_commands: Vec::new(),
            timeout_secs: 30,
            max_output_bytes: 100 * 1024,
            no_output_timeout_secs: default_no_output_timeout(),
        }
    }
}

// ---------------------------------------------------------------------------
// Gap 2: No-output idle timeout for subprocess sandbox
// ---------------------------------------------------------------------------

/// Reason a subprocess was terminated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminationReason {
    /// Process exited normally.
    Exited(i32),
    /// Absolute timeout exceeded.
    AbsoluteTimeout,
    /// No output timeout exceeded.
    NoOutputTimeout,
}

// ---------------------------------------------------------------------------
// Gap 3: Auth profile rotation — multi-key per provider
// ---------------------------------------------------------------------------

/// A named authentication profile for a provider.
///
/// Multiple profiles can be configured per provider to enable key rotation
/// when one key gets rate-limited or has billing issues.
#[derive(Clone, Serialize, Deserialize)]
pub struct AuthProfile {
    /// Profile name (e.g., "primary", "secondary").
    pub name: String,
    /// Environment variable holding the API key.
    pub api_key_env: String,
    /// Priority (lower = preferred). Default: 0.
    #[serde(default)]
    pub priority: u32,
}

/// SECURITY: Custom Debug impl redacts env var name.
impl std::fmt::Debug for AuthProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthProfile")
            .field("name", &self.name)
            .field("api_key_env", &"<redacted>")
            .field("priority", &self.priority)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Gap 5: Docker sandbox maturity
// ---------------------------------------------------------------------------

/// Docker sandbox activation mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DockerSandboxMode {
    /// Docker sandbox disabled.
    #[default]
    Off,
    /// Only use Docker for non-main agents.
    NonMain,
    /// Use Docker for all agents.
    All,
}

/// Docker container lifecycle scope.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DockerScope {
    /// Container per session (destroyed when session ends).
    #[default]
    Session,
    /// Container per agent (reused across sessions).
    Agent,
    /// Shared container pool.
    Shared,
}

// ---------------------------------------------------------------------------
// Gap 6: Typing indicator modes
// ---------------------------------------------------------------------------

/// Typing indicator behavior mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypingMode {
    /// Send typing indicator immediately on message receipt (default).
    #[default]
    Instant,
    /// Send typing indicator only when first text delta arrives.
    Message,
    /// Send typing indicator only during LLM reasoning.
    Thinking,
    /// Never send typing indicators.
    Never,
}

// ---------------------------------------------------------------------------
// Gap 7: Thinking level support
// ---------------------------------------------------------------------------

/// Extended thinking configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ThinkingConfig {
    /// Maximum tokens for thinking (budget).
    pub budget_tokens: u32,
    /// Whether to stream thinking tokens to the client.
    pub stream_thinking: bool,
}

impl Default for ThinkingConfig {
    fn default() -> Self {
        Self {
            budget_tokens: 10_000,
            stream_thinking: false,
        }
    }
}

/// Top-level kernel configuration.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct KernelConfig {
    /// OpenFang home directory (default: ~/.openfang).
    pub home_dir: PathBuf,
    /// Data directory for databases (default: ~/.openfang/data).
    pub data_dir: PathBuf,
    /// Log level (trace, debug, info, warn, error).
    pub log_level: String,
    /// API listen address (e.g., "0.0.0.0:4200").
    #[serde(alias = "listen_addr")]
    pub api_listen: String,
    /// Whether to enable the OFP network layer.
    pub network_enabled: bool,
    /// Default LLM provider configuration.
    pub default_model: DefaultModelConfig,
    /// When both `provider` and `model` are non-empty, per-project workflow orchestrators
    /// (e.g. Mattermost) spawn with this LLM; otherwise the hand's bundled model applies.
    #[serde(default)]
    pub orchestrator_default_model: DefaultModelConfig,
    /// Settings default for **memory embeddings** (semantic recall). When both `provider` and
    /// `model` are non-empty, this overrides `[memory].embedding_provider` / `embedding_model`
    /// for the embedding driver. When empty, `[memory]` and auto-detect apply.
    #[serde(default)]
    pub memory_default_embedding: DefaultModelConfig,
    /// Memory substrate configuration.
    pub memory: MemoryConfig,
    /// Network configuration.
    pub network: NetworkConfig,
    /// Channel bridge configuration (Signal, Mattermost).
    pub channels: ChannelsConfig,
    /// API authentication key. When set, all API endpoints (except /api/health)
    /// require a `Authorization: Bearer <key>` header.
    /// If empty, the API is unauthenticated (local development only).
    pub api_key: String,
    /// Kernel operating mode (stable, default, dev).
    #[serde(default)]
    pub mode: KernelMode,
    /// Language/locale for CLI and messages (default: "en").
    #[serde(default = "default_language")]
    pub language: String,
    /// User configurations for RBAC multi-user support.
    #[serde(default)]
    pub users: Vec<UserConfig>,
    /// MCP server configurations for external tool integration.
    #[serde(default)]
    pub mcp_servers: Vec<McpServerConfigEntry>,
    /// A2A (Agent-to-Agent) protocol configuration.
    #[serde(default)]
    pub a2a: Option<A2aConfig>,
    /// Usage footer mode (what to show after each response).
    #[serde(default)]
    pub usage_footer: UsageFooterMode,
    /// Web tools configuration (search + fetch).
    #[serde(default)]
    pub web: WebConfig,
    /// Fallback providers tried in order if the primary fails.
    /// Configure in config.toml as `[[fallback_providers]]`.
    #[serde(default)]
    pub fallback_providers: Vec<FallbackProviderConfig>,
    /// Browser automation configuration.
    #[serde(default)]
    pub browser: BrowserConfig,
    /// Extensions & integrations configuration.
    #[serde(default)]
    pub extensions: ExtensionsConfig,
    /// Credential vault configuration.
    #[serde(default)]
    pub vault: VaultConfig,
    /// Root directory for agent workspaces. Default: `~/.openfang/workspaces`
    #[serde(default)]
    pub workspaces_dir: Option<PathBuf>,
    /// Media understanding configuration.
    #[serde(default)]
    pub media: crate::media::MediaConfig,
    /// Link understanding configuration.
    #[serde(default)]
    pub links: crate::media::LinkConfig,
    /// Config hot-reload settings.
    #[serde(default)]
    pub reload: ReloadConfig,
    /// Webhook trigger configuration (external event injection).
    #[serde(default)]
    pub webhook_triggers: Option<WebhookTriggerConfig>,
    /// Execution approval policy.
    #[serde(default, alias = "approval_policy")]
    pub approval: crate::approval::ApprovalPolicy,
    /// Cron scheduler max total jobs across all agents. Default: 500.
    #[serde(default = "default_max_cron_jobs")]
    pub max_cron_jobs: usize,
    /// Config include files — loaded and deep-merged before the root config.
    /// Paths are relative to the root config file's directory.
    /// Security: absolute paths and `..` components are rejected.
    #[serde(default)]
    pub include: Vec<String>,
    /// Shell/exec security policy.
    #[serde(default)]
    pub exec_policy: ExecPolicy,
    /// Agent bindings for multi-account routing.
    #[serde(default)]
    pub bindings: Vec<AgentBinding>,
    /// Broadcast routing configuration.
    #[serde(default)]
    pub broadcast: BroadcastConfig,
    /// Auto-reply background engine configuration.
    #[serde(default)]
    pub auto_reply: AutoReplyConfig,
    /// Canvas (A2UI) configuration.
    #[serde(default)]
    pub canvas: CanvasConfig,
    /// Text-to-speech configuration.
    #[serde(default)]
    pub tts: TtsConfig,
    /// Docker container sandbox configuration.
    #[serde(default)]
    pub docker: DockerSandboxConfig,
    /// Device pairing configuration.
    #[serde(default)]
    pub pairing: PairingConfig,
    /// Auth profiles for key rotation (provider name → profiles).
    #[serde(default)]
    pub auth_profiles: HashMap<String, Vec<AuthProfile>>,
    /// Extended thinking configuration.
    #[serde(default)]
    pub thinking: Option<ThinkingConfig>,
    /// Global spending budget configuration.
    #[serde(default)]
    pub budget: BudgetConfig,
    /// Provider base URL overrides (provider ID → custom base URL).
    /// e.g. `ollama = "http://192.168.1.100:11434/v1"`
    #[serde(default)]
    pub provider_urls: HashMap<String, String>,
    /// Provider API key env var overrides (provider ID → env var name).
    /// For custom/unknown providers, maps the provider name to the environment
    /// variable holding the API key. e.g. `nvidia = "NVIDIA_API_KEY"`.
    /// If not set, the convention `{PROVIDER_UPPER}_API_KEY` is used automatically.
    #[serde(default)]
    pub provider_api_keys: HashMap<String, String>,
    /// Per-provider opt-in for using models in OpenFang (Settings → Providers).
    ///
    /// Missing entry defaults to **disabled** until toggled on. If the provider's API key
    /// env var is set in the process environment (non-empty), the provider is always
    /// treated as enabled regardless of this map.
    #[serde(default)]
    pub provider_enabled: HashMap<String, bool>,
    /// OAuth client ID overrides for PKCE flows.
    #[serde(default)]
    pub oauth: OAuthConfig,
    /// Dashboard authentication (username/password login).
    #[serde(default)]
    pub auth: AuthConfig,
    /// Directory for auto-loading workflow JSON files on startup.
    /// Defaults to `~/.openfang/conduits`. Set to empty string to disable.
    #[serde(default)]
    pub conduits_dir: Option<PathBuf>,
    /// Heartbeat monitor settings.
    #[serde(default)]
    pub heartbeat: HeartbeatSettings,
    /// Automation pipeline (spoke quality gates, backlog tooling).
    #[serde(default)]
    pub automation: AutomationConfig,
}

fn default_automation_max_retries() -> u32 {
    2
}

fn default_project_context_max_failures() -> usize {
    50
}

fn default_project_context_decision_max_age_days() -> u32 {
    30
}

fn default_project_context_prompt_max_chars() -> usize {
    8192
}

fn default_automation_model_routing() -> HashMap<String, String> {
    [
        ("planning".to_string(), "premium".to_string()),
        ("implementation".to_string(), "default".to_string()),
        ("retry".to_string(), "default".to_string()),
    ]
    .into_iter()
    .collect()
}

/// Built-in default model id when `[automation].model_routing` has no entry for `phase`.
pub fn default_automation_model_for_phase(phase: &str) -> &'static str {
    match phase {
        "planning" => "premium",
        "implementation" | "retry" => "default",
        _ => "default",
    }
}

/// Automation / pipeline configuration (`[automation]` in config.toml).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AutomationConfig {
    /// Absolute spoke repository roots allowed for `enforce_quality_gate` and `trigger_cursor_worker`.
    #[serde(default)]
    pub spoke_roots: Vec<PathBuf>,
    /// Absolute backlog repository roots for native `backlog_task_*` tools.
    #[serde(default)]
    pub backlog_roots: Vec<PathBuf>,
    /// Max pipeline retries after a failed quality gate (coordinator / hand).
    #[serde(default = "default_automation_max_retries")]
    pub max_retries: u32,
    /// Pipeline phase → model identifier (e.g. `planning`, `implementation`, `retry`).
    #[serde(default = "default_automation_model_routing")]
    pub model_routing: HashMap<String, String>,
    /// Max `past_failures` entries kept in per-project `context.json` (oldest dropped).
    #[serde(default = "default_project_context_max_failures")]
    pub project_context_max_failures: usize,
    /// Drop `decision_log` entries older than this many days (`0` = no age pruning).
    #[serde(default = "default_project_context_decision_max_age_days")]
    pub project_context_decision_max_age_days: u32,
    /// Max characters injected into workflow input from [`ProjectContext::conduit_prompt_section`].
    #[serde(default = "default_project_context_prompt_max_chars")]
    pub project_context_prompt_max_chars: usize,
    /// Default env var for GitHub API token used by `git_create_pr` when a project has no override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_token_env: Option<String>,
    /// Default `{task_id}` branch naming template for `git_create_branch`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_branch_name_template: Option<String>,
}

impl Default for AutomationConfig {
    fn default() -> Self {
        Self {
            spoke_roots: Vec::new(),
            backlog_roots: Vec::new(),
            max_retries: default_automation_max_retries(),
            model_routing: default_automation_model_routing(),
            project_context_max_failures: default_project_context_max_failures(),
            project_context_decision_max_age_days: default_project_context_decision_max_age_days(),
            project_context_prompt_max_chars: default_project_context_prompt_max_chars(),
            github_token_env: None,
            git_branch_name_template: None,
        }
    }
}

impl AutomationConfig {
    /// Resolved model identifier for a pipeline phase (configured or built-in default).
    pub fn model_for_phase(&self, phase: &str) -> &str {
        self.model_routing
            .get(phase)
            .map(|s| s.as_str())
            .unwrap_or_else(|| default_automation_model_for_phase(phase))
    }
}

/// Validate `backlog_root` is absolute, exists, and lies under one of `allowlisted_roots`.
pub fn validate_backlog_root_allowlisted(
    allowlisted_roots: &[PathBuf],
    backlog_root: &std::path::Path,
) -> Result<PathBuf, String> {
    if !backlog_root.is_absolute() {
        return Err("backlog_root must be an absolute path".to_string());
    }
    if allowlisted_roots.is_empty() {
        return Err(
            "No automation backlog roots configured. Add paths to [automation].backlog_roots in config.toml."
                .to_string(),
        );
    }
    let canon = std::fs::canonicalize(backlog_root)
        .map_err(|e| format!("backlog_root does not exist or is not accessible: {e}"))?;
    for root in allowlisted_roots {
        if !root.is_absolute() {
            continue;
        }
        let Ok(root_canon) = std::fs::canonicalize(root) else {
            continue;
        };
        if canon.strip_prefix(&root_canon).is_ok() {
            return Ok(canon);
        }
    }
    Err(format!(
        "backlog_root '{}' is not under any path in [automation].backlog_roots ({} configured root(s))",
        canon.display(),
        allowlisted_roots.len()
    ))
}

/// Resolve working directory for Backlog CLI: single configured root, or an explicit allowlisted path.
pub fn resolve_automation_backlog_cwd(
    allowlisted_roots: &[PathBuf],
    backlog_root_param: Option<&str>,
) -> Result<PathBuf, String> {
    if allowlisted_roots.is_empty() {
        return Err(
            "No automation backlog roots configured. Add paths to [automation].backlog_roots in config.toml."
                .to_string(),
        );
    }
    match backlog_root_param {
        Some(p) => validate_backlog_root_allowlisted(allowlisted_roots, std::path::Path::new(p)),
        None => {
            if allowlisted_roots.len() != 1 {
                return Err(
                    "Multiple [automation].backlog_roots entries are configured; pass backlog_root (absolute path under an allowlisted root) on this tool call."
                        .to_string(),
                );
            }
            let root = &allowlisted_roots[0];
            if !root.is_absolute() {
                return Err(
                    "backlog_roots entries must be absolute paths in config.toml.".to_string(),
                );
            }
            std::fs::canonicalize(root)
                .map_err(|e| format!("backlog root does not exist or is not accessible: {e}"))
        }
    }
}

/// Validate `spoke_root` is absolute, exists, and lies under one of `allowlisted_roots`.
pub fn validate_spoke_root_allowlisted(
    allowlisted_roots: &[PathBuf],
    spoke_root: &std::path::Path,
) -> Result<PathBuf, String> {
    if !spoke_root.is_absolute() {
        return Err("spoke_root must be an absolute path".to_string());
    }
    if allowlisted_roots.is_empty() {
        return Err(
            "No automation spoke roots configured. Add paths to [automation].spoke_roots in config.toml."
                .to_string(),
        );
    }
    let canon = std::fs::canonicalize(spoke_root)
        .map_err(|e| format!("spoke_root does not exist or is not accessible: {e}"))?;
    for root in allowlisted_roots {
        if !root.is_absolute() {
            continue;
        }
        let Ok(root_canon) = std::fs::canonicalize(root) else {
            continue;
        };
        if canon.strip_prefix(&root_canon).is_ok() {
            return Ok(canon);
        }
    }
    Err(format!(
        "spoke_root '{}' is not under any path in [automation].spoke_roots ({} configured root(s))",
        canon.display(),
        allowlisted_roots.len()
    ))
}

fn path_is_under_any_canonical_root(canon: &std::path::Path, roots: &[PathBuf]) -> bool {
    for root in roots {
        if !root.is_absolute() {
            continue;
        }
        let Ok(root_canon) = std::fs::canonicalize(root) else {
            continue;
        };
        if canon.strip_prefix(&root_canon).is_ok() {
            return true;
        }
    }
    false
}

/// Canonicalize `candidate` when it exists and lies under `[automation].spoke_roots` or `[automation].backlog_roots`.
///
/// At least one of the two allowlists must be non-empty. Used for unified pipeline path checks (e.g. git tools).
pub fn validate_automation_allowlisted_path(
    spoke_roots: &[PathBuf],
    backlog_roots: &[PathBuf],
    candidate: &std::path::Path,
) -> Result<PathBuf, String> {
    if spoke_roots.is_empty() && backlog_roots.is_empty() {
        return Err(
            "No automation paths configured. Add [automation].spoke_roots and/or [automation].backlog_roots in config.toml."
                .to_string(),
        );
    }
    if !candidate.is_absolute() {
        return Err("path must be an absolute path".to_string());
    }
    let canon = std::fs::canonicalize(candidate)
        .map_err(|e| format!("path does not exist or is not accessible: {e}"))?;
    if path_is_under_any_canonical_root(&canon, spoke_roots)
        || path_is_under_any_canonical_root(&canon, backlog_roots)
    {
        Ok(canon)
    } else {
        Err(format!(
            "path '{}' is not under any [automation].spoke_roots or [automation].backlog_roots entry",
            canon.display()
        ))
    }
}

/// Heartbeat monitor settings exposed in `[heartbeat]` config section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatSettings {
    /// Seconds of inactivity before a reactive agent is marked as unresponsive.
    /// Default: 180. Set higher to prevent idle hands from being marked as crashed.
    #[serde(default = "default_heartbeat_timeout")]
    pub default_timeout_secs: u64,
}

fn default_heartbeat_timeout() -> u64 {
    180
}

impl Default for HeartbeatSettings {
    fn default() -> Self {
        Self {
            default_timeout_secs: default_heartbeat_timeout(),
        }
    }
}

/// Dashboard authentication (username/password login).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthConfig {
    /// Enable username/password authentication for the dashboard.
    pub enabled: bool,
    /// Admin username.
    pub username: String,
    /// Argon2id password hash (PHC string format).
    /// Generate with: openfang auth hash-password
    pub password_hash: String,
    /// Session token lifetime in hours (default: 168 = 7 days).
    pub session_ttl_hours: u64,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            username: "admin".to_string(),
            password_hash: String::new(),
            session_ttl_hours: 168,
        }
    }
}

/// OAuth client ID overrides for PKCE flows.
///
/// Configure in config.toml:
/// ```toml
/// [oauth]
/// google_client_id = "your-google-client-id"
/// github_client_id = "your-github-client-id"
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct OAuthConfig {
    /// Google OAuth2 client ID for PKCE flow.
    pub google_client_id: Option<String>,
    /// GitHub OAuth client ID for PKCE flow.
    pub github_client_id: Option<String>,
    /// Microsoft (Entra ID) OAuth client ID.
    pub microsoft_client_id: Option<String>,
    /// Slack OAuth client ID.
    pub slack_client_id: Option<String>,
}

/// Global spending budget configuration.
///
/// Set limits to 0.0 for unlimited. All limits apply across all agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BudgetConfig {
    /// Maximum total cost in USD per hour (0.0 = unlimited).
    pub max_hourly_usd: f64,
    /// Maximum total cost in USD per day (0.0 = unlimited).
    pub max_daily_usd: f64,
    /// Maximum total cost in USD per month (0.0 = unlimited).
    pub max_monthly_usd: f64,
    /// Alert threshold as a fraction (0.0 - 1.0). Trigger warnings at this % of any limit.
    pub alert_threshold: f64,
    /// Default per-agent hourly token limit override. When set (> 0), all agents
    /// will be overridden to this value. Set to 0 to keep each agent's own limit.
    /// Use this to globally raise or lower the token budget for all agents.
    pub default_max_llm_tokens_per_hour: u64,
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            max_hourly_usd: 0.0,
            max_daily_usd: 0.0,
            max_monthly_usd: 0.0,
            alert_threshold: 0.8,
            default_max_llm_tokens_per_hour: 0,
        }
    }
}

fn default_max_cron_jobs() -> usize {
    500
}

/// Configuration entry for an MCP server.
///
/// This is the config.toml representation. The runtime `McpServerConfig`
/// struct is constructed from this during kernel boot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfigEntry {
    /// Display name for this server.
    pub name: String,
    /// Transport configuration.
    pub transport: McpTransportEntry,
    /// Request timeout in seconds.
    #[serde(default = "default_mcp_timeout")]
    pub timeout_secs: u64,
    /// Environment variables to pass through (e.g., ["GITHUB_PERSONAL_ACCESS_TOKEN"]).
    #[serde(default)]
    pub env: Vec<String>,
    /// Extra HTTP headers for SSE / Streamable-HTTP transports.
    /// Each entry is `"Header-Name: value"` (e.g., `"Authorization: Bearer <token>"`).
    #[serde(default)]
    pub headers: Vec<String>,
}

fn default_mcp_timeout() -> u64 {
    30
}

/// Transport configuration for an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpTransportEntry {
    /// Subprocess with JSON-RPC over stdin/stdout.
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
    },
    /// HTTP Server-Sent Events.
    Sse { url: String },
    /// Streamable HTTP (MCP 2025-03-26+).
    Http { url: String },
}

/// A2A (Agent-to-Agent) protocol configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct A2aConfig {
    /// Whether A2A is enabled.
    pub enabled: bool,
    /// Path to serve A2A endpoints (default: "/a2a").
    #[serde(default = "default_a2a_path")]
    pub listen_path: String,
    /// External A2A agents to connect to.
    #[serde(default)]
    pub external_agents: Vec<ExternalAgent>,
}

fn default_a2a_path() -> String {
    "/a2a".to_string()
}

/// An external A2A agent to discover and interact with.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalAgent {
    /// Display name.
    pub name: String,
    /// Agent endpoint URL.
    pub url: String,
}

fn default_language() -> String {
    "en".to_string()
}

fn default_true() -> bool {
    true
}

impl Default for KernelConfig {
    fn default() -> Self {
        let home_dir = openfang_home_dir();
        Self {
            data_dir: home_dir.join("data"),
            home_dir,
            log_level: "info".to_string(),
            api_listen: "127.0.0.1:50051".to_string(),
            network_enabled: false,
            default_model: DefaultModelConfig::default(),
            orchestrator_default_model: DefaultModelConfig {
                provider: String::new(),
                model: String::new(),
                api_key_env: String::new(),
                base_url: None,
            },
            memory_default_embedding: DefaultModelConfig {
                provider: String::new(),
                model: String::new(),
                api_key_env: String::new(),
                base_url: None,
            },
            memory: MemoryConfig::default(),
            network: NetworkConfig::default(),
            channels: ChannelsConfig::default(),
            api_key: String::new(),
            mode: KernelMode::default(),
            language: "en".to_string(),
            users: Vec::new(),
            mcp_servers: Vec::new(),
            a2a: None,
            usage_footer: UsageFooterMode::default(),
            web: WebConfig::default(),
            fallback_providers: Vec::new(),
            browser: BrowserConfig::default(),
            extensions: ExtensionsConfig::default(),
            vault: VaultConfig::default(),
            workspaces_dir: None,
            media: crate::media::MediaConfig::default(),
            links: crate::media::LinkConfig::default(),
            reload: ReloadConfig::default(),
            webhook_triggers: None,
            approval: crate::approval::ApprovalPolicy::default(),
            max_cron_jobs: default_max_cron_jobs(),
            include: Vec::new(),
            exec_policy: ExecPolicy::default(),
            bindings: Vec::new(),
            broadcast: BroadcastConfig::default(),
            auto_reply: AutoReplyConfig::default(),
            canvas: CanvasConfig::default(),
            tts: TtsConfig::default(),
            docker: DockerSandboxConfig::default(),
            pairing: PairingConfig::default(),
            auth_profiles: HashMap::new(),
            thinking: None,
            budget: BudgetConfig::default(),
            provider_urls: HashMap::new(),
            provider_api_keys: HashMap::new(),
            provider_enabled: HashMap::new(),
            oauth: OAuthConfig::default(),
            auth: AuthConfig::default(),
            conduits_dir: None,
            heartbeat: HeartbeatSettings::default(),
            automation: AutomationConfig::default(),
        }
    }
}

impl KernelConfig {
    /// Canonical path when `candidate` is under `automation.spoke_roots` or `automation.backlog_roots`.
    pub fn validate_automation_allowlisted_path(
        &self,
        candidate: &std::path::Path,
    ) -> Result<PathBuf, String> {
        validate_automation_allowlisted_path(
            &self.automation.spoke_roots,
            &self.automation.backlog_roots,
            candidate,
        )
    }

    /// `[automation].max_retries` (default 2).
    pub fn automation_max_retries(&self) -> u32 {
        self.automation.max_retries
    }

    /// `[automation].model_routing` lookup for a pipeline phase.
    pub fn automation_model_for_phase(&self, phase: &str) -> &str {
        self.automation.model_for_phase(phase)
    }

    /// Resolved workspaces root directory.
    pub fn effective_workspaces_dir(&self) -> PathBuf {
        self.workspaces_dir
            .clone()
            .unwrap_or_else(|| self.home_dir.join("workspaces"))
    }

    /// Resolve the API key env var name for a provider.
    ///
    /// Checks: 1) explicit `provider_api_keys` mapping, 2) `auth_profiles` first entry,
    /// 3) convention `{PROVIDER_UPPER}_API_KEY`.
    pub fn resolve_api_key_env(&self, provider: &str) -> String {
        // 1. Explicit mapping in [provider_api_keys]
        if let Some(env_var) = self.provider_api_keys.get(provider) {
            return env_var.clone();
        }
        // 2. Auth profiles (first profile by priority)
        if let Some(profiles) = self.auth_profiles.get(provider) {
            let mut sorted: Vec<_> = profiles.iter().collect();
            sorted.sort_by_key(|p| p.priority);
            if let Some(best) = sorted.first() {
                return best.api_key_env.clone();
            }
        }
        // 3. Convention: NVIDIA → NVIDIA_API_KEY
        format!("{}_API_KEY", provider.to_uppercase().replace('-', "_"))
    }
}

/// SECURITY: Custom Debug impl redacts sensitive fields (api_key).
impl std::fmt::Debug for KernelConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KernelConfig")
            .field("home_dir", &self.home_dir)
            .field("data_dir", &self.data_dir)
            .field("log_level", &self.log_level)
            .field("api_listen", &self.api_listen)
            .field("network_enabled", &self.network_enabled)
            .field("default_model", &self.default_model)
            .field("orchestrator_default_model", &self.orchestrator_default_model)
            .field("memory_default_embedding", &self.memory_default_embedding)
            .field("memory", &self.memory)
            .field("network", &self.network)
            .field("channels", &self.channels)
            .field(
                "api_key",
                &if self.api_key.is_empty() {
                    "<empty>"
                } else {
                    "<redacted>"
                },
            )
            .field("mode", &self.mode)
            .field("language", &self.language)
            .field("users", &format!("{} user(s)", self.users.len()))
            .field(
                "mcp_servers",
                &format!("{} server(s)", self.mcp_servers.len()),
            )
            .field("a2a", &self.a2a.as_ref().map(|a| a.enabled))
            .field("usage_footer", &self.usage_footer)
            .field("web", &self.web)
            .field(
                "fallback_providers",
                &format!("{} provider(s)", self.fallback_providers.len()),
            )
            .field("browser", &self.browser)
            .field("extensions", &self.extensions)
            .field("vault", &format!("enabled={}", self.vault.enabled))
            .field("workspaces_dir", &self.workspaces_dir)
            .field(
                "media",
                &format!(
                    "image={} audio={} video={}",
                    self.media.image_description,
                    self.media.audio_transcription,
                    self.media.video_description
                ),
            )
            .field("links", &format!("enabled={}", self.links.enabled))
            .field("reload", &self.reload.mode)
            .field(
                "webhook_triggers",
                &self.webhook_triggers.as_ref().map(|w| w.enabled),
            )
            .field(
                "approval",
                &format!("{} tool(s)", self.approval.require_approval.len()),
            )
            .field("max_cron_jobs", &self.max_cron_jobs)
            .field("include", &format!("{} file(s)", self.include.len()))
            .field("exec_policy", &self.exec_policy.mode)
            .field("bindings", &format!("{} binding(s)", self.bindings.len()))
            .field(
                "broadcast",
                &format!("{} route(s)", self.broadcast.routes.len()),
            )
            .field(
                "auto_reply",
                &format!("enabled={}", self.auto_reply.enabled),
            )
            .field("canvas", &format!("enabled={}", self.canvas.enabled))
            .field("tts", &format!("enabled={}", self.tts.enabled))
            .field("docker", &format!("enabled={}", self.docker.enabled))
            .field("pairing", &format!("enabled={}", self.pairing.enabled))
            .field(
                "auth_profiles",
                &format!("{} provider(s)", self.auth_profiles.len()),
            )
            .field("thinking", &self.thinking.is_some())
            .field(
                "provider_api_keys",
                &format!("{} mapping(s)", self.provider_api_keys.len()),
            )
            .field(
                "provider_enabled",
                &format!("{} provider(s)", self.provider_enabled.len()),
            )
            .field("auth", &format!("enabled={}", self.auth.enabled))
            .field(
                "automation",
                &format!(
                    "{} spoke root(s), {} backlog root(s), max_retries={}, {} model route(s)",
                    self.automation.spoke_roots.len(),
                    self.automation.backlog_roots.len(),
                    self.automation.max_retries,
                    self.automation.model_routing.len()
                ),
            )
            .finish()
    }
}

/// Resolve the OpenFang home directory.
///
/// Priority: `OPENFANG_HOME` env var > `~/.openfang`.
fn openfang_home_dir() -> PathBuf {
    if let Ok(home) = std::env::var("OPENFANG_HOME") {
        return PathBuf::from(home);
    }
    dirs::home_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(".openfang")
}

/// Default LLM model configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DefaultModelConfig {
    /// Provider name (e.g., "anthropic", "openai").
    pub provider: String,
    /// Model identifier.
    pub model: String,
    /// Environment variable name for the API key.
    pub api_key_env: String,
    /// Optional base URL override.
    pub base_url: Option<String>,
}

impl Default for DefaultModelConfig {
    fn default() -> Self {
        Self {
            provider: "anthropic".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
            api_key_env: "ANTHROPIC_API_KEY".to_string(),
            base_url: None,
        }
    }
}

/// Memory substrate configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MemoryConfig {
    /// Path to SQLite database file.
    pub sqlite_path: Option<PathBuf>,
    /// Embedding model for semantic search.
    pub embedding_model: String,
    /// Maximum memories before consolidation is triggered.
    pub consolidation_threshold: u64,
    /// Memory decay rate (0.0 = no decay, 1.0 = aggressive decay).
    pub decay_rate: f32,
    /// Embedding provider (e.g., "openai", "gemini", "ollama"). None = auto-detect
    /// (`GOOGLE_API_KEY` / `GEMINI_API_KEY` → Gemini embeddings before probing local Ollama).
    #[serde(default)]
    pub embedding_provider: Option<String>,
    /// Environment variable name for the embedding API key.
    #[serde(default)]
    pub embedding_api_key_env: Option<String>,
    /// How often to run memory consolidation (hours). 0 = disabled.
    #[serde(default = "default_consolidation_interval")]
    pub consolidation_interval_hours: u64,
    /// Memory backend: "sqlite" (default) or "http".
    #[serde(default = "default_memory_backend")]
    pub backend: String,
    /// HTTP memory API URL (when backend = "http").
    /// e.g., "http://127.0.0.1:5500"
    #[serde(default)]
    pub http_url: Option<String>,
    /// Env var name holding the HTTP memory API bearer token.
    #[serde(default)]
    pub http_token_env: Option<String>,
}

fn default_consolidation_interval() -> u64 {
    24
}

fn default_memory_backend() -> String {
    "sqlite".to_string()
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            sqlite_path: None,
            embedding_model: "all-MiniLM-L6-v2".to_string(),
            consolidation_threshold: 10_000,
            decay_rate: 0.1,
            embedding_provider: None,
            embedding_api_key_env: None,
            consolidation_interval_hours: default_consolidation_interval(),
            backend: default_memory_backend(),
            http_url: None,
            http_token_env: None,
        }
    }
}

/// Network layer configuration.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkConfig {
    /// libp2p listen addresses.
    pub listen_addresses: Vec<String>,
    /// Bootstrap peers for DHT.
    pub bootstrap_peers: Vec<String>,
    /// Enable mDNS for local discovery.
    pub mdns_enabled: bool,
    /// Maximum number of connected peers.
    pub max_peers: u32,
    /// Pre-shared secret for OFP HMAC authentication (required when network is enabled).
    pub shared_secret: String,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            listen_addresses: vec!["/ip4/0.0.0.0/tcp/0".to_string()],
            bootstrap_peers: vec![],
            mdns_enabled: true,
            max_peers: 50,
            shared_secret: String::new(),
        }
    }
}

/// SECURITY: Custom Debug impl redacts sensitive fields (shared_secret).
impl std::fmt::Debug for NetworkConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetworkConfig")
            .field("listen_addresses", &self.listen_addresses)
            .field("bootstrap_peers", &self.bootstrap_peers)
            .field("mdns_enabled", &self.mdns_enabled)
            .field("max_peers", &self.max_peers)
            .field(
                "shared_secret",
                &if self.shared_secret.is_empty() {
                    "<empty>"
                } else {
                    "<redacted>"
                },
            )
            .finish()
    }
}

/// Channel bridge configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ChannelsConfig {
    /// Signal (via signal-cli) configuration (None = disabled).
    pub signal: Option<SignalConfig>,
    /// Mattermost configuration (None = disabled).
    pub mattermost: Option<MattermostConfig>,
}

/// Signal channel adapter configuration (via signal-cli REST API).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SignalConfig {
    /// URL of the signal-cli REST API (e.g., "http://localhost:8080").
    pub api_url: String,
    /// Registered phone number.
    pub phone_number: String,
    /// Allowed phone numbers (empty = allow all).
    #[serde(default, deserialize_with = "deserialize_string_or_int_vec")]
    pub allowed_users: Vec<String>,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for SignalConfig {
    fn default() -> Self {
        Self {
            api_url: "http://localhost:8080".to_string(),
            phone_number: String::new(),
            allowed_users: vec![],
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

/// Mattermost channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MattermostConfig {
    /// Mattermost server URL (e.g., `"https://mattermost.example.com"`).
    pub server_url: String,
    /// Env var name holding the bot token.
    pub token_env: String,
    /// Allowed channel IDs (empty = all).
    #[serde(default, deserialize_with = "deserialize_string_or_int_vec")]
    pub allowed_channels: Vec<String>,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for MattermostConfig {
    fn default() -> Self {
        Self {
            server_url: String::new(),
            token_env: "MATTERMOST_TOKEN".to_string(),
            allowed_channels: vec![],
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

impl KernelConfig {
    /// Validate the configuration, returning a list of warnings.
    ///
    /// Checks that env vars referenced by configured channels are set.
    pub fn validate(&self) -> Vec<String> {
        let mut warnings = Vec::new();

        if let Some(ref s) = self.channels.signal {
            if s.api_url.trim().is_empty() {
                warnings.push("Signal configured but api_url is empty".to_string());
            }
            if s.phone_number.trim().is_empty() {
                warnings.push("Signal configured but phone_number is empty".to_string());
            }
        }
        if let Some(ref m) = self.channels.mattermost {
            if std::env::var(&m.token_env).unwrap_or_default().is_empty() {
                warnings.push(format!(
                    "Mattermost configured but {} is not set",
                    m.token_env
                ));
            }
        }

        // Web search provider validation
        match self.web.search_provider {
            SearchProvider::Brave => {
                if std::env::var(&self.web.brave.api_key_env)
                    .unwrap_or_default()
                    .is_empty()
                {
                    warnings.push(format!(
                        "Brave search selected but {} is not set",
                        self.web.brave.api_key_env
                    ));
                }
            }
            SearchProvider::Tavily => {
                if std::env::var(&self.web.tavily.api_key_env)
                    .unwrap_or_default()
                    .is_empty()
                {
                    warnings.push(format!(
                        "Tavily search selected but {} is not set",
                        self.web.tavily.api_key_env
                    ));
                }
            }
            SearchProvider::Perplexity => {
                if std::env::var(&self.web.perplexity.api_key_env)
                    .unwrap_or_default()
                    .is_empty()
                {
                    warnings.push(format!(
                        "Perplexity search selected but {} is not set",
                        self.web.perplexity.api_key_env
                    ));
                }
            }
            SearchProvider::Searxng => {
                if self.web.searxng.url.is_empty() {
                    warnings.push(
                        "Searxng search selected but searxng.url is not configured".to_string(),
                    );
                }
            }
            SearchProvider::DuckDuckGo | SearchProvider::Auto => {}
        }

        // --- Production bounds validation ---
        // Clamp dangerous zero/extreme values to safe defaults instead of crashing.
        warnings
    }

    /// Clamp configuration values to safe production bounds.
    ///
    /// Called after loading config to prevent zero timeouts, unbounded buffers,
    /// or other misconfigurations that cause silent failures at runtime.
    pub fn clamp_bounds(&mut self) {
        // Browser timeout: min 5s, max 300s
        if self.browser.timeout_secs == 0 {
            self.browser.timeout_secs = 30;
        } else if self.browser.timeout_secs > 300 {
            self.browser.timeout_secs = 300;
        }

        // Browser max sessions: min 1, max 100
        if self.browser.max_sessions == 0 {
            self.browser.max_sessions = 3;
        } else if self.browser.max_sessions > 100 {
            self.browser.max_sessions = 100;
        }

        // Web fetch max_response_bytes: min 1KB, max 50MB
        if self.web.fetch.max_response_bytes == 0 {
            self.web.fetch.max_response_bytes = 5_000_000;
        } else if self.web.fetch.max_response_bytes > 50_000_000 {
            self.web.fetch.max_response_bytes = 50_000_000;
        }

        // Web fetch timeout: min 5s, max 120s
        if self.web.fetch.timeout_secs == 0 {
            self.web.fetch.timeout_secs = 30;
        } else if self.web.fetch.timeout_secs > 120 {
            self.web.fetch.timeout_secs = 120;
        }

        if self.automation.project_context_max_failures == 0 {
            self.automation.project_context_max_failures = default_project_context_max_failures();
        } else if self.automation.project_context_max_failures > 500 {
            self.automation.project_context_max_failures = 500;
        }
        if self.automation.project_context_decision_max_age_days > 3650 {
            self.automation.project_context_decision_max_age_days = 3650;
        }
        if self.automation.project_context_prompt_max_chars == 0 {
            self.automation.project_context_prompt_max_chars =
                default_project_context_prompt_max_chars();
        } else if self.automation.project_context_prompt_max_chars > 100_000 {
            self.automation.project_context_prompt_max_chars = 100_000;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_default_config() {
        let config = KernelConfig::default();
        assert_eq!(config.log_level, "info");
        assert_eq!(config.api_listen, "127.0.0.1:50051");
        assert!(!config.network_enabled);
    }

    #[test]
    fn test_config_serialization() {
        let config = KernelConfig::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        assert!(toml_str.contains("log_level"));
    }

    #[test]
    fn test_validate_no_channels() {
        let config = KernelConfig::default();
        let warnings = config.validate();
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_kernel_mode_default() {
        let mode = KernelMode::default();
        assert_eq!(mode, KernelMode::Default);
    }

    #[test]
    fn test_kernel_mode_serde() {
        let stable = KernelMode::Stable;
        let json = serde_json::to_string(&stable).unwrap();
        assert_eq!(json, "\"stable\"");
        let back: KernelMode = serde_json::from_str(&json).unwrap();
        assert_eq!(back, KernelMode::Stable);
    }

    #[test]
    fn test_user_config_serde() {
        let uc = UserConfig {
            name: "Alice".to_string(),
            role: "owner".to_string(),
            channel_bindings: {
                let mut m = std::collections::HashMap::new();
                m.insert("signal".to_string(), "123456".to_string());
                m
            },
            api_key_hash: None,
        };
        let json = serde_json::to_string(&uc).unwrap();
        let back: UserConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "Alice");
        assert_eq!(back.role, "owner");
        assert_eq!(back.channel_bindings.get("signal").unwrap(), "123456");
    }

    #[test]
    fn test_config_with_mode_and_language() {
        let config = KernelConfig {
            mode: KernelMode::Stable,
            language: "ar".to_string(),
            ..Default::default()
        };
        assert_eq!(config.mode, KernelMode::Stable);
        assert_eq!(config.language, "ar");
    }

    #[test]
    fn test_validate_mattermost_missing_token_env() {
        let mut config = KernelConfig::default();
        config.channels.mattermost = Some(MattermostConfig {
            token_env: "OPENFANG_TEST_NONEXISTENT_MM".to_string(),
            ..Default::default()
        });
        let warnings = config.validate();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("Mattermost"));
    }

    #[test]
    fn test_validate_signal_empty_required_fields() {
        let mut config = KernelConfig::default();
        config.channels.signal = Some(SignalConfig {
            api_url: String::new(),
            phone_number: String::new(),
            ..Default::default()
        });
        let warnings = config.validate();
        assert_eq!(warnings.len(), 2);
        assert!(warnings.iter().any(|w| w.contains("api_url")));
        assert!(warnings.iter().any(|w| w.contains("phone_number")));
    }

    #[test]
    fn test_signal_config_defaults() {
        let sig = SignalConfig::default();
        assert_eq!(sig.api_url, "http://localhost:8080");
        assert!(sig.phone_number.is_empty());
    }

    #[test]
    fn test_channels_config_signal_mattermost() {
        let config = KernelConfig {
            channels: ChannelsConfig {
                signal: Some(SignalConfig {
                    phone_number: "+1".to_string(),
                    ..Default::default()
                }),
                mattermost: Some(MattermostConfig::default()),
            },
            ..Default::default()
        };
        assert!(config.channels.signal.is_some());
        assert!(config.channels.mattermost.is_some());
    }

    #[test]
    fn test_mattermost_config_defaults() {
        let m = MattermostConfig::default();
        assert_eq!(m.token_env, "MATTERMOST_TOKEN");
        assert!(m.server_url.is_empty());
    }

    #[test]
    fn test_channels_config_toml_roundtrip() {
        let config = KernelConfig {
            channels: ChannelsConfig {
                signal: Some(SignalConfig {
                    phone_number: "+15551212".to_string(),
                    ..Default::default()
                }),
                mattermost: Some(MattermostConfig::default()),
            },
            ..Default::default()
        };
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let back: KernelConfig = toml::from_str(&toml_str).unwrap();
        assert!(back.channels.signal.is_some());
        assert!(back.channels.mattermost.is_some());
    }

    #[test]
    fn test_channel_overrides_defaults() {
        let ov = ChannelOverrides::default();
        assert_eq!(ov.dm_policy, DmPolicy::Respond);
        assert_eq!(ov.group_policy, GroupPolicy::MentionOnly);
        assert_eq!(ov.rate_limit_per_user, 0);
        assert!(!ov.threading);
        assert!(ov.output_format.is_none());
        assert!(ov.model.is_none());
        assert!(ov.lifecycle_reactions);
    }

    #[test]
    fn test_fallback_config_serde_roundtrip() {
        let fb = FallbackProviderConfig {
            provider: "ollama".to_string(),
            model: "llama3.2:latest".to_string(),
            api_key_env: String::new(),
            base_url: None,
        };
        let json = serde_json::to_string(&fb).unwrap();
        let back: FallbackProviderConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.provider, "ollama");
        assert_eq!(back.model, "llama3.2:latest");
        assert!(back.api_key_env.is_empty());
        assert!(back.base_url.is_none());
    }

    #[test]
    fn test_fallback_config_default_empty() {
        let config = KernelConfig::default();
        assert!(config.fallback_providers.is_empty());
    }

    #[test]
    fn test_fallback_config_in_toml() {
        let toml_str = r#"
            [[fallback_providers]]
            provider = "ollama"
            model = "llama3.2:latest"

            [[fallback_providers]]
            provider = "groq"
            model = "llama-3.3-70b-versatile"
            api_key_env = "GROQ_API_KEY"
        "#;
        let config: KernelConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.fallback_providers.len(), 2);
        assert_eq!(config.fallback_providers[0].provider, "ollama");
        assert_eq!(config.fallback_providers[1].provider, "groq");
    }

    #[test]
    fn test_channel_overrides_serde() {
        let ov = ChannelOverrides {
            dm_policy: DmPolicy::Ignore,
            group_policy: GroupPolicy::CommandsOnly,
            rate_limit_per_user: 10,
            threading: true,
            output_format: Some(OutputFormat::TelegramHtml),
            ..Default::default()
        };
        let json = serde_json::to_string(&ov).unwrap();
        let back: ChannelOverrides = serde_json::from_str(&json).unwrap();
        assert_eq!(back.dm_policy, DmPolicy::Ignore);
        assert_eq!(back.group_policy, GroupPolicy::CommandsOnly);
        assert_eq!(back.rate_limit_per_user, 10);
        assert!(back.threading);
        assert_eq!(back.output_format, Some(OutputFormat::TelegramHtml));
        // lifecycle_reactions defaults to true via ..Default::default()
        assert!(back.lifecycle_reactions);
    }

    #[test]
    fn test_channel_overrides_lifecycle_reactions_disabled() {
        let json = r#"{"lifecycle_reactions": false}"#;
        let ov: ChannelOverrides = serde_json::from_str(json).unwrap();
        assert!(!ov.lifecycle_reactions);
        // Other fields should have their defaults
        assert_eq!(ov.dm_policy, DmPolicy::Respond);
        assert!(ov.model.is_none());
    }

    #[test]
    fn test_channel_overrides_lifecycle_reactions_missing_defaults_true() {
        let json = r#"{}"#;
        let ov: ChannelOverrides = serde_json::from_str(json).unwrap();
        assert!(ov.lifecycle_reactions);
    }

    #[test]
    fn test_clamp_bounds_zero_browser_timeout() {
        let mut config = KernelConfig::default();
        config.browser.timeout_secs = 0;
        config.clamp_bounds();
        assert_eq!(config.browser.timeout_secs, 30);
    }

    #[test]
    fn test_clamp_bounds_excessive_browser_sessions() {
        let mut config = KernelConfig::default();
        config.browser.max_sessions = 999;
        config.clamp_bounds();
        assert_eq!(config.browser.max_sessions, 100);
    }

    #[test]
    fn test_clamp_bounds_zero_fetch_bytes() {
        let mut config = KernelConfig::default();
        config.web.fetch.max_response_bytes = 0;
        config.clamp_bounds();
        assert_eq!(config.web.fetch.max_response_bytes, 5_000_000);
    }

    #[test]
    fn test_clamp_bounds_zero_fetch_timeout() {
        let mut config = KernelConfig::default();
        config.web.fetch.timeout_secs = 0;
        config.clamp_bounds();
        assert_eq!(config.web.fetch.timeout_secs, 30);
    }

    #[test]
    fn test_clamp_bounds_defaults_unchanged() {
        let mut config = KernelConfig::default();
        let browser_timeout = config.browser.timeout_secs;
        let browser_sessions = config.browser.max_sessions;
        let fetch_bytes = config.web.fetch.max_response_bytes;
        let fetch_timeout = config.web.fetch.timeout_secs;
        config.clamp_bounds();
        assert_eq!(config.browser.timeout_secs, browser_timeout);
        assert_eq!(config.browser.max_sessions, browser_sessions);
        assert_eq!(config.web.fetch.max_response_bytes, fetch_bytes);
        assert_eq!(config.web.fetch.timeout_secs, fetch_timeout);
    }

    #[test]
    fn test_resolve_api_key_env_convention() {
        let config = KernelConfig::default();
        // Unknown provider falls back to convention
        assert_eq!(config.resolve_api_key_env("nvidia"), "NVIDIA_API_KEY");
        assert_eq!(config.resolve_api_key_env("my-custom"), "MY_CUSTOM_API_KEY");
    }

    #[test]
    fn test_resolve_api_key_env_explicit_mapping() {
        let mut config = KernelConfig::default();
        config
            .provider_api_keys
            .insert("nvidia".to_string(), "NIM_KEY".to_string());
        // Explicit mapping takes precedence over convention
        assert_eq!(config.resolve_api_key_env("nvidia"), "NIM_KEY");
    }

    #[test]
    fn test_resolve_api_key_env_auth_profiles() {
        let mut config = KernelConfig::default();
        config.auth_profiles.insert(
            "nvidia".to_string(),
            vec![AuthProfile {
                name: "primary".to_string(),
                api_key_env: "NVIDIA_PRIMARY_KEY".to_string(),
                priority: 0,
            }],
        );
        // Auth profiles take precedence over convention (but not explicit mapping)
        assert_eq!(config.resolve_api_key_env("nvidia"), "NVIDIA_PRIMARY_KEY");
    }

    #[test]
    fn test_resolve_api_key_env_explicit_over_auth_profile() {
        let mut config = KernelConfig::default();
        config
            .provider_api_keys
            .insert("nvidia".to_string(), "NIM_KEY".to_string());
        config.auth_profiles.insert(
            "nvidia".to_string(),
            vec![AuthProfile {
                name: "primary".to_string(),
                api_key_env: "NVIDIA_PRIMARY_KEY".to_string(),
                priority: 0,
            }],
        );
        // Explicit mapping wins over auth profiles
        assert_eq!(config.resolve_api_key_env("nvidia"), "NIM_KEY");
    }

    #[test]
    fn test_provider_api_keys_toml_roundtrip() {
        let toml_str = r#"
            [provider_api_keys]
            nvidia = "NVIDIA_NIM_KEY"
            azure = "AZURE_OPENAI_KEY"
        "#;
        let config: KernelConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.provider_api_keys.len(), 2);
        assert_eq!(
            config.provider_api_keys.get("nvidia").unwrap(),
            "NVIDIA_NIM_KEY"
        );
        assert_eq!(
            config.provider_api_keys.get("azure").unwrap(),
            "AZURE_OPENAI_KEY"
        );
    }

    #[test]
    fn test_heartbeat_settings_default() {
        let settings = HeartbeatSettings::default();
        assert_eq!(settings.default_timeout_secs, 180);
    }

    #[test]
    fn test_heartbeat_settings_deserialization() {
        let toml_str = r#"default_timeout_secs = 600"#;
        let settings: HeartbeatSettings = toml::from_str(toml_str).unwrap();
        assert_eq!(settings.default_timeout_secs, 600);
    }

    #[test]
    fn test_heartbeat_settings_omitted_uses_default() {
        // When [heartbeat] section is omitted entirely, KernelConfig should use defaults
        let toml_str = r#"
            log_level = "debug"
        "#;
        let config: KernelConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.heartbeat.default_timeout_secs, 180);
    }

    #[test]
    fn test_heartbeat_settings_in_kernel_config() {
        let toml_str = r#"
            [heartbeat]
            default_timeout_secs = 300
        "#;
        let config: KernelConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.heartbeat.default_timeout_secs, 300);
    }

    #[test]
    fn test_automation_spoke_roots_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let mut config = KernelConfig::default();
        config.automation.spoke_roots = vec![root.clone()];
        let toml_str = toml::to_string(&config).unwrap();
        let back: KernelConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(back.automation.spoke_roots, vec![root]);
    }

    #[test]
    fn test_automation_max_retries_default() {
        assert_eq!(AutomationConfig::default().max_retries, 2);
        assert_eq!(KernelConfig::default().automation_max_retries(), 2);
    }

    #[test]
    fn test_automation_max_retries_in_toml() {
        let toml_str = r#"
            [automation]
            max_retries = 5
        "#;
        let config: KernelConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.automation.max_retries, 5);
        assert_eq!(config.automation_max_retries(), 5);
    }

    #[test]
    fn test_automation_max_retries_omitted_in_section() {
        let toml_str = r#"
            [automation]
        "#;
        let config: KernelConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.automation.max_retries, 2);
    }

    #[test]
    fn test_automation_project_context_toml_overrides() {
        let toml_str = r#"
            [automation]
            project_context_max_failures = 12
            project_context_decision_max_age_days = 7
            project_context_prompt_max_chars = 2048
        "#;
        let config: KernelConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.automation.project_context_max_failures, 12);
        assert_eq!(config.automation.project_context_decision_max_age_days, 7);
        assert_eq!(config.automation.project_context_prompt_max_chars, 2048);
    }

    #[test]
    fn test_automation_model_routing_defaults() {
        let a = AutomationConfig::default();
        assert_eq!(a.model_for_phase("planning"), "premium");
        assert_eq!(a.model_for_phase("implementation"), "default");
        assert_eq!(a.model_for_phase("retry"), "default");
        assert_eq!(a.model_for_phase("unknown"), "default");
        assert_eq!(
            KernelConfig::default().automation_model_for_phase("planning"),
            "premium"
        );
    }

    #[test]
    fn test_automation_model_routing_toml_override() {
        let toml_str = r#"
            [automation.model_routing]
            planning = "claude-opus"
            implementation = "groq-fast"
        "#;
        let config: KernelConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.automation_model_for_phase("planning"), "claude-opus");
        assert_eq!(
            config.automation_model_for_phase("implementation"),
            "groq-fast"
        );
        assert_eq!(config.automation_model_for_phase("retry"), "default");
    }

    #[test]
    fn test_validate_spoke_root_allowlisted_accepts_child() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let child = dir.path().join("nested");
        std::fs::create_dir_all(&child).unwrap();
        let allow = vec![root.clone()];
        let got = validate_spoke_root_allowlisted(&allow, child.as_path()).unwrap();
        assert_eq!(got, child.canonicalize().unwrap());
    }

    #[test]
    fn test_validate_spoke_root_allowlisted_rejects_outside() {
        let dir = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let allow = vec![root];
        let err = validate_spoke_root_allowlisted(&allow, other.path()).unwrap_err();
        assert!(err.contains("not under any path"));
    }

    #[test]
    fn test_validate_spoke_root_allowlisted_empty_list() {
        let dir = tempfile::tempdir().unwrap();
        let err = validate_spoke_root_allowlisted(&[], dir.path()).unwrap_err();
        assert!(err.contains("No automation spoke roots"));
    }

    #[test]
    fn test_validate_spoke_root_not_absolute() {
        let err = validate_spoke_root_allowlisted(&[PathBuf::from("/tmp")], Path::new("relative"))
            .unwrap_err();
        assert!(err.contains("absolute"));
    }

    #[test]
    fn test_validate_backlog_root_allowlisted_accepts_child() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let child = dir.path().join("nested");
        std::fs::create_dir_all(&child).unwrap();
        let allow = vec![root.clone()];
        let got = validate_backlog_root_allowlisted(&allow, child.as_path()).unwrap();
        assert_eq!(got, child.canonicalize().unwrap());
    }

    #[test]
    fn test_resolve_automation_backlog_cwd_single_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let got = resolve_automation_backlog_cwd(std::slice::from_ref(&root), None).unwrap();
        assert_eq!(got, root);
    }

    #[test]
    fn test_resolve_automation_backlog_cwd_requires_param_when_multiple() {
        let a = tempfile::tempdir().unwrap().path().canonicalize().unwrap();
        let b = tempfile::tempdir().unwrap().path().canonicalize().unwrap();
        let err = resolve_automation_backlog_cwd(&[a, b], None).unwrap_err();
        assert!(err.contains("Multiple"));
    }

    #[test]
    fn test_validate_automation_allowlisted_path_accepts_spoke_or_backlog() {
        let spoke = tempfile::tempdir().unwrap();
        let backlog = tempfile::tempdir().unwrap();
        let spoke_root = spoke.path().canonicalize().unwrap();
        let backlog_root = backlog.path().canonicalize().unwrap();
        let child_spoke = spoke.path().join("nested");
        std::fs::create_dir_all(&child_spoke).unwrap();
        let child_bl = backlog.path().join("docs");
        std::fs::create_dir_all(&child_bl).unwrap();

        let got = validate_automation_allowlisted_path(
            std::slice::from_ref(&spoke_root),
            std::slice::from_ref(&backlog_root),
            child_spoke.as_path(),
        )
        .unwrap();
        assert_eq!(got, child_spoke.canonicalize().unwrap());

        let got2 = validate_automation_allowlisted_path(
            std::slice::from_ref(&spoke_root),
            std::slice::from_ref(&backlog_root),
            child_bl.as_path(),
        )
        .unwrap();
        assert_eq!(got2, child_bl.canonicalize().unwrap());
    }

    #[test]
    fn test_validate_automation_allowlisted_path_rejects_outside() {
        let spoke = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let spoke_root = spoke.path().canonicalize().unwrap();
        let err = validate_automation_allowlisted_path(
            std::slice::from_ref(&spoke_root),
            &[],
            other.path(),
        )
        .unwrap_err();
        assert!(err.contains("not under any"));
    }

    #[test]
    fn test_validate_automation_allowlisted_path_both_lists_empty() {
        let dir = tempfile::tempdir().unwrap();
        let err = validate_automation_allowlisted_path(&[], &[], dir.path()).unwrap_err();
        assert!(err.contains("No automation paths configured"));
    }

    #[test]
    fn test_validate_automation_allowlisted_path_requires_absolute() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        let err = validate_automation_allowlisted_path(
            std::slice::from_ref(&root),
            &[],
            Path::new("relative"),
        )
        .unwrap_err();
        assert!(err.contains("absolute"));
    }

    #[test]
    fn test_kernel_config_validate_automation_allowlisted_path() {
        let mut config = KernelConfig::default();
        let base = tempfile::tempdir().unwrap();
        let root = base.path().canonicalize().unwrap();
        let nested = base.path().join("sub");
        std::fs::create_dir_all(&nested).unwrap();
        config.automation.backlog_roots = vec![root];
        let got = config
            .validate_automation_allowlisted_path(nested.as_path())
            .unwrap();
        assert_eq!(got, nested.canonicalize().unwrap());
    }
}
