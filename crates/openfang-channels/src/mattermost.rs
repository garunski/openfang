//! Mattermost channel adapter for the OpenFang channel bridge.
//!
//! Uses the Mattermost WebSocket API v4 for real-time message reception and the
//! REST API v4 for sending messages. No external Mattermost crate — just
//! `tokio-tungstenite` + `reqwest`.

use crate::types::{
    split_message, ChannelAdapter, ChannelContent, ChannelMessage, ChannelType, ChannelUser,
};
use async_trait::async_trait;
use chrono::Utc;
use futures::{SinkExt, Stream, StreamExt};
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch, RwLock};
use tracing::{debug, info, warn};
use zeroize::Zeroizing;

/// Maximum Mattermost message length (characters). The server limit is 16383.
const MAX_MESSAGE_LEN: usize = 16383;
const MAX_BACKOFF: Duration = Duration::from_secs(60);
const INITIAL_BACKOFF: Duration = Duration::from_secs(1);

/// Mattermost WebSocket + REST API v4 adapter.
///
/// Inbound messages arrive via WebSocket events (`posted`).
/// Outbound messages are sent via `POST /api/v4/posts`.
pub struct MattermostAdapter {
    /// Mattermost server URL (e.g., `"https://mattermost.example.com"`).
    server_url: String,
    /// SECURITY: Auth token is zeroized on drop to prevent memory disclosure.
    token: Zeroizing<String>,
    /// Restrict to specific channel IDs (empty = all channels the bot is in).
    allowed_channels: Vec<String>,
    /// HTTP client for outbound REST API calls.
    client: reqwest::Client,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
    /// Bot's own user ID (populated after /api/v4/users/me).
    bot_user_id: Arc<RwLock<Option<String>>>,
}

impl MattermostAdapter {
    /// Create a new Mattermost adapter.
    ///
    /// * `server_url` — Base Mattermost server URL (no trailing slash).
    /// * `token` — Personal access token or bot token.
    /// * `allowed_channels` — Channel IDs to listen on (empty = all).
    pub fn new(server_url: String, token: String, allowed_channels: Vec<String>) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            server_url: server_url.trim_end_matches('/').to_string(),
            token: Zeroizing::new(token),
            allowed_channels,
            client: reqwest::Client::new(),
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            bot_user_id: Arc::new(RwLock::new(None)),
        }
    }

    /// Validate the token by calling `GET /api/v4/users/me`.
    async fn validate_token(&self) -> Result<String, Box<dyn std::error::Error>> {
        let url = format!("{}/api/v4/users/me", self.server_url);
        let resp = self
            .client
            .get(&url)
            .bearer_auth(self.token.as_str())
            .send()
            .await?;

        let status = resp.status();
        let raw = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(format!("Mattermost auth failed {status}: {raw}").into());
        }

        let body: serde_json::Value = serde_json::from_str(raw.trim()).map_err(|e| {
            let preview: String = raw.chars().take(240).collect();
            format!(
                "Mattermost /users/me returned {status} but the body is not valid JSON ({e}). \
                 Often this means server_url points at a reverse proxy or non-Mattermost page (HTML) \
                 instead of the Mattermost API root. Preview: {preview:?}"
            )
        })?;
        let user_id = body["id"].as_str().unwrap_or("unknown").to_string();
        let username = body["username"].as_str().unwrap_or("unknown");
        info!("Mattermost authenticated as {username} ({user_id})");

        Ok(user_id)
    }

    /// Build the WebSocket URL for the Mattermost API v4.
    fn ws_url(&self) -> String {
        let base = if self.server_url.starts_with("https://") {
            self.server_url.replacen("https://", "wss://", 1)
        } else if self.server_url.starts_with("http://") {
            self.server_url.replacen("http://", "ws://", 1)
        } else {
            format!("wss://{}", self.server_url)
        };
        format!("{base}/api/v4/websocket")
    }

    /// Send a text message to a Mattermost channel via REST API.
    async fn api_send_message(
        &self,
        channel_id: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/api/v4/posts", self.server_url);
        let chunks = split_message(text, MAX_MESSAGE_LEN);

        for chunk in chunks {
            let body = serde_json::json!({
                "channel_id": channel_id,
                "message": chunk,
            });

            let resp = self
                .client
                .post(&url)
                .bearer_auth(self.token.as_str())
                .json(&body)
                .send()
                .await?;

            if !resp.status().is_success() {
                let status = resp.status();
                let resp_body = resp.text().await.unwrap_or_default();
                warn!("Mattermost sendMessage failed {status}: {resp_body}");
            }
        }

        Ok(())
    }

    /// Check whether a channel ID is allowed (empty list = allow all).
    #[allow(dead_code)]
    fn is_allowed_channel(&self, channel_id: &str) -> bool {
        self.allowed_channels.is_empty() || self.allowed_channels.iter().any(|c| c == channel_id)
    }
}

