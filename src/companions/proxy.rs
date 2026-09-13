//! Authenticated reverse proxy for companion web apps.
//!
//! # The sub-path problem
//!
//! A companion is written to live at the root of its own origin: its frontend
//! fetches `/api/…`, opens `ws://host/api/…`, and loads `/app.js`. Mounted under
//! `/companions/dbpylot/`, every one of those absolute paths misses.
//!
//! Rewriting the HTML would only fix the paths that appear literally in it, not
//! the ones a bundled script builds at runtime — which is most of them. So the
//! proxy injects a small shim ahead of the app's own scripts that wraps `fetch`,
//! `EventSource` and `WebSocket` and prefixes any root-absolute URL. That works
//! regardless of how the companion builds its URLs, and needs no cooperation
//! from the companion itself.

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};

use super::{CompanionRegistry, MOUNT_PREFIX};

/// Headers that describe a specific connection and must not be forwarded.
///
/// Copying `content-length` or a `transfer-encoding` from the upstream response
/// onto a body the proxy may have rewritten produces a truncated page — a
/// genuinely confusing bug, since the HTML arrives but stops mid-tag.
const HOP_BY_HOP: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
    "content-length",
    "content-encoding",
];

/// Proxy a request to a running companion.
///
/// Mounted behind the same token gate as the rest of the API, so a companion is
/// never independently reachable: it binds loopback on an ephemeral port, and
/// this is the only route to it.
pub async fn handle(State(registry): State<CompanionRegistry>, request: Request) -> Response {
    // Mounted with `nest_service`, so the URI here has already had
    // `/companions` removed: `/companions/dbpylot/api/x` arrives as
    // `/dbpylot/api/x`. The companion name is therefore the first segment.
    let Some((name, rest)) = split_mounted_path(request.uri().path()) else {
        return unknown_companion();
    };

    let Some(port) = registry.port(&name).await else {
        return not_running(&name);
    };

    let prefix = mount_path(&name);
    let upstream = upstream_url(port, &rest, request.uri().query());

    let client = reqwest::Client::builder()
        // A companion's own streaming endpoints can stay open indefinitely, so
        // no overall timeout — only a connect timeout.
        .connect_timeout(std::time::Duration::from_secs(5))
        .build();
    let Ok(client) = client else {
        return upstream_error(&name, "could not build an HTTP client");
    };

    let method = request.method().clone();
    let headers = forwardable_request_headers(request.headers());
    let body = match axum::body::to_bytes(request.into_body(), 64 * 1024 * 1024).await {
        Ok(bytes) => bytes,
        Err(e) => return upstream_error(&name, &format!("could not read the request body: {e}")),
    };

    let response = client
        .request(method, &upstream)
        .headers(headers)
        .body(body)
        .send()
        .await;

    let response = match response {
        Ok(r) => r,
        Err(e) => return upstream_error(&name, &e.to_string()),
    };

    let status = response.status();
    let upstream_headers = response.headers().clone();
    let content_type = upstream_headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let bytes = match response.bytes().await {
        Ok(b) => b,
        Err(e) => return upstream_error(&name, &format!("could not read the response: {e}")),
    };

    // HTML is the one thing worth rewriting: it is where the shim has to land.
    let body = if content_type.contains("text/html") {
        match String::from_utf8(bytes.to_vec()) {
            Ok(html) => Body::from(inject_shim(&html, &prefix)),
            // Not valid UTF-8 despite the content type — pass it through rather
            // than corrupting it.
            Err(e) => Body::from(e.into_bytes()),
        }
    } else {
        Body::from(bytes)
    };

    let mut out = Response::builder().status(status);
    if let Some(map) = out.headers_mut() {
        for (key, value) in upstream_headers.iter() {
            if HOP_BY_HOP.contains(&key.as_str().to_ascii_lowercase().as_str()) {
                continue;
            }
            map.insert(key.clone(), value.clone());
        }
    }
    out.body(body)
        .unwrap_or_else(|_| upstream_error(&name, "could not build the response"))
}

/// Where a companion is mounted, without a trailing slash.
pub fn mount_path(name: &str) -> String {
    format!("{MOUNT_PREFIX}/{name}")
}

/// Split a mount-relative path into the companion name and the path to forward.
///
/// `/dbpylot/api/x` → `("dbpylot", "/api/x")`
/// `/dbpylot/`      → `("dbpylot", "/")`
/// `/dbpylot`       → `("dbpylot", "/")`
///
/// Returns `None` when there is no name segment at all.
pub fn split_mounted_path(path: &str) -> Option<(String, String)> {
    let trimmed = path.trim_start_matches('/');
    let (name, rest) = match trimmed.split_once('/') {
        Some((name, rest)) => (name, rest),
        None => (trimmed, ""),
    };
    if name.is_empty() {
        return None;
    }
    // A companion name is a path segment we chose; anything else is a probe.
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return None;
    }
    Some((name.to_string(), format!("/{rest}")))
}

