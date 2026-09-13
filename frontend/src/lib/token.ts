/**
 * API access token handling.
 *
 * The backend gates every `/api/*`, `/ws/*` and `/uploads/*` request behind a
 * per-install bearer token (see `src/api/auth.rs`). `pylot serve` opens the UI
 * with `?token=…` in the URL; we capture it once, persist it, and strip it from
 * the address bar so it does not linger in history or get pasted into a chat.
 *
 * Storage is `localStorage`, which is per-origin and per-browser. The token
 * never leaves this machine — it is only ever sent back to the local server
 * that issued it.
 */

const STORAGE_KEY = "pylot.apiToken";
const QUERY_KEY = "token";

/** Header the backend accepts alongside `Authorization: Bearer`. */
export const TOKEN_HEADER = "X-Pylot-Token";

let cached: string | null = null;

/**
 * Read the token from the URL (if present) and persist it, then return the
 * effective token. Safe to call repeatedly and safe during SSR, where there is
 * no `window` and this simply yields `null`.
 */
export function initToken(): string | null {
  if (typeof window === "undefined") return null;

  const params = new URLSearchParams(window.location.search);
  const fromUrl = params.get(QUERY_KEY);

  if (fromUrl) {
    setToken(fromUrl);
    // Strip the token from the visible URL without adding a history entry.
    params.delete(QUERY_KEY);
    const query = params.toString();
    const clean = window.location.pathname + (query ? `?${query}` : "") + window.location.hash;
    window.history.replaceState({}, "", clean);
    return fromUrl;
  }

  return getToken();
}

/** The current token, or null if the UI has not been handed one yet. */
export function getToken(): string | null {
  if (cached) return cached;
  if (typeof window === "undefined") return null;
  try {
    cached = window.localStorage.getItem(STORAGE_KEY);
  } catch {
    // Private browsing or blocked storage — fall back to in-memory only.
    cached = null;
  }
  return cached;
}

export function setToken(token: string): void {
  cached = token;
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(STORAGE_KEY, token);
  } catch {
    /* in-memory only */
  }
}

export function clearToken(): void {
  cached = null;
  if (typeof window === "undefined") return;
  try {
    window.localStorage.removeItem(STORAGE_KEY);
  } catch {
    /* nothing to clear */
  }
}

/**
 * Auth headers for a fetch call. Returns an empty object when no token is
 * known, so the request still goes out and the 401 surfaces as a normal error
 * the UI can explain — rather than failing silently before it leaves.
 */
export function authHeaders(): Record<string, string> {
  const token = getToken();
  return token ? { Authorization: `Bearer ${token}` } : {};
}

/**
 * Append the token to a URL as a query parameter.
 *
 * Needed where headers are impossible: WebSocket upgrades from the browser, and
 * `<img src>` / `<a href>` pointing at `/uploads/*`.
 */
export function withToken(url: string): string {
  const token = getToken();
  if (!token) return url;
  const separator = url.includes("?") ? "&" : "?";
  return `${url}${separator}${QUERY_KEY}=${encodeURIComponent(token)}`;
}