/// Parse a Mattermost WebSocket `posted` event into a `ChannelMessage`.
///
/// The `data` field of a `posted` event contains a JSON string under `post`
/// which holds the actual post payload.
fn parse_mattermost_event(
    event: &serde_json::Value,
    bot_user_id: &Option<String>,
    allowed_channels: &[String],
) -> Option<ChannelMessage> {
    let event_type = event["event"].as_str().unwrap_or("");
    if event_type != "posted" {
        return None;
    }

    // The `data.post` field is a JSON string that needs a second parse
    let post_str = event["data"]["post"].as_str()?;
    let post: serde_json::Value = serde_json::from_str(post_str).ok()?;

    let user_id = post["user_id"].as_str().unwrap_or("");
    let channel_id = post["channel_id"].as_str().unwrap_or("");
    let message = post["message"].as_str().unwrap_or("");
    let post_id = post["id"].as_str().unwrap_or("").to_string();

    // Skip messages from the bot itself
    if let Some(ref bid) = bot_user_id {
        if user_id == bid {
            return None;
        }
    }

    // Filter by allowed channels
    if !allowed_channels.is_empty() && !allowed_channels.iter().any(|c| c == channel_id) {
        return None;
    }

    if message.is_empty() {
        return None;
    }

    // Determine if group conversation from channel_type in event data
    let channel_type = event["data"]["channel_type"].as_str().unwrap_or("");
    let is_group = channel_type != "D"; // "D" = direct message

    // Extract thread root id if this is a threaded reply
    let root_id = post["root_id"].as_str().unwrap_or("");
    let thread_id = if root_id.is_empty() {
        None
    } else {
        Some(root_id.to_string())
    };

    // Extract sender display name from event data
    let sender_name = event["data"]["sender_name"].as_str().unwrap_or(user_id);

    // Parse commands (messages starting with /)
    let content = if message.starts_with('/') {
        let parts: Vec<&str> = message.splitn(2, ' ').collect();
        let cmd_name = &parts[0][1..];
        let args = if parts.len() > 1 {
            parts[1].split_whitespace().map(String::from).collect()
        } else {
            vec![]
        };
        ChannelContent::Command {
            name: cmd_name.to_string(),
            args,
        }
    } else {
        ChannelContent::Text(message.to_string())
    };

    let mut metadata = HashMap::new();
    metadata.insert(
        "mattermost_channel_id".to_string(),
        serde_json::Value::String(channel_id.to_string()),
    );
    metadata.insert(
        "sender_user_id".to_string(),
        serde_json::Value::String(user_id.to_string()),
    );

    Some(ChannelMessage {
        channel: ChannelType::Mattermost,
        platform_message_id: post_id,
        sender: ChannelUser {
            platform_id: channel_id.to_string(),
            display_name: sender_name.to_string(),
            openfang_user: None,
        },
        content,
        target_agent: None,
        timestamp: Utc::now(),
        is_group,
        thread_id,
        metadata,
    })
}

