//! Webhook HTTP server built on axum.
//!
//! Receives push notifications from external services (Google Calendar,
//! Gmail, GitHub, Slack) and routes them to the agent's event processor.
//!
//! # Architecture
//!
//! ```text
//!   Google Calendar ──►┐
//!   Gmail Push ────────►  Webhook Server (:8443)  ──► Event Queue ──► Notifier
//!   GitHub Webhook ────►┘
//! ```

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing;

// ── Shared application state ────────────────────────────────────────

#[derive(Clone)]
pub struct WebhookState {
    /// Directory for persisted state (rsvp_state.json, etc.)
    pub data_dir: PathBuf,
    /// Event queue for processing received webhooks
    pub events: Arc<Mutex<Vec<WebhookEvent>>>,
    /// Optional Telegram bot token for forwarding notifications
    pub telegram_bot_token: Option<String>,
    /// Optional Telegram chat ID
    pub telegram_chat_id: Option<String>,
    /// Secrets used to verify that a delivery really came from its provider.
    /// A provider with no secret here is accepted unverified — see
    /// [`crate::webhooks::verify`].
    pub secrets: super::verify::WebhookSecrets,
}

/// Most recent webhook events retained in memory.
///
/// The webhook port is public and unauthenticated by design (providers must be
/// able to reach it), so an unbounded queue is a free denial-of-service: anyone
/// who can reach the port can grow the process until it is killed.
const MAX_RETAINED_EVENTS: usize = 500;

/// Record an event, evicting the oldest once the cap is reached.
async fn record_event(state: &WebhookState, event: WebhookEvent) {
    let mut events = state.events.lock().await;
    if events.len() >= MAX_RETAINED_EVENTS {
        let overflow = events.len() + 1 - MAX_RETAINED_EVENTS;
        events.drain(0..overflow);
    }
    events.push(event);
}

// ── Event types ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookEvent {
    pub source: String,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub received_at: String,
}

// ── Google Calendar push notification ───────────────────────────────

#[derive(Debug, Deserialize)]
pub struct GoogleCalendarNotification {
    /// The channel ID (matches what we registered)
    #[serde(rename = "channelId")]
    pub channel_id: Option<String>,
    /// Resource ID
    #[serde(rename = "resourceId")]
    pub resource_id: Option<String>,
    /// Resource state: "sync", "exists", "not_exists"
    #[serde(rename = "resourceState")]
    pub resource_state: Option<String>,
}

// ── GitHub webhook payload (simplified) ─────────────────────────────

#[derive(Debug, Deserialize)]
pub struct GitHubWebhookPayload {
    pub action: Option<String>,
    pub sender: Option<GitHubUser>,
    pub repository: Option<GitHubRepo>,
    pub pull_request: Option<GitHubPR>,
    pub issue: Option<GitHubIssue>,
}

#[derive(Debug, Deserialize)]
pub struct GitHubUser {
    pub login: String,
}

#[derive(Debug, Deserialize)]
pub struct GitHubRepo {
    pub full_name: String,
}

#[derive(Debug, Deserialize)]
pub struct GitHubPR {
    pub title: String,
    pub number: u64,
    pub html_url: String,
}

#[derive(Debug, Deserialize)]
pub struct GitHubIssue {
    pub title: String,
    pub number: u64,
    pub html_url: String,
}

// ── Slack event payload ─────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SlackEventPayload {
    /// "url_verification" or "event_callback"
    #[serde(rename = "type")]
    pub event_type: String,
    /// Present for url_verification
    pub challenge: Option<String>,
    /// The actual event
    pub event: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct SlackChallengeResponse {
    pub challenge: String,
}

// ── Router builder ──────────────────────────────────────────────────

