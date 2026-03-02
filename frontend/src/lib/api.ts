import type {
  AgentSettings,
  AgentStatus,
  Conversation,
  Integration,
  LogEntry,
  MemoryFact,
  Message,
  ScheduledJob,
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
}

export const api = new ApiClient();
export const apiClient = api;