/// Build the upstream URL for an already-split path.
pub fn upstream_url(port: u16, path: &str, query: Option<&str>) -> String {
    match query {
        Some(q) if !q.is_empty() => format!("http://127.0.0.1:{port}{path}?{q}"),
        _ => format!("http://127.0.0.1:{port}{path}"),
    }
}

/// Request headers safe to forward upstream.
///
/// The parent's `Authorization` is deliberately dropped: it is OpenPylot's
/// credential, and forwarding it would hand the companion a token that unlocks
/// the agent, including shell access.
fn forwardable_request_headers(headers: &HeaderMap) -> reqwest::header::HeaderMap {
    let mut out = reqwest::header::HeaderMap::new();
    for (key, value) in headers.iter() {
        let name = key.as_str().to_ascii_lowercase();
        if HOP_BY_HOP.contains(&name.as_str())
            || name == "host"
            || name == "authorization"
            || name == "cookie"
        {
            continue;
        }
        if let Ok(k) = reqwest::header::HeaderName::from_bytes(key.as_ref()) {
            if let Ok(v) = reqwest::header::HeaderValue::from_bytes(value.as_bytes()) {
                out.insert(k, v);
            }
        }
    }
    out
}

/// The script that re-points a companion's absolute URLs at its mount prefix.
///
/// Wrapping the three URL-taking browser APIs covers everything a bundled app
/// can do at runtime, which static rewriting of the HTML cannot.
pub fn shim(prefix: &str) -> String {
    let prefix = js_string_literal(prefix);
    format!(
        r#"<script>
(function () {{
  var P = {prefix};
  // Only root-absolute paths need moving, and never twice.
  function fix(u) {{
    if (typeof u !== "string") return u;
    if (u.indexOf(P + "/") === 0) return u;
    if (u.charAt(0) === "/" && u.charAt(1) !== "/") return P + u;
    return u;
  }}
  function fixWs(u) {{
    if (typeof u !== "string") return u;
    try {{
      var parsed = new URL(u, location.href);
      if (parsed.host !== location.host) return u;
      if (parsed.pathname.indexOf(P + "/") === 0) return u;
      parsed.pathname = P + parsed.pathname;
      return parsed.toString();
    }} catch (e) {{ return u; }}
  }}

  var origFetch = window.fetch;
  window.fetch = function (input, init) {{
    if (input && typeof input === "object" && "url" in input) {{
      return origFetch(new Request(fix(input.url), input), init);
    }}
    return origFetch(fix(input), init);
  }};

  if (window.EventSource) {{
    var OrigES = window.EventSource;
    window.EventSource = function (url, cfg) {{ return new OrigES(fix(url), cfg); }};
    window.EventSource.prototype = OrigES.prototype;
  }}

  if (window.WebSocket) {{
    var OrigWS = window.WebSocket;
    window.WebSocket = function (url, protocols) {{
      return protocols === undefined
        ? new OrigWS(fixWs(url))
        : new OrigWS(fixWs(url), protocols);
    }};
    window.WebSocket.prototype = OrigWS.prototype;
  }}

  var origOpen = XMLHttpRequest.prototype.open;
  XMLHttpRequest.prototype.open = function (method, url) {{
    var rest = Array.prototype.slice.call(arguments, 2);
    return origOpen.apply(this, [method, fix(url)].concat(rest));
  }};
}})();
</script>"#
    )
}

/// Quote a string for embedding inside a `<script>` block.
///
/// `{:?}` alone is not enough: it escapes quotes and backslashes, but the HTML
/// parser ends a script element at the literal bytes `</script>` no matter what
/// JavaScript string it appears inside. Escaping the slash closes that hole.
fn js_string_literal(value: &str) -> String {
    format!("{value:?}").replace("</", "<\\/")
}

/// Put the shim and a `<base>` into a companion's HTML.
///
/// The shim must run before any of the app's own scripts, so it goes as early
/// in `<head>` as possible; `<base>` handles relative URLs, which the shim
/// deliberately leaves alone.
pub fn inject_shim(html: &str, prefix: &str) -> String {
    let injection = format!(
        "<base href=\"{}/\">{}",
        html_attribute(prefix),
        shim(prefix)
    );

    // Prefer immediately after <head>, so nothing the app declares runs first.
    if let Some(i) = find_ci(html, "<head>") {
        let at = i + "<head>".len();
        return format!("{}{}{}", &html[..at], injection, &html[at..]);
    }
    if let Some(i) = find_ci(html, "<html>") {
        let at = i + "<html>".len();
        return format!("{}<head>{}</head>{}", &html[..at], injection, &html[at..]);
    }
    // No recognisable structure — prepend rather than give up, so a bare
    // fragment still gets its URLs corrected.
    format!("{injection}{html}")
}

