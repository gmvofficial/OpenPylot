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

  // WebSocket
  wsClient: WebSocketClient | null;

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

  connect() {
    const existing = get().wsClient;
    if (existing?.connected) return;

    // Clean up any previous client that is no longer connected
    if (existing) {
      existing.disconnect();
    }

    const client = new WebSocketClient("/ws/chat");

    client.onOpen(() => {
      set({ isConnected: true });
    });

    client.onClose(() => {
      set({ isConnected: false });
    });

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
          // Refresh conversation list in sidebar
          get().loadConversations();
          break;
        }

        case "error":
          set({ isStreaming: false, streamingContent: "" });
          console.error("[Chat WS] Error:", msg.message);
          break;
      }
    });

    client.connect();
    set({ wsClient: client });
  },

  disconnect() {
    get().wsClient?.disconnect();
    set({ wsClient: null, isConnected: false });
  },

  sendMessage(content: string) {
    const { wsClient, messages, activeConversationId } = get();
    if (!wsClient?.connected) return;

    const userMsg: Message = {
      id: generateId(),
      role: "user",
      content,
      timestamp: new Date().toISOString(),
    };

    set({ messages: [...messages, userMsg] });

    wsClient.send({
      type: "message",
      content,
      conversationId: activeConversationId,
    });
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
