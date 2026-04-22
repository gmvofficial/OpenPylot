import type {
  AgentSettings,
  AgentStatus,
  AgentPreset,
  Conversation,
  Integration,
  LogEntry,
  LearningRule,
  McpServer,
  McpTool,
  MemoryFact,
  Message,
  ScheduledJob,
  Skill,
  SocialCampaign,
  SocialPost,
  ToolDefinition,
  Collection,
  Document,
  SearchResult,
} from "@/types";
import { getApiBaseUrl } from "./utils";

class ApiClient {
  private baseUrl: string;

  constructor() {
    this.baseUrl = getApiBaseUrl();
  }

  private async request<T>(
    path: string,
    options?: RequestInit
  ): Promise<T> {
    const url = `${this.baseUrl}${path}`;
    const res = await fetch(url, {
      headers: {
        "Content-Type": "application/json",
        ...options?.headers,
      },
      ...options,
    });

    if (!res.ok) {
      const error = await res.text().catch(() => "Unknown error");
      throw new Error(`API error ${res.status}: ${error}`);
    }

    // Handle 204 No Content
    if (res.status === 204) return undefined as T;

    const json = await res.json();
    // Rust backend wraps responses in { success, data }.
    // Unwrap automatically so callers get the inner payload.
    if (json && typeof json === "object" && "data" in json) {
      return json.data as T;
    }
    return json as T;
  }

  // ── Chat ────────────────────────────────────────────────────────

  async sendMessage(
    message: string,
    conversationId?: string
  ): Promise<{ id: string; content: string; toolCalls?: unknown[]; conversationId: string }> {
    return this.request("/api/chat", {
      method: "POST",
      body: JSON.stringify({ message, conversation_id: conversationId }),
    });
  }

