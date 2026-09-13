//! Bearer-token authentication for the local API + WebSocket surface.
//!
//! # Why this exists
//!
//! `pylot serve` exposes the *whole agent* over HTTP — including `bash`, file
//! writes, the secrets-backed integrations and every stored conversation.
//! Before this module the server bound `0.0.0.0` with `allow_origin(Any)` and
//! no credential of any kind, which made it unauthenticated remote code
//! execution for anyone on the same network, and CSRF-able from any website the
//! user happened to visit while the server was up.
//!
//! The fix is the same shape Jupyter uses for exactly the same problem:
//!
//! 1. bind loopback by default (see [`crate::api::start_api_server`]);
//! 2. mint a per-install token, stored `0600` in the data dir;
//! 3. require it on every `/api/*` and `/ws/*` request;
//! 4. let the browser bootstrap itself from `?token=…` on first load.
//!
//! Static frontend assets stay unauthenticated — they are inert HTML/JS and
//! carry no user data, and gating them would leave the browser with no way to
//! bootstrap. Everything that reads or mutates state sits behind the token.

use std::path::{Path, PathBuf};

use axum::{
    body::Body,
    extract::Request,
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use rand::RngCore;

/// Name of the token file inside the data directory.
const TOKEN_FILE: &str = "api-token";

/// Header clients may use instead of `Authorization: Bearer …`.
pub const TOKEN_HEADER: &str = "x-pylot-token";

/// Query parameter used to bootstrap a browser session (and to authenticate
/// WebSocket upgrades, which cannot carry custom headers from JS).
pub const TOKEN_QUERY: &str = "token";

/// Cookie the frontend may set so navigations survive without the query string.
pub const TOKEN_COOKIE: &str = "pylot_token";

/// The API access token for this install.
#[derive(Clone)]
pub struct ApiToken {
    value: String,
}

impl ApiToken {
    /// Load the token from `<data_dir>/api-token`, creating it on first run.
    ///
    /// The file is written with mode `0600` on Unix so other local accounts
    /// cannot read it. A pre-existing file with a blank or whitespace-only body
    /// is treated as absent and replaced, rather than yielding an empty token
    /// that would authenticate everyone.
    pub fn load_or_create(data_dir: &Path) -> anyhow::Result<Self> {
        let path = token_path(data_dir);

        if let Ok(existing) = std::fs::read_to_string(&path) {
            let trimmed = existing.trim();
            if !trimmed.is_empty() {
                return Ok(Self {
                    value: trimmed.to_string(),
                });
            }
        }

        let value = generate();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, &value)?;
        restrict_permissions(&path);
        tracing::info!("Minted a new API token at {}", path.display());

        Ok(Self { value })
    }

    /// Build a token from an explicit value (e.g. `PYLOT_API_TOKEN`).
    pub fn from_value(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.value
    }

    /// Whether `candidate` matches, compared in constant time.
    ///
    /// A short-circuiting `==` on a secret leaks its prefix through timing;
    /// since an attacker can retry against a local port at will, that is worth
    /// closing even though the window is narrow.
    pub fn matches(&self, candidate: &str) -> bool {
        constant_time_eq(self.value.as_bytes(), candidate.as_bytes())
    }
}

/// Path of the token file for a given data directory.
pub fn token_path(data_dir: &Path) -> PathBuf {
    data_dir.join(TOKEN_FILE)
}

/// 32 bytes of OS randomness, URL-safe base64 (43 chars, no padding).
fn generate() -> String {
    use base64::Engine as _;
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Best-effort `chmod 600`. A failure is logged, never fatal — on platforms
/// without Unix permissions the token file simply inherits the directory's.
fn restrict_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)) {
            tracing::warn!("Could not restrict permissions on {}: {e}", path.display());
        }
    }
    #[cfg(not(unix))]
    let _ = path;
}

/// Byte comparison whose duration depends only on the lengths, not the contents.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Pull a candidate token off a request, in order of preference:
/// `Authorization: Bearer …` → `X-Pylot-Token` → `?token=…` → `pylot_token` cookie.
fn extract_token(req: &Request<Body>) -> Option<String> {
    let headers = req.headers();

    if let Some(value) = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()) {
        // Case-insensitive scheme per RFC 7235.
        if let Some(rest) = value
            .strip_prefix("Bearer ")
            .or_else(|| value.strip_prefix("bearer "))
        {
            let rest = rest.trim();
            if !rest.is_empty() {
                return Some(rest.to_string());
            }
        }
    }

    if let Some(value) = headers.get(TOKEN_HEADER).and_then(|v| v.to_str().ok()) {
        let value = value.trim();
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }

    if let Some(query) = req.uri().query() {
        if let Some(value) = query_param(query, TOKEN_QUERY) {
            return Some(value);
        }
    }

    if let Some(value) = headers.get(header::COOKIE).and_then(|v| v.to_str().ok()) {
        if let Some(value) = cookie_value(value, TOKEN_COOKIE) {
            return Some(value);
        }
    }

    None
}

/// Read one parameter out of a raw query string, percent-decoding the value.
fn query_param(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        if k != key {
            return None;
        }
        let decoded = urlencoding::decode(v).ok()?.into_owned();
        (!decoded.is_empty()).then_some(decoded)
    })
}

/// Read one cookie out of a `Cookie:` header value.
fn cookie_value(header_value: &str, key: &str) -> Option<String> {
    header_value.split(';').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        if k.trim() != key {
            return None;
        }
        let v = v.trim();
        (!v.is_empty()).then(|| v.to_string())
    })
}