/// Build the webhook router with all service endpoints.
pub fn webhook_router(state: WebhookState) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route(
            "/webhooks/google/calendar",
            post(handle_google_calendar_webhook),
        )
        .route("/webhooks/google/gmail", post(handle_gmail_webhook))
        .route("/webhooks/github", post(handle_github_webhook))
        .route("/webhooks/slack/events", post(handle_slack_events))
        .with_state(state)
}

/// Read a header as a string, if it is present and valid UTF-8.
fn header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

// ── Handlers ────────────────────────────────────────────────────────

async fn health_check() -> &'static str {
    "ok"
}

/// Handle Google Calendar push notifications.
///
/// When a calendar event changes, Google sends a POST with resource info.
/// We then fetch updated events via the Calendar API to detect RSVP changes.
async fn handle_google_calendar_webhook(
    State(state): State<WebhookState>,
    Json(payload): Json<GoogleCalendarNotification>,
) -> StatusCode {
    tracing::info!(
        "Google Calendar webhook received: state={:?}, resource={:?}",
        payload.resource_state,
        payload.resource_id
    );

    // Skip sync messages (initial subscription confirmation)
    if payload.resource_state.as_deref() == Some("sync") {
        return StatusCode::OK;
    }

    let event = WebhookEvent {
        source: "google_calendar".into(),
        event_type: payload
            .resource_state
            .unwrap_or_else(|| "unknown".into()),
        payload: serde_json::json!({
            "channel_id": payload.channel_id,
            "resource_id": payload.resource_id,
        }),
        received_at: chrono::Utc::now().to_rfc3339(),
    };

    record_event(&state, event).await;

    // Trigger an RSVP check for the affected resource
    let data_dir = state.data_dir.clone();
    tokio::spawn(async move {
        if let Err(e) = process_calendar_update(&data_dir).await {
            tracing::error!("Failed to process calendar update: {}", e);
        }
    });

    StatusCode::OK
}

/// Handle Gmail push notifications.
async fn handle_gmail_webhook(
    State(state): State<WebhookState>,
    body: String,
) -> StatusCode {
    tracing::info!("Gmail webhook received ({} bytes)", body.len());

    let event = WebhookEvent {
        source: "gmail".into(),
        event_type: "push_notification".into(),
        payload: serde_json::json!({ "raw": body }),
        received_at: chrono::Utc::now().to_rfc3339(),
    };

    record_event(&state, event).await;
    StatusCode::OK
}

/// Handle GitHub webhooks.
///
/// Processes PR reviews, issue assignments, and push notifications.
async fn handle_github_webhook(
    State(state): State<WebhookState>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    // Verification happens on the raw bytes, before parsing: the signature
    // covers exactly what was sent, and a re-serialized payload would not match.
    if let Err(rejection) = super::verify::github(
        state.secrets.github.as_deref(),
        header(&headers, "x-hub-signature-256"),
        &body,
    ) {
        tracing::warn!("Rejected a GitHub webhook: {}", rejection.detail());
        return StatusCode::UNAUTHORIZED;
    }

    let Ok(payload) = serde_json::from_slice::<GitHubWebhookPayload>(&body) else {
        tracing::warn!("GitHub webhook body was not the expected JSON");
        return StatusCode::BAD_REQUEST;
    };

    let action = payload.action.as_deref().unwrap_or("unknown");
    let repo = payload
        .repository
        .as_ref()
        .map(|r| r.full_name.as_str())
        .unwrap_or("unknown");

    tracing::info!("GitHub webhook: action={}, repo={}", action, repo);

    let (event_type, summary) = if let Some(ref pr) = payload.pull_request {
        (
            format!("pull_request.{}", action),
            format!("PR #{}: {}", pr.number, pr.title),
        )
    } else if let Some(ref issue) = payload.issue {
        (
            format!("issues.{}", action),
            format!("Issue #{}: {}", issue.number, issue.title),
        )
    } else {
        (format!("other.{}", action), "GitHub event".to_string())
    };

    let event = WebhookEvent {
        source: "github".into(),
        event_type,
        payload: serde_json::json!({
            "action": action,
            "repo": repo,
            "summary": summary,
        }),
        received_at: chrono::Utc::now().to_rfc3339(),
    };

    record_event(&state, event).await;

    // Notify user if it's a PR review request
    if action == "review_requested" {
        if let Some(ref token) = state.telegram_bot_token {
            if let Some(ref chat_id) = state.telegram_chat_id {
                let _ = send_telegram_notification(
                    token,
                    chat_id,
                    &format!("🔔 GitHub: {}", summary),
                )
                .await;
            }
        }
    }

    StatusCode::OK
}

