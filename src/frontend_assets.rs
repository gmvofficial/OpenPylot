//! Embedded web frontend.
//!
//! The Next.js static export in `frontend/out` is compiled directly into the
//! `pylot` binary at build time via `rust-embed`. This makes `pylot serve`
//! self-contained: every install channel that ships the binary (curl installer,
//! `cargo install`, Homebrew, the pip/npm wrappers) serves the exact same UI
//! with no external files, downloads, or path lookups.
//!
//! In development you can still override this with an on-disk build (see
//! `resolve_frontend_dir` in `main.rs` / the `PYLOT_FRONTEND_DIR` env var);
//! when an override is present and exists, the API server serves from disk so
//! `npm run dev`/`npm run build` changes show up without recompiling.
//!
//! Note: `frontend/out` is a build product (gitignored). A `.gitkeep` keeps the
//! folder present so this compiles even before the frontend is built; released
//! artifacts run `npm run build` first (see the CI workflows) so real assets are
//! embedded.

use axum::{
    body::Body,
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "frontend/out"]
struct FrontendAssets;

/// Whether a usable frontend (an `index.html`) was embedded at build time.
/// Used to decide startup messaging.
pub fn has_embedded_frontend() -> bool {
    FrontendAssets::get("index.html").is_some()
}

/// Axum fallback handler that serves the embedded frontend.
///
/// Resolution order for a request path `p`:
///   1. exact asset `p`
///   2. `p.html` (Next.js static export names routes like `/settings` →
///      `settings.html`)
///   3. `p/index.html` (nested route directories)
///   4. `index.html` (SPA-style fallback for client-routed paths)
pub async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    // 1. Exact match (assets, `_next/...`, `foo.html`, `favicon.ico`, ...).
    if let Some(res) = try_serve(path) {
        return res;
    }

    if !path.is_empty() {
        // 2. `<path>.html`
        if let Some(res) = try_serve(&format!("{path}.html")) {
            return res;
        }
        // 3. `<path>/index.html`
        let nested = format!("{}/index.html", path.trim_end_matches('/'));
        if let Some(res) = try_serve(&nested) {
            return res;
        }
    }

    // 4. SPA fallback to the app shell.
    if let Some(res) = try_serve("index.html") {
        return res;
    }

    (StatusCode::NOT_FOUND, "404 Not Found").into_response()
}

fn try_serve(path: &str) -> Option<Response> {
    let file = FrontendAssets::get(path)?;
    let mime = file.metadata.mimetype();
    Some(
        (
            [(header::CONTENT_TYPE, mime.to_string())],
            Body::from(file.data.into_owned()),
        )
            .into_response(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn serves_embedded_index_at_root() {
        // Requires a built frontend (frontend/out/index.html). The release CI
        // builds it; locally run `cd frontend && npm run build` first.
        if !has_embedded_frontend() {
            eprintln!("skipping: no frontend build present");
            return;
        }
        let res = static_handler("/".parse().unwrap()).await;
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn spa_fallback_serves_index_for_unknown_route() {
        if !has_embedded_frontend() {
            return;
        }
        // A client-side route with no matching file should fall back to the shell.
        let res = static_handler("/some/client/route".parse().unwrap()).await;
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn resolves_next_export_html_route() {
        if !has_embedded_frontend() {
            return;
        }
        // Next.js static export writes `/settings` as `settings.html`.
        let res = static_handler("/settings".parse().unwrap()).await;
        assert_eq!(res.status(), StatusCode::OK);
    }
}
