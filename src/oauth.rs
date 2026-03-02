//! Browser-based OAuth integration flows.
//!
//! Provides a generic OAuth 2.0 flow that:
//! 1. Opens the user's browser to the provider's consent screen
//! 2. Listens on a local HTTP callback server for the redirect
//! 3. Exchanges the authorization code for tokens
//! 4. Stores tokens securely in the secrets vault
//!
//! Supported providers: Google (Calendar, Gmail, Drive), GitHub, Slack.

use anyhow::{Context, Result};
use axum::{
    extract::Query,
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};
use tracing;

// ── OAuth configuration ─────────────────────────────────────────────

/// Configuration for an OAuth 2.0 flow.
#[derive(Debug, Clone)]
pub struct OAuthConfig {
    /// Provider name (for display)
    pub provider: String,
    /// OAuth authorize URL
    pub auth_url: String,
    /// OAuth token exchange URL
    pub token_url: String,
    /// Client ID
    pub client_id: String,
    /// Client secret
    pub client_secret: String,
    /// Requested scopes
    pub scopes: Vec<String>,
    /// Local redirect port (e.g. 8085)
    pub redirect_port: u16,
    /// Additional query parameters for the auth URL
    pub extra_params: HashMap<String, String>,
}

/// Token response from the OAuth provider.
#[derive(Debug, Clone, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: Option<u64>,
    pub token_type: Option<String>,
    pub scope: Option<String>,
}

// ── Query parameters from the redirect ──────────────────────────────

#[derive(Debug, Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    error: Option<String>,
    state: Option<String>,
}

// ── Provider presets ────────────────────────────────────────────────

/// Google OAuth configuration for Calendar, Gmail, and Drive.
pub fn google_oauth_config(
    client_id: &str,
    client_secret: &str,
    scopes: Vec<String>,
    redirect_port: u16,
) -> OAuthConfig {
    let mut extra = HashMap::new();
    extra.insert("access_type".into(), "offline".into());
    extra.insert("prompt".into(), "consent".into());

    OAuthConfig {
        provider: "Google".into(),
        auth_url: "https://accounts.google.com/o/oauth2/v2/auth".into(),
        token_url: "https://oauth2.googleapis.com/token".into(),
        client_id: client_id.into(),
        client_secret: client_secret.into(),
        scopes,
        redirect_port,
        extra_params: extra,
    }
}

/// GitHub OAuth configuration.
pub fn github_oauth_config(
    client_id: &str,
    client_secret: &str,
    scopes: Vec<String>,
) -> OAuthConfig {
    OAuthConfig {
        provider: "GitHub".into(),
        auth_url: "https://github.com/login/oauth/authorize".into(),
        token_url: "https://github.com/login/oauth/access_token".into(),
        client_id: client_id.into(),
        client_secret: client_secret.into(),
        scopes,
        redirect_port: 8085,
        extra_params: HashMap::new(),
    }
}

/// Slack OAuth configuration.
pub fn slack_oauth_config(
    client_id: &str,
    client_secret: &str,
    scopes: Vec<String>,
) -> OAuthConfig {
    OAuthConfig {
        provider: "Slack".into(),
        auth_url: "https://slack.com/oauth/v2/authorize".into(),
        token_url: "https://slack.com/api/oauth.v2.access".into(),
        client_id: client_id.into(),
        client_secret: client_secret.into(),
        scopes,
        redirect_port: 8085,
        extra_params: HashMap::new(),
    }
}

// ── Default Google scopes ───────────────────────────────────────────

/// Standard Google scopes for the GMV Agent.
pub fn default_google_scopes() -> Vec<String> {
    vec![
        "https://www.googleapis.com/auth/calendar".into(),
        "https://www.googleapis.com/auth/calendar.events".into(),
        "https://www.googleapis.com/auth/gmail.modify".into(),
        "https://www.googleapis.com/auth/gmail.send".into(),
    ]
}

/// Standard GitHub scopes.
pub fn default_github_scopes() -> Vec<String> {
    vec!["repo".into(), "read:org".into(), "notifications".into()]
}