#[async_trait]
impl ChannelAdapter for MattermostAdapter {
    fn name(&self) -> &str {
        "mattermost"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Mattermost
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        // Validate token and get bot user ID
        let user_id = self.validate_token().await?;
        *self.bot_user_id.write().await = Some(user_id);

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let ws_url = self.ws_url();
        let token = self.token.clone();
        let bot_user_id = self.bot_user_id.clone();
        let allowed_channels = self.allowed_channels.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            let mut backoff = INITIAL_BACKOFF;

            loop {
                if *shutdown_rx.borrow() {
                    break;
                }

                info!("Connecting to Mattermost WebSocket at {ws_url}...");

                let ws_result = tokio_tungstenite::connect_async(&ws_url).await;
                let ws_stream = match ws_result {
                    Ok((stream, _)) => stream,
                    Err(e) => {
                        warn!(
                            "Mattermost WebSocket connection failed: {e}, retrying in {backoff:?}"
                        );
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(MAX_BACKOFF);
                        continue;
                    }
                };

                backoff = INITIAL_BACKOFF;
                info!("Mattermost WebSocket connected");

                let (mut ws_tx, mut ws_rx) = ws_stream.split();

                // Authenticate over WebSocket with the token
                let auth_msg = serde_json::json!({
                    "seq": 1,
                    "action": "authentication_challenge",
                    "data": {
                        "token": token.as_str()
                    }
                });

                if let Err(e) = ws_tx
                    .send(tokio_tungstenite::tungstenite::Message::Text(
                        serde_json::to_string(&auth_msg).unwrap(),
                    ))
                    .await
                {
                    warn!("Mattermost WebSocket auth send failed: {e}");
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(MAX_BACKOFF);
                    continue;
                }

                // Inner message loop — returns true if we should reconnect
                let should_reconnect = 'inner: loop {
                    let msg = tokio::select! {
                        msg = ws_rx.next() => msg,
                        _ = shutdown_rx.changed() => {
                            if *shutdown_rx.borrow() {
                                info!("Mattermost adapter shutting down");
                                let _ = ws_tx.close().await;
                                return;
                            }
                            continue;
                        }
                    };

                    let msg = match msg {
                        Some(Ok(m)) => m,
                        Some(Err(e)) => {
                            warn!("Mattermost WebSocket error: {e}");
                            break 'inner true;
                        }
                        None => {
                            info!("Mattermost WebSocket closed");
                            break 'inner true;
                        }
                    };

                    let text = match msg {
                        tokio_tungstenite::tungstenite::Message::Text(t) => t,
                        tokio_tungstenite::tungstenite::Message::Close(_) => {
                            info!("Mattermost WebSocket closed by server");
                            break 'inner true;
                        }
                        _ => continue,
                    };

                    let payload: serde_json::Value = match serde_json::from_str(&text) {
                        Ok(v) => v,
                        Err(e) => {
                            warn!("Mattermost: failed to parse message: {e}");
                            continue;
                        }
                    };

                    // Check for auth response
                    if payload.get("status").is_some() {
                        let status = payload["status"].as_str().unwrap_or("");
                        if status == "OK" {
                            debug!("Mattermost WebSocket authentication successful");
                        } else {
                            warn!("Mattermost WebSocket auth response: {status}");
                        }
                        continue;
                    }

                    // Parse events
                    let bot_id_guard = bot_user_id.read().await;
                    if let Some(channel_msg) =
                        parse_mattermost_event(&payload, &bot_id_guard, &allowed_channels)
                    {
                        debug!(
                            "Mattermost message from {}: {:?}",
                            channel_msg.sender.display_name, channel_msg.content
                        );
                        drop(bot_id_guard);
                        if tx.send(channel_msg).await.is_err() {
                            return;
                        }
                    }
                };

                if !should_reconnect || *shutdown_rx.borrow() {
                    break;
                }

                warn!("Mattermost: reconnecting in {backoff:?}");
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(MAX_BACKOFF);
            }

