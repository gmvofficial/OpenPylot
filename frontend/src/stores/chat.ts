import { create } from "zustand";
import type { Message, Conversation, ToolCall, WSServerMessage } from "@/types";
import { WebSocketClient } from "@/lib/websocket";
import { generateId } from "@/lib/utils";
import { api } from "@/lib/api";

interface ChatState {
  // Data
  messages: Message[];
  conversations: Conversation[];
  activeConversationId: string | null;
  isStreaming: boolean;
  streamingContent: string;
  streamingToolCalls: ToolCall[];
  isConnected: boolean;
  isSidePanelOpen: boolean;
  sidePanelContent: ToolCall | null;

  // WebSocket (notifications / presence)
  wsClient: WebSocketClient | null;

  // Active SSE abort handle
  _sseAbort: AbortController | null;

  // Actions
  connect: () => void;
  disconnect: () => void;
  sendMessage: (content: string) => void;
  loadConversations: () => Promise<void>;
  loadConversation: (id: string) => Promise<void>;
  newConversation: () => void;
  deleteConversation: (id: string) => Promise<void>;
  setSidePanelContent: (toolCall: ToolCall | null) => void;
  toggleSidePanel: () => void;
}

export const useChatStore = create<ChatState>((set, get) => ({
  messages: [],
  conversations: [],
  activeConversationId: null,
  isStreaming: false,
  streamingContent: "",
  streamingToolCalls: [],
  isConnected: false,
  isSidePanelOpen: false,
  sidePanelContent: null,
  wsClient: null,
  _sseAbort: null,

  // WebSocket is kept for notifications / pings but messages now go via SSE.
  connect() {
    const existing = get().wsClient;
    if (existing?.connected) return;
    if (existing) existing.disconnect();

    const client = new WebSocketClient("/ws/chat");

    client.onOpen(() => set({ isConnected: true }));
    client.onClose(() => set({ isConnected: false }));

    // Handle any WS messages that still arrive (e.g. legacy path or server push).
    client.onMessage((msg: WSServerMessage) => {
      const state = get();
      switch (msg.type) {
        case "thinking":
          set({ isStreaming: true, streamingContent: "", streamingToolCalls: [] });
          break;
        case "tool_call_start": {
          const toolCall: ToolCall = {
            id: msg.id,
            name: msg.tool,
            arguments: msg.args,
            status: "running",
          };
          set({ streamingToolCalls: [...state.streamingToolCalls, toolCall] });
          break;
        }
        case "tool_call_end": {
          const updated = state.streamingToolCalls.map((tc) =>
            tc.id === msg.id
              ? { ...tc, status: msg.status as ToolCall["status"], result: msg.result, durationMs: msg.durationMs }
              : tc
          );
          set({ streamingToolCalls: updated });
          break;
        }
        case "text_delta":
          set({ streamingContent: state.streamingContent + msg.content });
          break;
        case "message_end": {
          const assistantMsg: Message = {
            id: msg.id,
            role: "assistant",
            content: state.streamingContent,
            timestamp: new Date().toISOString(),
            toolCalls: state.streamingToolCalls.length > 0 ? state.streamingToolCalls : undefined,
          };
          set({
            messages: [...state.messages, assistantMsg],
            isStreaming: false,
            streamingContent: "",
            streamingToolCalls: [],
            activeConversationId: msg.conversationId || state.activeConversationId,
          });
          get().loadConversations();
          break;
        }
        case "error":
          set({ isStreaming: false, streamingContent: "" });
          break;
      }
    });

    client.connect();
    set({ wsClient: client });
  },

  disconnect() {
    get().wsClient?.disconnect();
    get()._sseAbort?.abort();
    set({ wsClient: null, isConnected: false, _sseAbort: null });
  },

  sendMessage(content: string) {
    const state = get();

    // Cancel any in-flight SSE stream.
    state._sseAbort?.abort();

    const userMsg: Message = {
      id: generateId(),
      role: "user",
      content,
      timestamp: new Date().toISOString(),
    };
    set({ messages: [...state.messages, userMsg] });

    // ── SSE streaming path ───────────────────────────────────────
    set({ isStreaming: true, streamingContent: "", streamingToolCalls: [] });

    const abort = api.streamMessage(
      content,
      state.activeConversationId,
      {
        onThinking() {
          // already set above; nothing extra needed
        },

        onDelta(text: string) {
          set((s) => ({ streamingContent: s.streamingContent + text }));
        },

        onToolEvent(eventType: string, payload: unknown) {
          if (eventType === "tool_use_start") {
            const p = payload as { data?: { id?: string; name?: string }; id?: string; name?: string };
            const id = p.data?.id ?? p.id ?? generateId();
            const name = p.data?.name ?? p.name ?? "tool";
            const toolCall: ToolCall = { id, name, arguments: {}, status: "running" };
            set((s) => ({ streamingToolCalls: [...s.streamingToolCalls, toolCall] }));
          } else if (eventType === "tool_result") {
            const p = payload as {
              data?: { id?: string; name?: string; success?: boolean; output?: string };
              id?: string; name?: string; success?: boolean; output?: string;
            };
            const id = p.data?.id ?? p.id;
            const success = p.data?.success ?? p.success ?? false;
            const output = p.data?.output ?? p.output ?? "";
            if (id) {
              set((s) => ({
                streamingToolCalls: s.streamingToolCalls.map((tc) =>
                  tc.id === id
                    ? { ...tc, status: success ? "success" as const : "error" as const, result: output }
                    : tc
                ),
              }));
            }
          }
        },

        onDone({ conversationId, messageId, content: fullContent }) {
          const s = get();
          const assistantMsg: Message = {
            id: messageId,
            role: "assistant",
            content: fullContent || s.streamingContent,
            timestamp: new Date().toISOString(),
            toolCalls: s.streamingToolCalls.length > 0 ? s.streamingToolCalls : undefined,
          };
          set({
            messages: [...s.messages, assistantMsg],
            isStreaming: false,
            streamingContent: "",
            streamingToolCalls: [],
            activeConversationId: conversationId,
            _sseAbort: null,
          });
          get().loadConversations();
        },

        onError(msg: string) {
          console.error("[SSE] stream error:", msg);
          set({ isStreaming: false, streamingContent: "", _sseAbort: null });
        },
      }
    );

    set({ _sseAbort: abort });
  },

  async loadConversations() {
    try {
      const conversations = await api.getConversations();
      set({ conversations });
    } catch {
      // API might not be ready yet — use empty list
      set({ conversations: [] });
    }
  },

  async loadConversation(id: string) {
    try {
      const convo = await api.getConversation(id);
      set({
        activeConversationId: id,
        messages: convo.messages,
      });
    } catch {
      console.error("Failed to load conversation:", id);
    }
  },

  newConversation() {
    set({
      activeConversationId: null,
      messages: [],
      streamingContent: "",
      streamingToolCalls: [],
      isStreaming: false,
    });
  },

  async deleteConversation(id: string) {
    try {
      await api.deleteConversation(id);
      const { conversations, activeConversationId } = get();
      set({
        conversations: conversations.filter((c) => c.id !== id),
        ...(activeConversationId === id ? { activeConversationId: null, messages: [] } : {}),
      });
    } catch {
      console.error("Failed to delete conversation:", id);
    }
  },

  setSidePanelContent(toolCall: ToolCall | null) {
    set({ sidePanelContent: toolCall, isSidePanelOpen: toolCall !== null });
  },

  toggleSidePanel() {
    set((state) => ({ isSidePanelOpen: !state.isSidePanelOpen }));
  },
}));