/// Standard Slack bot scopes.
pub fn default_slack_scopes() -> Vec<String> {
    vec![
        "chat:write".into(),
        "channels:read".into(),
        "im:read".into(),
        "im:write".into(),
    ]
}

// ── Main OAuth flow ─────────────────────────────────────────────────

/// Run a complete browser-based OAuth 2.0 authorization code flow.
///
/// 1. Builds the authorization URL with scopes, redirect URI, state
/// 2. Opens the user's default browser
/// 3. Starts a local HTTP server to catch the redirect
/// 4. Exchanges the code for tokens
/// 5. Returns the token response
///
/// # Arguments
/// * `config` - OAuth provider configuration
///
/// # Returns
/// Token response with access_token and optional refresh_token.
pub async fn run_oauth_flow(config: &OAuthConfig) -> Result<TokenResponse> {
    let redirect_uri = format!("http://localhost:{}/callback", config.redirect_port);

    // Generate a random state parameter for CSRF protection
    let state = generate_state();

    // Build the authorization URL
    let mut auth_url = format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&state={}",
        config.auth_url,
        urlencoding::encode(&config.client_id),
        urlencoding::encode(&redirect_uri),
        urlencoding::encode(&config.scopes.join(" ")),
        urlencoding::encode(&state),
    );

    for (k, v) in &config.extra_params {
        auth_url.push_str(&format!(
            "&{}={}",
            urlencoding::encode(k),
            urlencoding::encode(v)
        ));
    }

    // Open browser
    tracing::info!("Opening browser for {} authorization...", config.provider);
    println!("  → Opening browser for {} authorization...", config.provider);
    println!("  → If the browser doesn't open, visit:");
    println!("    {}", auth_url);

    if let Err(e) = open::that(&auth_url) {
        tracing::warn!("Failed to open browser: {}", e);
        println!("  ⚠ Could not open browser automatically.");
        println!("  → Please open the URL above manually.");
    }

    // Start local callback server and wait for the redirect
    println!(
        "  → Waiting for callback on http://localhost:{}/ ...",
        config.redirect_port
    );

    let code = wait_for_callback(config.redirect_port, &state).await?;

    println!("  → Exchanging authorization code for tokens...");

    // Exchange code for tokens
    let tokens = exchange_code(config, &code, &redirect_uri).await?;

    println!("  ✅ {} authorization complete!", config.provider);

    Ok(tokens)
}

// ── Local callback server ───────────────────────────────────────────

/// Start a local HTTP server and wait for the OAuth callback.
///
/// The server listens on the specified port for a single GET /callback
/// request, extracts the authorization code, and shuts down.
pub async fn wait_for_callback(port: u16, expected_state: &str) -> Result<String> {
    let (tx, rx) = oneshot::channel::<String>();
    let tx = Arc::new(Mutex::new(Some(tx)));
    let expected = expected_state.to_string();

    let tx_clone = tx.clone();
    let expected_clone = expected.clone();

    let app = Router::new().route(
        "/callback",
        get(move |Query(params): Query<CallbackQuery>| {
            let tx = tx_clone.clone();
            let expected = expected_clone.clone();
            async move {
                // Check for errors
                if let Some(error) = params.error {
                    return Html(format!(
                        "<html><body><h1>❌ Authorization Failed</h1>\
                         <p>Error: {}</p>\
                         <p>You can close this window and try again.</p>\
                         </body></html>",
                        error
                    ))
                    .into_response();
                }

                // Validate state parameter
                if let Some(ref state) = params.state {
                    if state != &expected {
                        return Html(
                            "<html><body><h1>❌ Invalid State</h1>\
                             <p>CSRF state mismatch. Please try again.</p>\
                             </body></html>"
                                .to_string(),
                        )
                        .into_response();
                    }
                }

                // Extract authorization code
                if let Some(code) = params.code {
                    let mut guard = tx.lock().await;
                    if let Some(sender) = guard.take() {
                        let _ = sender.send(code);
                    }

                    Html(
                        "<html><body style='font-family: system-ui; text-align: center; padding: 60px'>\
                         <h1>✅ Authorization Successful!</h1>\
                         <p>You can close this window and return to the terminal.</p>\
                         <script>setTimeout(() => window.close(), 3000)</script>\
                         </body></html>"
                            .to_string(),
                    )
                    .into_response()
                } else {
                    (
                        StatusCode::BAD_REQUEST,
                        Html(
                            "<html><body><h1>❌ Missing Code</h1>\
                             <p>No authorization code received.</p></body></html>"
                                .to_string(),
                        ),
                    )
                        .into_response()
                }
            }
        }),
    );

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .context(format!("Failed to bind callback server to port {}", port))?;

    // Run server in background, shut down after receiving the code
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });

    // Wait for the code with a timeout
    let code = tokio::time::timeout(std::time::Duration::from_secs(300), rx)
        .await
        .context("OAuth callback timed out (5 minutes). Please try again.")?
        .context("OAuth callback channel closed unexpectedly")?;

    // Shut down the server
    server.abort();

    Ok(code)
}