            info!("Mattermost WebSocket loop stopped");
        });

        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    async fn send(
        &self,
        user: &ChannelUser,
        content: ChannelContent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let channel_id = &user.platform_id;
        match content {
            ChannelContent::Text(text) => {
                self.api_send_message(channel_id, &text).await?;
            }
            _ => {
                self.api_send_message(channel_id, "(Unsupported content type)")
                    .await?;
            }
        }
        Ok(())
    }

    async fn send_typing(&self, user: &ChannelUser) -> Result<(), Box<dyn std::error::Error>> {
        // Mattermost supports typing indicators via the WebSocket, but since we
        // only hold a WebSocket reader in the spawn loop, we use the REST API
        // userTyping action via a POST to /api/v4/users/me/typing.
        let url = format!("{}/api/v4/users/me/typing", self.server_url);
        let body = serde_json::json!({
            "channel_id": user.platform_id,
        });

        let _ = self
            .client
            .post(&url)
            .bearer_auth(self.token.as_str())
            .json(&body)
            .send()
            .await;

        Ok(())
    }

    async fn send_in_thread(
        &self,
        user: &ChannelUser,
        content: ChannelContent,
        thread_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let channel_id = &user.platform_id;
        let text = match content {
            ChannelContent::Text(t) => t,
            _ => "(Unsupported content type)".to_string(),
        };

        let url = format!("{}/api/v4/posts", self.server_url);
        let chunks = split_message(&text, MAX_MESSAGE_LEN);

        for chunk in chunks {
            let body = serde_json::json!({
                "channel_id": channel_id,
                "message": chunk,
                "root_id": thread_id,
            });

            let resp = self
                .client
                .post(&url)
                .bearer_auth(self.token.as_str())
                .json(&body)
                .send()
                .await?;

            if !resp.status().is_success() {
                let status = resp.status();
                let resp_body = resp.text().await.unwrap_or_default();
                warn!("Mattermost send_in_thread failed {status}: {resp_body}");
            }
        }

        Ok(())
    }

    async fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        let _ = self.shutdown_tx.send(true);
        Ok(())
    }
}

/// Mattermost channel **handle** (API `name`): lowercase, digits, `-`, `_`.
pub fn validate_mattermost_channel_handle(name: &str) -> Result<(), String> {
    let t = name.trim();
    if t.is_empty() {
        return Err("Mattermost channel name must not be empty".into());
    }
    if t.len() > 64 {
        return Err("Mattermost channel name is too long (max 64)".into());
    }
    if !t
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return Err(
            "Use the channel handle (lowercase, e.g. town-square), not the display title".into(),
        );
    }
    Ok(())
}

/// User input for binding: channel handle only, or `team_handle/channel_handle` (matches …/team/channels/channel in the web URL).
pub fn validate_mattermost_channel_binding_input(raw: &str) -> Result<(), String> {
    let t = raw.trim();
    if t.is_empty() {
        return Err("Mattermost channel binding must not be empty".into());
    }
    if t.contains('/') {
        let Some((team, ch)) = t.split_once('/') else {
            return Err("Invalid team/channel binding".into());
        };
        if team.is_empty() || ch.is_empty() || ch.contains('/') {
            return Err(
                "Use team/channel with exactly one slash, e.g. fang/fang (team URL slug, then channel slug)"
                    .into(),
            );
        }
        validate_mattermost_channel_handle(team)?;
        validate_mattermost_channel_handle(ch)?;
        return Ok(());
    }
    validate_mattermost_channel_handle(t)
}

/// Resolve channel id using `GET /teams/name/{team}` then `GET /teams/{id}/channels/name/{channel}`.
async fn resolve_mattermost_team_channel_named(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    team_handle: &str,
    channel_handle: &str,
) -> Result<(String, String), String> {
    let team_url = format!("{base}/api/v4/teams/name/{team_handle}");
    let tr = client
        .get(&team_url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| format!("Mattermost request failed: {e}"))?;
    if tr.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(format!(
            "No team named {team_handle:?} (GET /api/v4/teams/name/… returned 404)."
        ));
    }
    if !tr.status().is_success() {
        let st = tr.status();
        let b = tr.text().await.unwrap_or_default();
        return Err(format!("Mattermost team lookup failed {st}: {b}"));
    }
    let team: serde_json::Value = tr
        .json()
        .await
        .map_err(|e| format!("Invalid Mattermost team JSON: {e}"))?;
    let Some(team_id) = team["id"].as_str() else {
        return Err("Mattermost team response missing id".into());
    };
    let ch_url = format!("{base}/api/v4/teams/{team_id}/channels/name/{channel_handle}");
    let r = client
        .get(&ch_url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| format!("Mattermost request failed: {e}"))?;
    if r.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(format!(
            "No channel named {channel_handle:?} in team {team_handle:?} (add the bot to private channels)."
        ));
    }
    if !r.status().is_success() {
        let st = r.status();
        let b = r.text().await.unwrap_or_default();
        return Err(format!("Mattermost channel lookup failed {st}: {b}"));
    }
    let ch: serde_json::Value = r
        .json()
        .await
        .map_err(|e| format!("Invalid channel JSON: {e}"))?;
    let id = ch["id"]
        .as_str()
        .ok_or_else(|| "Mattermost channel response missing id".to_string())?
        .to_string();
    let handle = ch["name"]
        .as_str()
        .unwrap_or(channel_handle)
        .to_string();
    Ok((id, handle))
}