/// Handle Slack events (including URL verification challenge).
async fn handle_slack_events(
    State(state): State<WebhookState>,
    headers: HeaderMap,
    body: Bytes,
) -> (StatusCode, Json<serde_json::Value>) {
    // Slack signs `v0:<timestamp>:<body>`, so the raw bytes and the timestamp
    // header both have to be in hand before anything is parsed.
    if let Err(rejection) = super::verify::slack(
        state.secrets.slack.as_deref(),
        header(&headers, "x-slack-signature"),
        header(&headers, "x-slack-request-timestamp"),
        &body,
        super::verify::now_unix(),
    ) {
        tracing::warn!("Rejected a Slack webhook: {}", rejection.detail());
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": rejection.message() })),
        );
    }

    let Ok(payload) = serde_json::from_slice::<SlackEventPayload>(&body) else {
        tracing::warn!("Slack webhook body was not the expected JSON");
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "malformed payload" })),
        );
    };

    // Handle URL verification challenge
    if payload.event_type == "url_verification" {
        if let Some(challenge) = payload.challenge {
            return (StatusCode::OK, Json(serde_json::json!({ "challenge": challenge })));
        }
    }

    tracing::info!("Slack event received: type={}", payload.event_type);

    if let Some(event_data) = payload.event {
        let event = WebhookEvent {
            source: "slack".into(),
            event_type: payload.event_type,
            payload: event_data,
            received_at: chrono::Utc::now().to_rfc3339(),
        };
        record_event(&state, event).await;
    }

    (StatusCode::OK, Json(serde_json::json!({})))
}

// ── Helper: Process calendar update ─────────────────────────────────

async fn process_calendar_update(data_dir: &std::path::Path) -> anyhow::Result<()> {
    // Load RSVP state and check for changes.
    // This is the webhook-triggered equivalent of the cron-based RSVP monitor.
    let state_path = data_dir.join("rsvp_state.json");
    tracing::info!("Processing calendar update, state: {:?}", state_path);
    // In a full implementation, this would:
    // 1. Load RsvpState from disk
    // 2. Fetch updated events from Google Calendar API
    // 3. Compare against stored state
    // 4. Send notifications for changes
    // 5. Save updated state
    Ok(())
}

// ── Helper: Send Telegram notification ──────────────────────────────

async fn send_telegram_notification(
    bot_token: &str,
    chat_id: &str,
    message: &str,
) -> anyhow::Result<()> {
    let url = format!(
        "https://api.telegram.org/bot{}/sendMessage",
        bot_token
    );

    let client = reqwest::Client::new();
    client
        .post(&url)
        .json(&serde_json::json!({
            "chat_id": chat_id,
            "text": message,
            "parse_mode": "HTML",
        }))
        .send()
        .await?;

    Ok(())
}

// ── Server startup ──────────────────────────────────────────────────