/// Axum middleware requiring a valid token on the wrapped routes.
///
/// CORS preflights (`OPTIONS`) pass through unauthenticated — a preflight
/// carries no credentials by definition, and the CORS layer is what decides
/// whether the real request is allowed to be sent at all.
pub async fn require_token(
    axum::extract::State(token): axum::extract::State<ApiToken>,
    req: Request<Body>,
    next: Next,
) -> Response {
    if req.method() == axum::http::Method::OPTIONS {
        return next.run(req).await;
    }

    match extract_token(&req) {
        Some(candidate) if token.matches(&candidate) => next.run(req).await,
        Some(_) => {
            tracing::warn!(
                path = %req.uri().path(),
                "Rejected an API request carrying an invalid token"
            );
            unauthorized("Invalid API token.")
        }
        None => unauthorized(
            "Missing API token. Send it as 'Authorization: Bearer <token>', \
             the 'X-Pylot-Token' header, or a '?token=' query parameter. \
             Run 'pylot token' to print this install's token.",
        ),
    }
}

fn unauthorized(message: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        axum::Json(serde_json::json!({
            "success": false,
            "error": message,
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request as HttpRequest;

    fn req(builder: axum::http::request::Builder) -> Request<Body> {
        builder.body(Body::empty()).unwrap()
    }

    #[test]
    fn generated_tokens_are_long_and_unique() {
        let a = generate();
        let b = generate();
        assert_ne!(a, b);
        // 32 bytes → 43 base64url chars without padding.
        assert_eq!(a.len(), 43);
        assert!(!a.contains('='), "URL-safe encoding should not be padded");
    }

    #[test]
    fn constant_time_eq_matches_semantics_of_eq() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn token_round_trips_through_the_data_dir() {
        let dir = tempfile::tempdir().unwrap();
        let first = ApiToken::load_or_create(dir.path()).unwrap();
        let second = ApiToken::load_or_create(dir.path()).unwrap();
        assert_eq!(
            first.as_str(),
            second.as_str(),
            "a second load must reuse the persisted token, not mint a new one"
        );
    }

    #[test]
    fn a_blank_token_file_is_replaced_rather_than_trusted() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(token_path(dir.path()), "   \n").unwrap();

        let token = ApiToken::load_or_create(dir.path()).unwrap();

        assert!(!token.as_str().is_empty());
        assert!(
            !token.matches(""),
            "an empty candidate must never authenticate"
        );
    }

    #[cfg(unix)]
    #[test]
    fn token_file_is_not_world_readable() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        ApiToken::load_or_create(dir.path()).unwrap();
        let mode = std::fs::metadata(token_path(dir.path()))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "group/other bits must be clear, got {mode:o}");
    }

    #[test]
    fn extracts_bearer_header() {
        let r = req(HttpRequest::get("/api/status").header(header::AUTHORIZATION, "Bearer secret"));
        assert_eq!(extract_token(&r).as_deref(), Some("secret"));
    }

    #[test]
    fn extracts_lowercase_bearer_scheme() {
        let r = req(HttpRequest::get("/api/status").header(header::AUTHORIZATION, "bearer secret"));
        assert_eq!(extract_token(&r).as_deref(), Some("secret"));
    }

    #[test]
    fn extracts_custom_header() {
        let r = req(HttpRequest::get("/api/status").header(TOKEN_HEADER, "secret"));
        assert_eq!(extract_token(&r).as_deref(), Some("secret"));
    }

    #[test]
    fn extracts_query_parameter_for_websocket_upgrades() {
        let r = req(HttpRequest::get("/ws/chat?token=secret&foo=1"));
        assert_eq!(extract_token(&r).as_deref(), Some("secret"));
    }

    #[test]
    fn percent_decodes_the_query_parameter() {
        let r = req(HttpRequest::get("/ws/chat?token=a%2Bb"));
        assert_eq!(extract_token(&r).as_deref(), Some("a+b"));
    }

    #[test]
    fn extracts_cookie() {
        let r = req(HttpRequest::get("/api/status").header(header::COOKIE, "other=1; pylot_token=secret"));
        assert_eq!(extract_token(&r).as_deref(), Some("secret"));
    }

    #[test]
    fn no_credential_yields_none() {
        let r = req(HttpRequest::get("/api/status"));
        assert_eq!(extract_token(&r), None);
    }

    #[test]
    fn empty_values_are_not_treated_as_credentials() {
        // An empty bearer, header, or query value must fall through rather than
        // produce Some("") — which would otherwise be compared against the real
        // token and (correctly) fail, but with a misleading "invalid" message.
        let r = req(HttpRequest::get("/api/status").header(header::AUTHORIZATION, "Bearer "));
        assert_eq!(extract_token(&r), None);
        let r = req(HttpRequest::get("/api/status").header(TOKEN_HEADER, "  "));
        assert_eq!(extract_token(&r), None);
        let r = req(HttpRequest::get("/ws/chat?token="));
        assert_eq!(extract_token(&r), None);
    }

    #[test]
    fn header_wins_over_query_parameter() {
        let r = req(HttpRequest::get("/api/status?token=from_query")
            .header(header::AUTHORIZATION, "Bearer from_header"));
        assert_eq!(extract_token(&r).as_deref(), Some("from_header"));
    }
}