/// Resolve channel id from handle by scanning teams the token can access.
pub async fn resolve_mattermost_channel_id_by_name(
    server_url: &str,
    token: &str,
    channel_name: &str,
) -> Result<(String, String), String> {
    validate_mattermost_channel_binding_input(channel_name)?;
    let base = server_url.trim_end_matches('/');
    if base.is_empty() {
        return Err("Mattermost server_url is empty".into());
    }
    let client = reqwest::Client::new();

    let trimmed = channel_name.trim();
    if let Some((team_handle, channel_handle)) = trimmed.split_once('/') {
        let team_handle = team_handle.trim();
        let channel_handle = channel_handle.trim();
        return resolve_mattermost_team_channel_named(
            &client,
            base,
            token,
            team_handle,
            channel_handle,
        )
        .await;
    }

    let channel_only = trimmed;
    let teams_url = format!("{base}/api/v4/users/me/teams");
    let resp = client
        .get(&teams_url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| format!("Mattermost request failed: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Mattermost teams list failed {status}: {body}"));
    }
    let teams: Vec<serde_json::Value> = resp
        .json()
        .await
        .map_err(|e| format!("Invalid Mattermost teams JSON: {e}"))?;

    // System-admin / bot tokens often return an empty `GET /users/me/teams` even when
    // `GET /teams/name/{slug}` works. Fall back when team slug equals channel slug (e.g. …/fang/channels/fang).
    if teams.is_empty() {
        return resolve_mattermost_team_channel_named(
            &client,
            base,
            token,
            channel_only,
            channel_only,
        )
        .await;
    }

    let team_count = teams.len();
    let mut found: Option<(String, String)> = None;
    for team in teams {
        let Some(team_id) = team["id"].as_str() else {
            continue;
        };
        let ch_url = format!("{base}/api/v4/teams/{team_id}/channels/name/{channel_only}");
        let r = client
            .get(&ch_url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| format!("Mattermost request failed: {e}"))?;
        if r.status() == reqwest::StatusCode::NOT_FOUND {
            continue;
        }
        if !r.status().is_success() {
            let st = r.status();
            let b = r.text().await.unwrap_or_default();
            return Err(format!("Mattermost channel lookup failed {st}: {b}"));
        }
        let ch: serde_json::Value = r
            .json()
            .await
            .map_err(|e| format!("Invalid channel JSON: {e}"))?;
        let id = ch["id"]
            .as_str()
            .ok_or_else(|| "Mattermost channel response missing id".to_string())?
            .to_string();
        let handle = ch["name"]
            .as_str()
            .unwrap_or(channel_only)
            .to_string();
        if found.is_some() {
            return Err(format!(
                "Channel name {channel_only:?} exists in more than one team; use team/channel (e.g. fang/fang) or the Mattermost API"
            ));
        }
        found = Some((id, handle));
    }

    if let Some(pair) = found {
        return Ok(pair);
    }

    if let Ok(pair) =
        resolve_mattermost_team_channel_named(&client, base, token, channel_only, channel_only).await
    {
        return Ok(pair);
    }

    Err(format!(
        "No channel named {channel_only:?} in any of the {team_count} team(s) from GET /users/me/teams, \
         and no team/channel match via GET /teams/name/{channel_only}. \
         Use explicit team/channel (e.g. fang/town-square) if the team slug differs from the channel slug."
    ))
}

