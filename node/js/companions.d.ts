/**
 * Companion apps and MCP servers, over a running `pylot serve`.
 *
 * These only exist while the server is up, so they are reached over HTTP
 * rather than through the native bindings.
 */

export declare const DEFAULT_BASE_URL: string;
/** Where `pylot serve` writes its per-install access token. */
export declare const TOKEN_PATH: string;

/** The server refused or could not satisfy the request. */
export declare class CompanionError extends Error {
  status?: number;
}

/** The access token for this install, or null if there is not one yet. */
export declare function defaultToken(): string | null;

export type CompanionState =
  | "not_installed"
  | "stopped"
  | "starting"
  | "running"
  | "failed";

export interface Companion {
  name: string;
  title: string;
  description: string;
  state: CompanionState;
  /** Loopback port the child came up on; present only while running. */
  port?: number;
  /** Why it failed to start; present only in the failed state. */
  error?: string;
  /** Mount path to point an iframe at; present only while running. */
  url: string | null;
}

export interface McpConfiguredServer {
  name: string;
  transport: string;
  /** The command or URL, whichever this server uses. */
  target: string;
  enabled: boolean;
  connected: boolean;
  tool_count: number;
}

export interface McpServerInput {
  name: string;
  /** For a stdio server. */
  command?: string;
  args?: string[];
  /** For an HTTP/SSE server. */
  url?: string;
  env?: Record<string, string>;
}

export interface CompanionsOptions {
  baseUrl?: string;
  token?: string;
  timeoutMs?: number;
}

export declare class Companions {
  constructor(options?: CompanionsOptions);
  readonly baseUrl: string;
  readonly token: string | null;

  /** Every companion, with whether it is installed, stopped or running. */
  list(): Promise<Companion[]>;
  /** Start a companion. Already-running is a no-op returning the same URL. */
  start(name: string): Promise<{ name: string; port: number; url: string }>;
  stop(name: string): Promise<boolean>;
  /** A full URL for a companion's own web interface, token included. */
  embedUrl(name: string, options?: { start?: boolean }): Promise<string>;

  /** Every configured MCP server, including disabled and failing ones. */
  mcpServers(): Promise<McpConfiguredServer[]>;
  /** Add or update an MCP server. Takes effect on the next restart. */
  addMcpServer(server: McpServerInput): Promise<{ replaced: boolean; restart_required: boolean }>;
  removeMcpServer(name: string): Promise<boolean>;
  setMcpServerEnabled(name: string, enabled: boolean): Promise<boolean>;
  /** Connect to one server and report what answered. */
  testMcpServer(name: string): Promise<{ connected: boolean; message: string; tools: string[] }>;
}