// ── Token exchange ──────────────────────────────────────────────────

/// Exchange an authorization code for access/refresh tokens.
pub async fn exchange_code(
    config: &OAuthConfig,
    code: &str,
    redirect_uri: &str,
) -> Result<TokenResponse> {
    let client = reqwest::Client::new();

    let mut params = HashMap::new();
    params.insert("grant_type", "authorization_code");
    params.insert("code", code);
    params.insert("redirect_uri", redirect_uri);
    params.insert("client_id", &config.client_id);
    params.insert("client_secret", &config.client_secret);

    let resp = client
        .post(&config.token_url)
        .form(&params)
        .header("Accept", "application/json")
        .send()
        .await
        .context("Failed to exchange authorization code")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!(
            "Token exchange failed (HTTP {}): {}",
            status,
            body
        );
    }

    let tokens: TokenResponse = resp
        .json()
        .await
        .context("Failed to parse token response")?;

    Ok(tokens)
}

/// Refresh an existing access token using a refresh token.
pub async fn refresh_access_token(
    token_url: &str,
    client_id: &str,
    client_secret: &str,
    refresh_token: &str,
) -> Result<TokenResponse> {
    let client = reqwest::Client::new();

    let mut params = HashMap::new();
    params.insert("grant_type", "refresh_token");
    params.insert("refresh_token", refresh_token);
    params.insert("client_id", client_id);
    params.insert("client_secret", client_secret);

    let resp = client
        .post(token_url)
        .form(&params)
        .header("Accept", "application/json")
        .send()
        .await
        .context("Failed to refresh access token")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Token refresh failed (HTTP {}): {}", status, body);
    }

    let tokens: TokenResponse = resp
        .json()
        .await
        .context("Failed to parse refresh token response")?;

    Ok(tokens)
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Generate a random state string for CSRF protection.
pub fn generate_state() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..32).map(|_| rng.gen()).collect();
    base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, &bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_google_oauth_config() {
        let config = google_oauth_config(
            "client-123",
            "secret-456",
            default_google_scopes(),
            8085,
        );
        assert_eq!(config.provider, "Google");
        assert_eq!(config.client_id, "client-123");
        assert_eq!(config.scopes.len(), 4);
        assert!(config.extra_params.contains_key("access_type"));
    }

    #[test]
    fn test_github_oauth_config() {
        let config = github_oauth_config("gh-id", "gh-secret", default_github_scopes());
        assert_eq!(config.provider, "GitHub");
        assert_eq!(config.scopes.len(), 3);
    }

    #[test]
    fn test_slack_oauth_config() {
        let config = slack_oauth_config("sl-id", "sl-secret", default_slack_scopes());
        assert_eq!(config.provider, "Slack");
        assert_eq!(config.scopes.len(), 4);
    }

    #[test]
    fn test_generate_state() {
        let s1 = generate_state();
        let s2 = generate_state();
        assert_ne!(s1, s2);
        assert!(s1.len() > 20);
    }

    #[test]
    fn test_default_scopes() {
        let google = default_google_scopes();
        assert!(google.iter().any(|s| s.contains("calendar")));

        let github = default_github_scopes();
        assert!(github.contains(&"repo".to_string()));

        let slack = default_slack_scopes();
        assert!(slack.contains(&"chat:write".to_string()));
    }
}