/// Start the webhook HTTP server.
///
/// # Arguments
/// * `port` - Port to listen on (default: 8443)
/// * `state` - Shared application state
pub async fn start_webhook_server(port: u16, state: WebhookState) -> anyhow::Result<()> {
    let app = webhook_router(state);

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("Webhook server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt; // for `oneshot`

    fn test_state() -> WebhookState {
        WebhookState {
            data_dir: PathBuf::from("/tmp/pylot-test"),
            events: Arc::new(Mutex::new(Vec::new())),
            telegram_bot_token: None,
            telegram_chat_id: None,
            secrets: crate::webhooks::verify::WebhookSecrets::default(),
        }
    }

    #[tokio::test]
    async fn test_health_check() {
        let app = webhook_router(test_state());

        let req = Request::builder()
            .uri("/health")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_google_calendar_sync_skip() {
        let state = test_state();
        let app = webhook_router(state.clone());

        let payload = serde_json::json!({
            "channelId": "test-channel",
            "resourceId": "test-resource",
            "resourceState": "sync"
        });

        let req = Request::builder()
            .method("POST")
            .uri("/webhooks/google/calendar")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&payload).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Sync messages should not be queued
        assert!(state.events.lock().await.is_empty());
    }

    #[tokio::test]
    async fn test_google_calendar_exists_event() {
        let state = test_state();
        let app = webhook_router(state.clone());

        let payload = serde_json::json!({
            "channelId": "ch-1",
            "resourceId": "res-1",
            "resourceState": "exists"
        });

        let req = Request::builder()
            .method("POST")
            .uri("/webhooks/google/calendar")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&payload).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Wait briefly for the event to be pushed
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let events = state.events.lock().await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].source, "google_calendar");
        assert_eq!(events[0].event_type, "exists");
    }

    #[tokio::test]
    async fn test_github_pr_webhook() {
        let state = test_state();
        let app = webhook_router(state.clone());

        let payload = serde_json::json!({
            "action": "opened",
            "sender": { "login": "testuser" },
            "repository": { "full_name": "openpylot/pylot" },
            "pull_request": {
                "title": "Add webhook support",
                "number": 42,
                "html_url": "https://github.com/openpylot/pylot/pull/42"
            }
        });

        let req = Request::builder()
            .method("POST")
            .uri("/webhooks/github")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&payload).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let events = state.events.lock().await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].source, "github");
        assert_eq!(events[0].event_type, "pull_request.opened");
    }

    #[tokio::test]
    async fn test_slack_url_verification() {
        let state = test_state();
        let app = webhook_router(state);

        let payload = serde_json::json!({
            "type": "url_verification",
            "challenge": "test-challenge-123"
        });

        let req = Request::builder()
            .method("POST")
            .uri("/webhooks/slack/events")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&payload).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["challenge"], "test-challenge-123");
    }
}

#[cfg(test)]
mod retention_tests {
    use super::*;

    fn state() -> WebhookState {
        WebhookState {
            data_dir: std::env::temp_dir(),
            events: Arc::new(Mutex::new(Vec::new())),
            telegram_bot_token: None,
            telegram_chat_id: None,
            secrets: crate::webhooks::verify::WebhookSecrets::default(),
        }
    }