  /**
   * Open a real-time SSE stream for a chat message.
   *
   * Fires callbacks as the server pushes events:
   *   onThinking  — agent is processing (no content yet)
   *   onDelta     — incremental text token
   *   onToolEvent — tool_use_start | tool_input_delta | tool_result JSON payloads
   *   onDone      — stream complete; receives { conversationId, messageId, content }
   *   onError     — error string
   *
   * Returns an AbortController so the caller can cancel early.
   */
  streamMessage(
    message: string,
    conversationId: string | null,
    callbacks: {
      onThinking?: () => void;
      onDelta: (text: string) => void;
      onToolEvent?: (eventType: string, payload: unknown) => void;
      onDone: (payload: { conversationId: string; messageId: string; content: string }) => void;
      onError: (msg: string) => void;
    }
  ): AbortController {
    const controller = new AbortController();
    // In dev mode, bypass the Next.js rewrite proxy for SSE —
    // the proxy may buffer the streamed response, breaking real-time delivery.
    const sseBase =
      typeof window !== "undefined" && process.env.NODE_ENV === "development"
        ? "http://localhost:3001"
        : this.baseUrl;
    const url = `${sseBase}/api/chat/stream`;

    fetch(url, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        message,
        conversation_id: conversationId ?? undefined,
      }),
      signal: controller.signal,
    })
      .then(async (res) => {
        if (!res.ok || !res.body) {
          callbacks.onError(`HTTP ${res.status}`);
          return;
        }

        const reader = res.body.getReader();
        const decoder = new TextDecoder();
        let buf = "";

        // Each SSE message is separated by "\n\n".
        // Format: "event: <name>\ndata: <json>\n\n"
        const processChunk = (chunk: string) => {
          buf += chunk;
          const parts = buf.split("\n\n");
          // Keep the last (possibly incomplete) part in the buffer.
          buf = parts.pop() ?? "";

          for (const part of parts) {
            if (!part.trim()) continue;

            let eventName = "message";
            let dataLine = "";

            for (const line of part.split("\n")) {
              if (line.startsWith("event: ")) {
                eventName = line.slice(7).trim();
              } else if (line.startsWith("data: ")) {
                dataLine = line.slice(6).trim();
              }
            }

            if (!dataLine) continue;

            switch (eventName) {
              case "thinking":
                callbacks.onThinking?.();
                break;

              case "text_delta": {
                try {
                  const parsed = JSON.parse(dataLine) as { data?: { text?: string }; text?: string };
                  const text = parsed.data?.text ?? (parsed as { text?: string }).text ?? "";
                  if (text) callbacks.onDelta(text);
                } catch {
                  // ignore malformed chunk
                }
                break;
              }

              case "tool_use_start":
              case "tool_input_delta":
              case "tool_result": {
                try {
                  callbacks.onToolEvent?.(eventName, JSON.parse(dataLine));
                } catch {
                  // ignore
                }
                break;
              }

              case "done": {
                try {
                  callbacks.onDone(JSON.parse(dataLine) as { conversationId: string; messageId: string; content: string });
                } catch {
                  callbacks.onError("Malformed done payload");
                }
                break;
              }

              case "error": {
                try {
                  const parsed = JSON.parse(dataLine) as { message?: string };
                  callbacks.onError(parsed.message ?? dataLine);
                } catch {
                  callbacks.onError(dataLine);
                }
                break;
              }

              case "message_stop":
              case "usage":
                // informational — no action needed in the UI
                break;

              default:
                break;
            }
          }
        };

        while (true) {
          const { done, value } = await reader.read();
          if (done) break;
          processChunk(decoder.decode(value, { stream: true }));
        }
      })
      .catch((err: unknown) => {
        if (err instanceof Error && err.name !== "AbortError") {
          callbacks.onError(err.message);
        }
      });

    return controller;
  }

  async getConversations(): Promise<Conversation[]> {
    return this.request("/api/conversations");
  }

  async getConversation(id: string): Promise<{ id: string; title: string; messages: Message[] }> {
    return this.request(`/api/conversations/${id}`);
  }

  async deleteConversation(id: string): Promise<void> {
    return this.request(`/api/conversations/${id}`, { method: "DELETE" });
  }

  // ── Integrations ────────────────────────────────────────────────

  async getIntegrations(): Promise<Integration[]> {
    return this.request("/api/integrations");
  }

  async connectIntegration(
    service: string,
    credentials?: Record<string, string>
  ): Promise<{
    authUrl?: string;
    auth_url?: string;
    message: string;
    requires_credentials?: boolean;
    credential_fields?: Array<{
      name: string;
      label: string;
      field_type: string;
      required: boolean;
      placeholder: string;
    }>;
  }> {
    return this.request(`/api/integrations/${service}/connect`, {
      method: "POST",
      body: JSON.stringify({ credentials: credentials ?? null }),
    });
  }

  async disconnectIntegration(service: string): Promise<void> {
    return this.request(`/api/integrations/${service}`, {
      method: "DELETE",
    });
  }

  async testIntegration(
    service: string
  ): Promise<{ healthy: boolean; details: string }> {
    return this.request(`/api/integrations/${service}/test`, {
      method: "POST",
    });
  }

  // ── Knowledge Base ──────────────────────────────────────────────

  async getCollections(): Promise<Collection[]> {
    return this.request("/api/knowledge/collections");
  }

  async createCollection(name: string, description?: string): Promise<Collection> {
    return this.request("/api/knowledge/collections", {
      method: "POST",
      body: JSON.stringify({ name, description }),
    });
  }

  async deleteCollection(id: string): Promise<void> {
    return this.request(`/api/knowledge/collections/${id}`, {
      method: "DELETE",
    });
  }

  async getDocuments(collectionId?: string): Promise<Document[]> {
    if (collectionId) {
      return this.request(`/api/knowledge/collections/${collectionId}/documents`);
    }
    return this.request("/api/knowledge/documents");
  }

  async deleteDocument(id: string): Promise<void> {
    return this.request(`/api/knowledge/documents/${id}`, {
      method: "DELETE",
    });
  }

  async uploadDocument(
    collectionId: string,
    title: string,
    content: string,
    source?: string
  ): Promise<Document> {
    return this.request("/api/knowledge/documents", {
      method: "POST",
      body: JSON.stringify({
        collection_id: collectionId,
        title,
        content,
        source,
      }),
    });
  }

  async searchKnowledge(
    query: string,
    collectionId?: string,
    limit?: number
  ): Promise<SearchResult[]> {
    return this.request("/api/knowledge/search", {
      method: "POST",
      body: JSON.stringify({ query, collection_id: collectionId, limit }),
    });
  }

  // ── Scheduler ───────────────────────────────────────────────────

  async getJobs(): Promise<ScheduledJob[]> {
    return this.request("/api/jobs");
  }

  async updateJob(name: string, updates: Partial<ScheduledJob>): Promise<void> {
    return this.request(`/api/jobs/${name}`, {
      method: "PATCH",
      body: JSON.stringify(updates),
    });
  }

  async runJob(name: string): Promise<{ result: string }> {
    return this.request(`/api/jobs/${name}/run`, { method: "POST" });
  }

  // ── Tools ───────────────────────────────────────────────────────

  async getTools(): Promise<ToolDefinition[]> {
    return this.request("/api/tools");
  }

  // ── Agent ───────────────────────────────────────────────────────

  async getStatus(): Promise<AgentStatus> {
    return this.request("/api/status");
  }

  async getMemory(): Promise<MemoryFact[]> {
    return this.request("/api/memory");
  }

  async updateMemoryFact(id: string, content: string): Promise<void> {
    return this.request(`/api/memory/${id}`, {
      method: "PATCH",
      body: JSON.stringify({ content }),
    });
  }

  async deleteMemoryFact(id: string): Promise<void> {
    return this.request(`/api/memory/${id}`, { method: "DELETE" });
  }

  async getSettings(): Promise<AgentSettings> {
    return this.request("/api/settings");
  }

  async updateSettings(settings: Partial<AgentSettings>): Promise<void> {
    return this.request("/api/settings", {
      method: "PATCH",
      body: JSON.stringify(settings),
    });
  }

  async getLogs(options?: {
    level?: string;
    source?: string;
    limit?: number;
    offset?: number;
  }): Promise<LogEntry[]> {
    const params = new URLSearchParams();
    if (options?.level) params.set("level", options.level);
    if (options?.source) params.set("source", options.source);
    if (options?.limit) params.set("limit", String(options.limit));
    if (options?.offset) params.set("offset", String(options.offset));
    const qs = params.toString();
    return this.request(`/api/logs${qs ? `?${qs}` : ""}`);
  }

  // ── Skills ──────────────────────────────────────────────────────

  async getSkills(): Promise<Skill[]> {
    return this.request("/api/skills");
  }

  async updateSkill(name: string, enabled: boolean): Promise<unknown> {
    return this.request("/api/skills/update", {
      method: "POST",
      body: JSON.stringify({ skill_key: name, enabled }),
    });
  }

  async deleteSkill(name: string): Promise<unknown> {
    return this.request(`/api/skills/delete/${encodeURIComponent(name)}`, {
      method: "DELETE",
    });
  }

  // ── Social Media ────────────────────────────────────────────────

  async getSocialPosts(): Promise<SocialPost[]> {
    return this.request("/api/social/posts");
  }

  async createSocialPost(post: {
    platform: string;
    content: string;
    hashtags?: string[];
    campaign_id?: string;
  }): Promise<SocialPost> {
    return this.request("/api/social/posts", {
      method: "POST",
      body: JSON.stringify(post),
    });
  }

  async getSocialCampaigns(): Promise<SocialCampaign[]> {
    return this.request("/api/social/campaigns");
  }

  async createSocialCampaign(campaign: {
    name: string;
    description?: string;
    platforms: string[];
  }): Promise<SocialCampaign> {
    return this.request("/api/social/campaigns", {
      method: "POST",
      body: JSON.stringify(campaign),
    });
  }

  // ── Learning ────────────────────────────────────────────────────

  async getLearningRules(): Promise<LearningRule[]> {
    return this.request("/api/learning/rules");
  }

  async submitFeedback(feedback: {
    session_id: string;
    turn_id: string;
    rating: number;
    comment?: string;
  }): Promise<void> {
    return this.request("/api/learning/feedback", {
      method: "POST",
      body: JSON.stringify(feedback),
    });
  }

  // ── MCP ─────────────────────────────────────────────────────────

  async getMcpServers(): Promise<McpServer[]> {
    return this.request("/api/mcp/servers");
  }

  async getMcpTools(): Promise<McpTool[]> {
    return this.request("/api/mcp/tools");
  }

  // ── Sub-Agents ──────────────────────────────────────────────────

  async getSubAgents() {
    return this.request("/api/agents");
  }

  async spawnSubAgent(params: { name: string; task: string; model?: string }) {
    return this.request("/api/agents", {
      method: "POST",
      body: JSON.stringify(params),
    });
  }

  async getSubAgent(id: string) {
    return this.request(`/api/agents/${id}`);
  }

  async cancelSubAgent(id: string) {
    return this.request(`/api/agents/${id}`, { method: "DELETE" });
  }

  // ── Agent Presets (plug-and-play manifests) ─────────────────────

  async getAgentPresets() {
    return this.request<{ presets: AgentPreset[]; count: number; user_dir?: string }>(
      "/api/agents/presets"
    );
  }

  async getAgentPreset(name: string) {
    return this.request<AgentPreset>(`/api/agents/presets/${encodeURIComponent(name)}`);
  }

  // ── Memory v2 ──────────────────────────────────────────────────

  async getMemoryUnits() {
    return this.request("/api/memory/v2/units");
  }

  async searchMemoryV2(query: string) {
    return this.request("/api/memory/v2/search", {
      method: "POST",
      body: JSON.stringify({ query }),
    });
  }

  // ── Social Platforms ───────────────────────────────────────────

  async getSocialPlatforms() {
    return this.request("/api/social/platforms");
  }

  async publishSocialPost(id: string) {
    return this.request(`/api/social/posts/${id}/publish`, { method: "POST" });
  }

  async connectSocialPlatform(platform: string) {
    return this.request(`/api/social/connect/${platform}`, { method: "POST" });
  }

  async disconnectSocialPlatform(platform: string) {
    return this.request(`/api/social/disconnect/${platform}`, { method: "POST" });
  }
}

export const api = new ApiClient();
export const apiClient = api;
