"use strict";

/**
 * Companion apps and MCP servers, over a running `pylot serve`.
 *
 * These are server-side concepts: a companion is a child process supervised by
 * the server and reached through its authenticated proxy, and an MCP server's
 * connection state only exists while the agent is up. So this talks to a
 * running instance over HTTP rather than going through the native bindings —
 * there is no meaningful offline answer to "is dbpylot running".
 *
 * The access token is read from the same file the server writes, so nothing
 * needs configuring:
 *
 *     const { Companions } = require("openpylot/companions");
 *
 *     const c = new Companions();
 *     for (const app of await c.list()) console.log(app.name, app.state);
 *
 *     const { url } = await c.start("dbpylot");
 *     console.log(await c.embedUrl("dbpylot"));  // a URL an iframe can load
 */

const fs = require("fs");
const os = require("os");
const path = require("path");

const DEFAULT_BASE_URL = "http://127.0.0.1:3001";

/** Where `pylot serve` writes its per-install access token. */
const TOKEN_PATH = path.join(os.homedir(), ".pylot", "data", "api-token");

/** The server refused or could not satisfy the request. */
class CompanionError extends Error {
  constructor(message, status) {
    super(message);
    this.name = "CompanionError";
    this.status = status;
  }
}

/**
 * The access token for this install, or null if there is not one yet.
 *
 * Checked in the order a caller would expect: an explicit environment variable
 * first, then the file the server writes on first run.
 */
function defaultToken() {
  if (process.env.PYLOT_API_TOKEN) return process.env.PYLOT_API_TOKEN;
  try {
    const token = fs.readFileSync(TOKEN_PATH, "utf8").trim();
    return token || null;
  } catch {
    return null;
  }
}

class Companions {
  /**
   * @param {{baseUrl?: string, token?: string, timeoutMs?: number}} [options]
   */
  constructor(options = {}) {
    this.baseUrl = (options.baseUrl || DEFAULT_BASE_URL).replace(/\/+$/, "");
    this.token = options.token || defaultToken();
    this.timeoutMs = options.timeoutMs ?? 30_000;
  }

  // ── Companions ─────────────────────────────────────────────────────

  /** Every companion, with whether it is installed, stopped or running. */
  list() {
    return this.#request("GET", "/api/companions");
  }

  /**
   * Start a companion and return `{name, port, url}`.
   *
   * Starting one that is already running is a no-op that returns the same URL,
   * so this is safe to call unconditionally.
   */
  start(name) {
    return this.#request("POST", `/api/companions/${encodeURIComponent(name)}/start`);
  }

  /** Stop a companion. Stopping one that is not running is a no-op. */
  stop(name) {
    return this.#request("POST", `/api/companions/${encodeURIComponent(name)}/stop`);
  }

  /**
   * A full URL for a companion's own web interface.
   *
   * The token rides as a query parameter because that is the only channel
   * available to an `<iframe src>` — it cannot send a header.
   */
  async embedUrl(name, { start = true } = {}) {
    if (start) await this.start(name);
    const path = `/companions/${encodeURIComponent(name)}/`;
    if (!this.token) return `${this.baseUrl}${path}`;
    return `${this.baseUrl}${path}?token=${encodeURIComponent(this.token)}`;
  }

  // ── MCP servers ────────────────────────────────────────────────────

  /** Every configured MCP server, including disabled and failing ones. */
  mcpServers() {
    return this.#request("GET", "/api/mcp/config");
  }

  /** Add or update an MCP server. Takes effect on the next restart. */
  addMcpServer({ name, command, args, url, env }) {
    if (!command && !url) {
      throw new TypeError("provide either command (stdio) or url (http/sse)");
    }
    const body = { name };
    if (command) body.command = command;
    if (args) body.args = args;
    if (url) body.url = url;
    if (env) body.env = env;
    return this.#request("POST", "/api/mcp/config", body);
  }

  removeMcpServer(name) {
    return this.#request("DELETE", `/api/mcp/config/${encodeURIComponent(name)}`);
  }

  setMcpServerEnabled(name, enabled) {
    return this.#request("PATCH", `/api/mcp/config/${encodeURIComponent(name)}`, { enabled });
  }

  /**
   * Connect to one server and report what answered. Uses a throwaway
   * connection, so this never disturbs the live registry.
   */
  testMcpServer(name) {
    return this.#request("POST", `/api/mcp/config/${encodeURIComponent(name)}/test`);
  }

  // ── Transport ──────────────────────────────────────────────────────

  async #request(method, endpoint, body) {
    const url = `${this.baseUrl}${endpoint}`;
    const headers = { "Content-Type": "application/json" };
    if (this.token) headers.Authorization = `Bearer ${this.token}`;

    // A hung server should surface as a timeout, not an indefinite await.
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.timeoutMs);

    let response;
    try {
      response = await fetch(url, {
        method,
        headers,
        body: body === undefined ? undefined : JSON.stringify(body),
        signal: controller.signal,
      });
    } catch (e) {
      if (e.name === "AbortError") {
        throw new CompanionError(`OpenPylot did not respond within ${this.timeoutMs}ms.`);
      }
      throw new CompanionError(
        `Could not reach OpenPylot at ${this.baseUrl}. Is 'pylot serve' running? (${e.message})`
      );
    } finally {
      clearTimeout(timer);
    }

    const text = await response.text();
    let payload;
    try {
      payload = text ? JSON.parse(text) : {};
    } catch {
      payload = {};
    }

    if (response.status === 401) {
      throw new CompanionError(
        "The server rejected the access token. Run 'pylot token' and pass it as " +
          "{ token }, or set PYLOT_API_TOKEN.",
        401
      );
    }
    if (!response.ok) {
      throw new CompanionError(payload.error || `HTTP ${response.status}`, response.status);
    }

    // The server wraps every response as {success, data}.
    if (payload && payload.success === false) {
      throw new CompanionError(payload.error || "request failed", response.status);
    }
    return payload && "data" in payload ? payload.data : payload;
  }
}

module.exports = { Companions, CompanionError, defaultToken, DEFAULT_BASE_URL, TOKEN_PATH };