    fn event(n: usize) -> WebhookEvent {
        WebhookEvent {
            source: "test".into(),
            event_type: format!("e{n}"),
            payload: serde_json::json!({}),
            received_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    #[tokio::test]
    async fn the_queue_is_capped_so_a_public_port_cannot_exhaust_memory() {
        let state = state();
        for n in 0..(MAX_RETAINED_EVENTS + 25) {
            record_event(&state, event(n)).await;
        }

        let events = state.events.lock().await;
        assert_eq!(events.len(), MAX_RETAINED_EVENTS, "queue must stop growing");
        assert_eq!(
            events.last().unwrap().event_type,
            format!("e{}", MAX_RETAINED_EVENTS + 24),
            "the newest event must survive"
        );
        assert_eq!(
            events.first().unwrap().event_type,
            format!("e{}", 25),
            "the oldest events are the ones evicted"
        );
    }

    #[tokio::test]
    async fn events_below_the_cap_are_all_retained() {
        let state = state();
        for n in 0..10 {
            record_event(&state, event(n)).await;
        }
        assert_eq!(state.events.lock().await.len(), 10);
    }
}

#[cfg(test)]
mod signature_tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    const SECRET: &str = "shhh";

    fn state_with(secrets: crate::webhooks::verify::WebhookSecrets) -> WebhookState {
        WebhookState {
            data_dir: std::env::temp_dir(),
            events: Arc::new(Mutex::new(Vec::new())),
            telegram_bot_token: None,
            telegram_chat_id: None,
            secrets,
        }
    }

    fn github_secrets() -> crate::webhooks::verify::WebhookSecrets {
        crate::webhooks::verify::WebhookSecrets {
            github: Some(SECRET.into()),
            slack: None,
        }
    }

    fn sign_github(body: &str) -> String {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        let mut mac = Hmac::<Sha256>::new_from_slice(SECRET.as_bytes()).unwrap();
        mac.update(body.as_bytes());
        let hex: String = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        format!("sha256={hex}")
    }

    const PR_BODY: &str = r#"{"action":"opened","pull_request":{"number":1,"title":"t","html_url":"u"}}"#;

    async fn post_github(
        state: WebhookState,
        body: &str,
        signature: Option<&str>,
    ) -> (StatusCode, WebhookState) {
        let mut request = Request::post("/webhooks/github").header("content-type", "application/json");
        if let Some(sig) = signature {
            request = request.header("x-hub-signature-256", sig);
        }
        let response = webhook_router(state.clone())
            .oneshot(request.body(Body::from(body.to_string())).unwrap())
            .await
            .unwrap();
        (response.status(), state)
    }

    #[tokio::test]
    async fn a_correctly_signed_github_delivery_is_accepted_and_recorded() {
        let (status, state) =
            post_github(state_with(github_secrets()), PR_BODY, Some(&sign_github(PR_BODY))).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(state.events.lock().await.len(), 1);
    }

    #[tokio::test]
    async fn an_unsigned_github_delivery_is_rejected_when_a_secret_is_set() {
        let (status, state) = post_github(state_with(github_secrets()), PR_BODY, None).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(
            state.events.lock().await.is_empty(),
            "a forged event must never reach the queue"
        );
    }

    #[tokio::test]
    async fn a_forged_github_delivery_is_rejected() {
        // Correctly signed for a *different* body — the shape an attacker who
        // captured one real delivery would produce.
        let signature = sign_github(PR_BODY);
        let forged = r#"{"action":"opened","pull_request":{"number":999,"title":"forged","html_url":"u"}}"#;

        let (status, state) = post_github(state_with(github_secrets()), forged, Some(&signature)).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(state.events.lock().await.is_empty());
    }

    #[tokio::test]
    async fn an_unsigned_github_delivery_is_accepted_when_no_secret_is_configured() {
        // The documented upgrade path: existing installs keep working, and
        // startup warns that this is what is happening.
        let (status, state) = post_github(
            state_with(crate::webhooks::verify::WebhookSecrets::default()),
            PR_BODY,
            None,
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(state.events.lock().await.len(), 1);
    }

    #[tokio::test]
    async fn a_signed_but_malformed_body_is_a_bad_request_not_a_panic() {
        let body = "{not json";
        let (status, state) =
            post_github(state_with(github_secrets()), body, Some(&sign_github(body))).await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(state.events.lock().await.is_empty());
    }

    #[tokio::test]
    async fn an_unsigned_slack_event_is_rejected_when_a_secret_is_set() {
        let state = state_with(crate::webhooks::verify::WebhookSecrets {
            github: None,
            slack: Some(SECRET.into()),
        });

        let response = webhook_router(state.clone())
            .oneshot(
                Request::post("/webhooks/slack/events")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"type":"url_verification","challenge":"c"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "even the URL-verification challenge must be signed"
        );
    }

    #[tokio::test]
    async fn health_needs_no_signature() {
        // Liveness probes do not sign anything.
        let response = webhook_router(state_with(github_secrets()))
            .oneshot(Request::get("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