/// Resolve project binding: optional Mattermost **team** slug + **channel** slug (channel must not contain `/`).
/// Returns `(channel_id, channel_name, team_slug_for_storage)`.
pub async fn resolve_mattermost_project_channel_binding(
    server_url: &str,
    token: &str,
    team_slug: Option<&str>,
    channel_slug: &str,
) -> Result<(String, String, Option<String>), String> {
    let channel_slug = channel_slug.trim();
    validate_mattermost_channel_handle(channel_slug)?;
    let base = server_url.trim_end_matches('/');
    if base.is_empty() {
        return Err("Mattermost server_url is empty".into());
    }
    let client = reqwest::Client::new();
    match team_slug.map(|s| s.trim()).filter(|s| !s.is_empty()) {
        Some(t) => {
            validate_mattermost_channel_handle(t)?;
            let (cid, cname) =
                resolve_mattermost_team_channel_named(&client, base, token, t, channel_slug).await?;
            Ok((cid, cname, Some(t.to_string())))
        }
        None => {
            let (cid, cname) =
                resolve_mattermost_channel_id_by_name(server_url, token, channel_slug).await?;
            let team_opt = mattermost_team_slug_for_channel_id(server_url, token, &cid).await?;
            Ok((cid, cname, team_opt))
        }
    }
}

/// Team URL slug for a channel id (`GET /channels/{id}` then `GET /teams/{team_id}`).
pub async fn mattermost_team_slug_for_channel_id(
    server_url: &str,
    token: &str,
    channel_id: &str,
) -> Result<Option<String>, String> {
    let base = server_url.trim_end_matches('/');
    if base.is_empty() {
        return Err("Mattermost server_url is empty".into());
    }
    let client = reqwest::Client::new();
    let url = format!("{base}/api/v4/channels/{}", channel_id.trim());
    let r = client
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| format!("Mattermost request failed: {e}"))?;
    if !r.status().is_success() {
        return Ok(None);
    }
    let ch: serde_json::Value = r
        .json()
        .await
        .map_err(|e| format!("Invalid channel JSON: {e}"))?;
    let Some(tid) = ch["team_id"].as_str() else {
        return Ok(None);
    };
    let tu = format!("{base}/api/v4/teams/{tid}");
    let tr = client
        .get(&tu)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| format!("Mattermost request failed: {e}"))?;
    if !tr.status().is_success() {
        return Ok(None);
    }
    let team: serde_json::Value = tr
        .json()
        .await
        .map_err(|e| format!("Invalid Mattermost team JSON: {e}"))?;
    Ok(team["name"].as_str().map(|s| s.to_string()))
}

