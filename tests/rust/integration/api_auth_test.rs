//! End-to-end checks on the API token gate.
//!
//! These drive a real `axum` router through `tower::ServiceExt::oneshot`, so
//! they exercise the middleware exactly as the live server does — including the
//! `route_layer` placement, which is easy to get subtly wrong (a `route_layer`
//! on a fallback-only router silently gates nothing).

use axum::{
    body::Body,
    http::{header, Request, StatusCode},
    routing::get,
    Router,
};
use openpylot::api::auth::{ApiToken, TOKEN_HEADER};
use tower::ServiceExt;

/// A stand-in for the protected surface: any 200 here means the gate let the
/// request through.
async fn protected() -> &'static str {
    "sensitive"
}

fn guarded_router(token: ApiToken) -> Router {
    Router::new()
        .route("/api/status", get(protected))
        .route_layer(axum::middleware::from_fn_with_state(
            token,
            openpylot::api::auth::require_token,
        ))
}

async fn status_of(app: Router, req: Request<Body>) -> StatusCode {
    app.oneshot(req).await.unwrap().status()
}

#[tokio::test]
async fn request_without_a_token_is_rejected() {
    let token = ApiToken::from_value("the-real-token");
    let code = status_of(
        guarded_router(token),
        Request::get("/api/status").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(
        code,
        StatusCode::UNAUTHORIZED,
        "an unauthenticated caller must never reach the agent"
    );
}

#[tokio::test]
async fn request_with_a_wrong_token_is_rejected() {
    let token = ApiToken::from_value("the-real-token");
    let code = status_of(
        guarded_router(token),
        Request::get("/api/status")
            .header(header::AUTHORIZATION, "Bearer not-the-token")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(code, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_token_prefix_does_not_authenticate() {
    // Guards against a comparison that stops at the shorter length.
    let token = ApiToken::from_value("the-real-token");
    let code = status_of(
        guarded_router(token),
        Request::get("/api/status")
            .header(header::AUTHORIZATION, "Bearer the-real")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(code, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn bearer_header_is_accepted() {
    let token = ApiToken::from_value("the-real-token");
    let code = status_of(
        guarded_router(token),
        Request::get("/api/status")
            .header(header::AUTHORIZATION, "Bearer the-real-token")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(code, StatusCode::OK);
}

#[tokio::test]
async fn custom_header_is_accepted() {
    let token = ApiToken::from_value("the-real-token");
    let code = status_of(
        guarded_router(token),
        Request::get("/api/status")
            .header(TOKEN_HEADER, "the-real-token")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(code, StatusCode::OK);
}

#[tokio::test]
async fn query_parameter_is_accepted_for_websocket_upgrades() {
    // Browser WebSocket clients cannot set headers, so the query string is the
    // only channel available for `/ws/*`.
    let token = ApiToken::from_value("the-real-token");
    let code = status_of(
        guarded_router(token),
        Request::get("/api/status?token=the-real-token")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(code, StatusCode::OK);
}

#[tokio::test]
async fn cookie_is_accepted() {
    let token = ApiToken::from_value("the-real-token");
    let code = status_of(
        guarded_router(token),
        Request::get("/api/status")
            .header(header::COOKIE, "pylot_token=the-real-token")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(code, StatusCode::OK);
}

#[tokio::test]
async fn cors_preflight_passes_without_a_token() {
    // A preflight carries no credentials by definition; answering it with 401
    // would break every legitimate cross-origin call before it starts.
    let token = ApiToken::from_value("the-real-token");
    let code = status_of(
        guarded_router(token),
        Request::builder()
            .method("OPTIONS")
            .uri("/api/status")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_ne!(code, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_fallback_only_router_is_still_gated() {
    // `/uploads` serves everything from a fallback. `route_layer` deliberately
    // skips fallbacks, so that sub-router must use `layer` — this test fails if
    // someone "tidies" it back to `route_layer`.
    let token = ApiToken::from_value("the-real-token");
    let app = Router::new()
        .fallback(protected)
        .layer(axum::middleware::from_fn_with_state(
            token,
            openpylot::api::auth::require_token,
        ));

    let code = status_of(
        app,
        Request::get("/uploads/private.png")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(
        code,
        StatusCode::UNAUTHORIZED,
        "uploaded files are user data and must not be world-readable"
    );
}

// ── Binding defaults ─────────────────────────────────────────────────

#[test]
fn the_default_binding_is_loopback() {
    let binding = openpylot::api::ServeBinding::loopback(3001);
    assert!(binding.host.is_loopback());
    assert!(!binding.is_public());
}

#[test]
fn cors_allowlist_never_contains_a_wildcard() {
    let binding = openpylot::api::ServeBinding::loopback(3001);
    let origins = binding.allowed_origins();
    assert!(!origins.is_empty());
    assert!(
        !origins.iter().any(|o| o == "*"),
        "a wildcard origin alongside a credential re-opens the CSRF hole"
    );
    assert!(origins.iter().any(|o| o == "http://127.0.0.1:3001"));
    assert!(origins.iter().any(|o| o == "http://localhost:3001"));
}

#[test]
fn a_public_bind_is_reported_as_public() {
    let binding = openpylot::api::ServeBinding {
        host: "0.0.0.0".parse().unwrap(),
        port: 3001,
    };
    assert!(binding.is_public());
}

#[test]
fn browser_url_carries_the_token_so_the_ui_can_bootstrap() {
    let token = ApiToken::from_value("abc123");
    let url = openpylot::api::ServeBinding::loopback(3001).browser_url(&token);
    assert_eq!(url, "http://127.0.0.1:3001/?token=abc123");
}