/// Escape a value for use inside a double-quoted HTML attribute.
fn html_attribute(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Case-insensitive substring search returning a byte index.
fn find_ci(haystack: &str, needle: &str) -> Option<usize> {
    haystack.to_ascii_lowercase().find(&needle.to_ascii_lowercase())
}

fn unknown_companion() -> Response {
    (
        StatusCode::NOT_FOUND,
        axum::Json(serde_json::json!({
            "success": false,
            "error": "No companion at that path.",
        })),
    )
        .into_response()
}

fn not_running(name: &str) -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        axum::Json(serde_json::json!({
            "success": false,
            "error": format!(
                "The '{name}' companion is not running. Start it from the Companions page, \
                 or run 'pylot companions start {name}'."
            ),
        })),
    )
        .into_response()
}

fn upstream_error(name: &str, detail: &str) -> Response {
    tracing::warn!("Companion '{name}' proxy error: {detail}");
    (
        StatusCode::BAD_GATEWAY,
        axum::Json(serde_json::json!({
            "success": false,
            "error": format!("The '{name}' companion did not answer: {detail}"),
        })),
    )
        .into_response()
}

/// Content types whose bodies are text and may legitimately be rewritten.
#[allow(dead_code)]
fn is_text(content_type: &HeaderValue) -> bool {
    content_type
        .to_str()
        .map(|v| v.starts_with("text/") || v.contains("json") || v.contains("javascript"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── URL translation ──────────────────────────────────────────────

    #[test]
    fn the_companion_name_is_the_first_segment() {
        let (name, rest) = split_mounted_path("/dbpylot/api/x").unwrap();
        assert_eq!(name, "dbpylot");
        assert_eq!(rest, "/api/x");
    }

    #[test]
    fn a_trailing_slash_maps_to_the_child_root() {
        // This is the URL a browser actually lands on, and route patterns of the
        // form `/{name}` + `/{name}/{*rest}` miss it entirely.
        let (name, rest) = split_mounted_path("/dbpylot/").unwrap();
        assert_eq!(name, "dbpylot");
        assert_eq!(rest, "/");
    }

    #[test]
    fn no_trailing_slash_also_maps_to_the_child_root() {
        let (name, rest) = split_mounted_path("/dbpylot").unwrap();
        assert_eq!(name, "dbpylot");
        assert_eq!(rest, "/");
    }

    #[test]
    fn a_deep_path_is_preserved_whole() {
        let (_, rest) = split_mounted_path("/dbpylot/a/b/c.js").unwrap();
        assert_eq!(rest, "/a/b/c.js");
    }

    #[test]
    fn an_empty_path_has_no_companion() {
        assert!(split_mounted_path("/").is_none());
        assert!(split_mounted_path("").is_none());
    }

    #[test]
    fn a_name_that_is_not_a_plain_segment_is_rejected() {
        // Guards against traversal-shaped probes reaching the registry lookup.
        assert!(split_mounted_path("/../etc/passwd").is_none());
        assert!(split_mounted_path("/a.b/x").is_none());
    }

    #[test]
    fn the_upstream_url_targets_loopback() {
        assert_eq!(
            upstream_url(9000, "/api/x", None),
            "http://127.0.0.1:9000/api/x"
        );
    }

    #[test]
    fn the_query_string_survives() {
        assert_eq!(
            upstream_url(9000, "/api/x", Some("a=1&b=2")),
            "http://127.0.0.1:9000/api/x?a=1&b=2"
        );
    }

    #[test]
    fn an_empty_query_adds_no_question_mark() {
        assert_eq!(upstream_url(9000, "/x", Some("")), "http://127.0.0.1:9000/x");
    }

    #[test]
    fn mount_path_has_no_trailing_slash() {
        assert_eq!(mount_path("dbpylot"), "/companions/dbpylot");
    }

    // ── Header filtering ─────────────────────────────────────────────

    #[test]
    fn the_parents_credentials_are_never_forwarded() {
        // Handing the companion OpenPylot's token would give it shell access.
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, "Bearer parent-secret".parse().unwrap());
        headers.insert(header::COOKIE, "pylot_token=parent-secret".parse().unwrap());
        headers.insert(header::ACCEPT, "text/html".parse().unwrap());

        let out = forwardable_request_headers(&headers);

        assert!(out.get("authorization").is_none());
        assert!(out.get("cookie").is_none());
        assert!(out.get("accept").is_some(), "ordinary headers still forward");
    }

    #[test]
    fn hop_by_hop_headers_are_dropped() {
        let mut headers = HeaderMap::new();
        headers.insert(header::CONNECTION, "keep-alive".parse().unwrap());
        headers.insert(header::HOST, "example.com".parse().unwrap());
        headers.insert("x-custom", "kept".parse().unwrap());

        let out = forwardable_request_headers(&headers);

        assert!(out.get("connection").is_none());
        assert!(out.get("host").is_none(), "the upstream sets its own Host");
        assert_eq!(out.get("x-custom").unwrap(), "kept");
    }

    #[test]
    fn content_length_is_on_the_drop_list() {
        // Forwarding it onto a rewritten body truncates the page.
        assert!(HOP_BY_HOP.contains(&"content-length"));
        assert!(HOP_BY_HOP.contains(&"content-encoding"));
    }

    // ── Shim injection ───────────────────────────────────────────────

    #[test]
    fn the_shim_lands_immediately_after_head() {
        let html = "<!doctype html><html><head><script src=\"/app.js\"></script></head><body></body></html>";
        let out = inject_shim(html, "/companions/dbpylot");

        let head = out.find("<head>").unwrap();
        let shim = out.find("window.fetch =").unwrap();
        let app = out.find("/app.js").unwrap();
        assert!(head < shim && shim < app, "the shim must run before the app's scripts");
    }

    #[test]
    fn a_base_tag_is_added_for_relative_urls() {
        let out = inject_shim("<html><head></head></html>", "/companions/dbpylot");
        assert!(out.contains("<base href=\"/companions/dbpylot/\">"));
    }

    #[test]
    fn uppercase_head_is_matched_too() {
        let out = inject_shim("<HTML><HEAD></HEAD></HTML>", "/companions/x");
        assert!(out.contains("window.fetch ="));
    }

    #[test]
    fn html_without_a_head_gets_one() {
        let out = inject_shim("<html><body>hi</body></html>", "/companions/x");
        assert!(out.contains("<head>"));
        assert!(out.contains("window.fetch ="));
        assert!(out.contains("hi"), "the original content must survive");
    }

    #[test]
    fn a_bare_fragment_still_gets_the_shim() {
        let out = inject_shim("<div>hello</div>", "/companions/x");
        assert!(out.contains("window.fetch ="));
        assert!(out.ends_with("<div>hello</div>"));
    }

    #[test]
    fn the_original_markup_is_never_lost() {
        let html = "<!doctype html><html><head><title>T</title></head><body><p>Body</p></body></html>";
        let out = inject_shim(html, "/companions/x");
        assert!(out.contains("<title>T</title>"));
        assert!(out.contains("<p>Body</p>"));
        assert!(out.starts_with("<!doctype html>"));
    }

    // ── The shim script itself ───────────────────────────────────────

    #[test]
    fn the_shim_carries_the_prefix_as_a_quoted_string() {
        let js = shim("/companions/dbpylot");
        assert!(js.contains(r#"var P = "/companions/dbpylot";"#), "{js}");
    }

    #[test]
    fn the_shim_wraps_every_url_taking_browser_api() {
        // Missing one means that transport silently 404s under the mount.
        let js = shim("/companions/x");
        assert!(js.contains("window.fetch ="));
        assert!(js.contains("window.EventSource ="));
        assert!(js.contains("window.WebSocket ="));
        assert!(js.contains("XMLHttpRequest.prototype.open ="));
    }

    #[test]
    fn the_shim_guards_against_double_prefixing() {
        // Without this, a retried request accumulates the prefix twice.
        let js = shim("/companions/x");
        assert!(js.contains(r#"u.indexOf(P + "/") === 0"#));
    }

    #[test]
    fn the_shim_leaves_protocol_relative_urls_alone() {
        // `//cdn.example.com/x` is absolute, not root-relative.
        let js = shim("/companions/x");
        assert!(js.contains(r#"u.charAt(1) !== "/""#));
    }

    #[test]
    fn a_prefix_with_a_quote_cannot_break_out_of_the_script() {
        // `{:?}` escapes it; a naive `{}` would be an injection.
        let js = shim(r#"/companions/"><script>alert(1)</script>"#);
        assert!(
            !js.contains("<script>alert(1)</script>"),
            "prefix must be escaped into the string literal: {js}"
        );
    }
}