/// Verify a channel id exists and the token can access it; returns (id, channel handle, team slug if known).
pub async fn validate_mattermost_channel_id(
    server_url: &str,
    token: &str,
    channel_id: &str,
) -> Result<(String, String, Option<String>), String> {
    let t = channel_id.trim();
    if t.is_empty() {
        return Err("mattermost_channel_id must not be empty".into());
    }
    let base = server_url.trim_end_matches('/');
    if base.is_empty() {
        return Err("Mattermost server_url is empty".into());
    }
    let client = reqwest::Client::new();
    let url = format!("{base}/api/v4/channels/{t}");
    let r = client
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| format!("Mattermost request failed: {e}"))?;
    if !r.status().is_success() {
        return Err(format!(
            "Unknown or inaccessible Mattermost channel id (HTTP {})",
            r.status()
        ));
    }
    let ch: serde_json::Value = r
        .json()
        .await
        .map_err(|e| format!("Invalid channel JSON: {e}"))?;
    let id = ch["id"]
        .as_str()
        .ok_or_else(|| "Mattermost channel response missing id".to_string())?
        .to_string();
    let handle = ch["name"].as_str().unwrap_or("").trim().to_string();
    if handle.is_empty() {
        return Err("Mattermost channel response missing name".into());
    }
    let team_slug = if let Some(tid) = ch["team_id"].as_str() {
        let tu = format!("{base}/api/v4/teams/{tid}");
        match client.get(&tu).bearer_auth(token).send().await {
            Ok(tr) if tr.status().is_success() => match tr.json::<serde_json::Value>().await {
                Ok(tv) => tv["name"].as_str().map(|s| s.to_string()),
                Err(_) => None,
            },
            _ => None,
        }
    } else {
        None
    };
    Ok((id, handle, team_slug))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_mattermost_channel_handle() {
        assert!(validate_mattermost_channel_handle("town-square").is_ok());
        assert!(validate_mattermost_channel_handle("Town").is_err());
        assert!(validate_mattermost_channel_handle("").is_err());
    }

    #[test]
    fn test_validate_mattermost_channel_binding_input() {
        assert!(validate_mattermost_channel_binding_input("fang/fang").is_ok());
        assert!(validate_mattermost_channel_binding_input("town-square").is_ok());
        assert!(validate_mattermost_channel_binding_input("a/b/c").is_err());
    }

    #[test]
    fn test_mattermost_adapter_creation() {
        let adapter = MattermostAdapter::new(
            "https://mattermost.example.com".to_string(),
            "test-token".to_string(),
            vec![],
        );
        assert_eq!(adapter.name(), "mattermost");
        assert_eq!(adapter.channel_type(), ChannelType::Mattermost);
    }

    #[test]
    fn test_mattermost_ws_url_https() {
        let adapter = MattermostAdapter::new(
            "https://mm.example.com".to_string(),
            "token".to_string(),
            vec![],
        );
        assert_eq!(adapter.ws_url(), "wss://mm.example.com/api/v4/websocket");
    }

    #[test]
    fn test_mattermost_ws_url_http() {
        let adapter = MattermostAdapter::new(
            "http://localhost:8065".to_string(),
            "token".to_string(),
            vec![],
        );
        assert_eq!(adapter.ws_url(), "ws://localhost:8065/api/v4/websocket");
    }

    #[test]
    fn test_mattermost_ws_url_trailing_slash() {
        let adapter = MattermostAdapter::new(
            "https://mm.example.com/".to_string(),
            "token".to_string(),
            vec![],
        );
        // Constructor trims trailing slash
        assert_eq!(adapter.ws_url(), "wss://mm.example.com/api/v4/websocket");
    }

    #[test]
    fn test_mattermost_allowed_channels() {
        let adapter = MattermostAdapter::new(
            "https://mm.example.com".to_string(),
            "token".to_string(),
            vec!["ch1".to_string(), "ch2".to_string()],
        );
        assert!(adapter.is_allowed_channel("ch1"));
        assert!(adapter.is_allowed_channel("ch2"));
        assert!(!adapter.is_allowed_channel("ch3"));

        let open = MattermostAdapter::new(
            "https://mm.example.com".to_string(),
            "token".to_string(),
            vec![],
        );
        assert!(open.is_allowed_channel("any-channel"));
    }

    #[test]
    fn test_parse_mattermost_event_basic() {
        let post = serde_json::json!({
            "id": "post-1",
            "user_id": "user-456",
            "channel_id": "ch-789",
            "message": "Hello from Mattermost!",
            "root_id": ""
        });

        let event = serde_json::json!({
            "event": "posted",
            "data": {
                "post": serde_json::to_string(&post).unwrap(),
                "channel_type": "O",
                "sender_name": "alice"
            }
        });

        let bot_id = Some("bot-123".to_string());
        let msg = parse_mattermost_event(&event, &bot_id, &[]).unwrap();
        assert_eq!(msg.channel, ChannelType::Mattermost);
        assert_eq!(msg.sender.display_name, "alice");
        assert_eq!(msg.sender.platform_id, "ch-789");
        assert_eq!(
            msg.metadata
                .get("mattermost_channel_id")
                .and_then(|v| v.as_str()),
            Some("ch-789")
        );
        assert_eq!(
            msg.metadata.get("sender_user_id").and_then(|v| v.as_str()),
            Some("user-456")
        );
        assert!(msg.is_group);
        assert!(msg.thread_id.is_none());
        assert!(
            matches!(msg.content, ChannelContent::Text(ref t) if t == "Hello from Mattermost!")
        );
    }

    #[test]
    fn test_parse_mattermost_event_dm() {
        let post = serde_json::json!({
            "id": "post-1",
            "user_id": "user-456",
            "channel_id": "ch-789",
            "message": "DM message",
            "root_id": ""
        });

        let event = serde_json::json!({
            "event": "posted",
            "data": {
                "post": serde_json::to_string(&post).unwrap(),
                "channel_type": "D",
                "sender_name": "bob"
            }
        });

        let msg = parse_mattermost_event(&event, &None, &[]).unwrap();
        assert!(!msg.is_group);
    }

    #[test]
    fn test_parse_mattermost_event_threaded() {
        let post = serde_json::json!({
            "id": "post-2",
            "user_id": "user-456",
            "channel_id": "ch-789",
            "message": "Thread reply",
            "root_id": "post-1"
        });

        let event = serde_json::json!({
            "event": "posted",
            "data": {
                "post": serde_json::to_string(&post).unwrap(),
                "channel_type": "O",
                "sender_name": "alice"
            }
        });

        let msg = parse_mattermost_event(&event, &None, &[]).unwrap();
        assert_eq!(msg.thread_id, Some("post-1".to_string()));
    }

    #[test]
    fn test_parse_mattermost_event_skips_bot() {
        let post = serde_json::json!({
            "id": "post-1",
            "user_id": "bot-123",
            "channel_id": "ch-789",
            "message": "Bot message",
            "root_id": ""
        });

        let event = serde_json::json!({
            "event": "posted",
            "data": {
                "post": serde_json::to_string(&post).unwrap(),
                "channel_type": "O",
                "sender_name": "openfang-bot"
            }
        });

        let bot_id = Some("bot-123".to_string());
        let msg = parse_mattermost_event(&event, &bot_id, &[]);
        assert!(msg.is_none());
    }

    #[test]
    fn test_parse_mattermost_event_channel_filter() {
        let post = serde_json::json!({
            "id": "post-1",
            "user_id": "user-456",
            "channel_id": "ch-789",
            "message": "Hello",
            "root_id": ""
        });

        let event = serde_json::json!({
            "event": "posted",
            "data": {
                "post": serde_json::to_string(&post).unwrap(),
                "channel_type": "O",
                "sender_name": "alice"
            }
        });

        // Not in allowed channels
        let msg =
            parse_mattermost_event(&event, &None, &["ch-111".to_string(), "ch-222".to_string()]);
        assert!(msg.is_none());

        // In allowed channels
        let msg = parse_mattermost_event(&event, &None, &["ch-789".to_string()]);
        assert!(msg.is_some());
    }

    #[test]
    fn test_parse_mattermost_event_command() {
        let post = serde_json::json!({
            "id": "post-1",
            "user_id": "user-456",
            "channel_id": "ch-789",
            "message": "/agent hello-world",
            "root_id": ""
        });

        let event = serde_json::json!({
            "event": "posted",
            "data": {
                "post": serde_json::to_string(&post).unwrap(),
                "channel_type": "O",
                "sender_name": "alice"
            }
        });

        let msg = parse_mattermost_event(&event, &None, &[]).unwrap();
        match &msg.content {
            ChannelContent::Command { name, args } => {
                assert_eq!(name, "agent");
                assert_eq!(args, &["hello-world"]);
            }
            other => panic!("Expected Command, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_mattermost_event_non_posted() {
        let event = serde_json::json!({
            "event": "typing",
            "data": {}
        });

        let msg = parse_mattermost_event(&event, &None, &[]);
        assert!(msg.is_none());
    }

    #[test]
    fn test_parse_mattermost_event_empty_message() {
        let post = serde_json::json!({
            "id": "post-1",
            "user_id": "user-456",
            "channel_id": "ch-789",
            "message": "",
            "root_id": ""
        });

        let event = serde_json::json!({
            "event": "posted",
            "data": {
                "post": serde_json::to_string(&post).unwrap(),
                "channel_type": "O",
                "sender_name": "alice"
            }
        });

        let msg = parse_mattermost_event(&event, &None, &[]);
        assert!(msg.is_none());
    }
}
