// ── TypeScript type definitions for the OpenPylot Frontend ─────────

// ── Chat types ──────────────────────────────────────────────────────

export interface Message {
  id: string;
  role: "user" | "assistant" | "system";
  content: string;
  timestamp: string;
  toolCalls?: ToolCall[];
  attachments?: Attachment[];
  isStreaming?: boolean;
}

export interface ToolCall {
  id: string;
  name: string;
  arguments: Record<string, unknown>;
  status: "pending" | "running" | "success" | "error";
  result?: string;
  durationMs?: number;
}

export interface Attachment {
  id: string;
  name: string;
  type: string;
  size: number;
  url?: string;
}

export interface Conversation {
  id: string;
  title: string;
  lastMessage?: string;
  messageCount: number;
  createdAt: string;
  updatedAt: string;
}

// ── Integration types ───────────────────────────────────────────────

export type IntegrationStatus = "connected" | "disconnected" | "error" | "coming_soon";

export interface Integration {
  service: string;
  displayName?: string;
  description?: string;
  icon?: string;
  status: IntegrationStatus;
  account?: string;
  connected_at?: string | null;
  connectedAt?: string;
  error?: string;
  category?: "google" | "messaging" | "developer" | "productivity" | "smart_home";
}

export interface IntegrationSettings {
  [key: string]: string | number | boolean;
}

// ── Knowledge Base types ────────────────────────────────────────────

export interface Collection {
  id: string;
  name: string;
  description?: string;
  document_count: number;
  documentCount?: number;
  totalSize?: number;
  updatedAt?: string;
}

export interface Document {
  id: string;
  title: string;
  name?: string;
  type?: string;
  source?: string;
  size?: number;
  collectionId?: string;
  tags?: string[];
  chunk_count: number;
  chunkCount?: number;
  created_at?: string;
  createdAt?: string;
}

export interface SearchResult {
  chunk: string;
  content: string;
  documentName?: string;
  document_title?: string;
  documentId?: string;
  score: number;
}

// ── Scheduler types ─────────────────────────────────────────────────

export interface ScheduledJob {
  id: string;
  name: string;
  description: string;
  schedule: string;
  lastRun?: string;
  last_run?: string;
  nextRun?: string;
  next_run?: string;
  enabled: boolean;
  status?: "idle" | "running" | "success" | "error";
  lastError?: string;
}

export interface JobHistoryEntry {
  startedAt: string;
  completedAt?: string;
  status: "success" | "error";
  result?: string;
  error?: string;
}

// ── Agent types ─────────────────────────────────────────────────────

export interface AgentStatus {
  status: string;
  online?: boolean;
  uptime: string | number;
  model: string;
  provider?: string;
  active_integrations: number;
  integrationCount?: number;
  agent_name?: string;
  toolCount?: number;
  memoryUsage?: number;
  version: string;
}

export interface AgentSettings {
  agent_name: string;
  user_name?: string | null;
  agentName?: string;
  persona: string;
  llmProvider?: string;
  llmModel?: string;
  model: string;
  temperature: number;
  maxTokens?: number;
  maxContextMessages?: number;
  max_context_messages?: number;
  max_tool_iterations?: number;
  maxToolIterations?: number;
}

export interface MemoryFact {
  id: string;
  content: string;
  category: string;
  key?: string;
  value?: string;
  created_at?: string;
  createdAt?: string;
  updatedAt?: string;
}

export interface LogEntry {
  id: string;
  timestamp: string;
  level: string;
  source?: string;
  message: string;
}

export interface ToolDefinition {
  name: string;
  description: string;
  parameters: Record<string, unknown>;
  enabled: boolean;
}

// ── Skill types ─────────────────────────────────────────────────────

export interface Skill {
  name: string;
  description: string;
  category: string;
  triggers?: string[];
  examples?: string[];
  enabled?: boolean;
}

// ── Agent Preset types ──────────────────────────────────────────────

export interface AgentPreset {
  name: string;
  description: string;
  agent_type: string;
  system_prompt?: string;
  model_override?: string | null;
  allowed_tools?: string[] | null;
  timeout_secs: number;
  max_iterations: number;
  source: "bundled" | "local" | "workspace" | string;
  source_path?: string | null;
}

// ── Social Media types ──────────────────────────────────────────────

export interface SocialPost {
  id: string;
  platform: string;
  content: string;
  hashtags?: string[];
  status: string;
  campaign_id?: string;
  published_at?: string;
  scheduled_at?: string;
  analytics?: {
    likes: number;
    shares: number;
    comments: number;
    impressions: number;
  };
}

export interface SocialCampaign {
  id: string;
  name: string;
  description?: string;
  platforms: string[];
  post_count?: number;
  status?: string;
  created_at?: string;
}

// ── Learning types ──────────────────────────────────────────────────

export interface LearningRule {
  id: string;
  rule: string;
  source?: string;
  confidence?: number;
  created_at?: string;
}

// ── MCP types ───────────────────────────────────────────────────────

export interface McpServer {
  name: string;
  url: string;
  status: string;
  tools?: string[];
}

export interface McpTool {
  name: string;
  description: string;
  server: string;
}

// ── Notification types ──────────────────────────────────────────────

export type NotificationType =
  | "rsvp_update"
  | "reminder_due"
  | "integration_status"
  | "job_completed"
  | "integration_connected"
  | "info"
  | "error";

export interface Notification {
  id: string;
  type: NotificationType;
  title: string;
  message: string;
  timestamp: string;
  read: boolean;
  data?: Record<string, unknown>;
}

// ── WebSocket message types ─────────────────────────────────────────

export type WSClientMessage = {
  type: "message";
  content: string;
  conversationId?: string;
};

export type WSServerMessage =
  | { type: "thinking" }
  | { type: "tool_call_start"; id: string; tool: string; args: Record<string, unknown> }
  | { type: "tool_call_end"; id: string; tool: string; result: string; status: "success" | "error"; durationMs: number }
  | { type: "text_delta"; content: string }
  | { type: "message_end"; id: string; conversationId: string }
  | { type: "error"; message: string };

export type WSNotificationMessage =
  | { type: "rsvp_update"; eventId: string; attendee: string; status: string }
  | { type: "reminder_due"; reminderId?: string; title: string; message?: string; conversationId?: string }
  | { type: "integration_status"; service: string; status: string; message: string }
  | { type: "job_completed"; job: string; result: string }
  | { type: "integration_connected"; service: string };

// ── API Response wrappers ───────────────────────────────────────────

export interface ApiResponse<T> {
  data: T;
  error?: string;
}

export interface PaginatedResponse<T> {
  data: T[];
  total: number;
  offset: number;
  limit: number;
}
