import { create } from "zustand";
import type { Notification, WSNotificationMessage } from "@/types";
import { WebSocketClient } from "@/lib/websocket";
import { generateId } from "@/lib/utils";
import { useChatStore } from "@/stores/chat";

interface NotificationState {
  notifications: Notification[];
  unreadCount: number;
  wsClient: WebSocketClient | null;

  connect: () => void;
  disconnect: () => void;
  markRead: (id: string) => void;
  markAllRead: () => void;
  dismiss: (id: string) => void;
  clearAll: () => void;
}

function toNotification(msg: WSNotificationMessage): Notification {
  switch (msg.type) {
    case "rsvp_update":
      return {
        id: generateId(),
        type: "rsvp_update",
        title: "RSVP Update",
        message: `${msg.attendee} ${msg.status}`,
        timestamp: new Date().toISOString(),
        read: false,
        data: msg as unknown as Record<string, unknown>,
      };
    case "reminder_due":
      return {
        id: generateId(),
        type: "reminder_due",
        title: msg.title || "Reminder",
        message: msg.message || msg.title,
        timestamp: new Date().toISOString(),
        read: false,
      };
    case "integration_status":
      return {
        id: generateId(),
        type: "integration_status",
        title: `${msg.service} — ${msg.status}`,
        message: msg.message,
        timestamp: new Date().toISOString(),
        read: false,
      };
    case "job_completed":
      return {
        id: generateId(),
        type: "job_completed",
        title: "Job Completed",
        message: `${msg.job}: ${msg.result}`,
        timestamp: new Date().toISOString(),
        read: false,
      };
    case "integration_connected":
      return {
        id: generateId(),
        type: "integration_connected",
        title: "Integration Connected",
        message: `${msg.service} has been connected`,
        timestamp: new Date().toISOString(),
        read: false,
      };
  }
}

export const useNotificationStore = create<NotificationState>((set, get) => ({
  notifications: [],
  unreadCount: 0,
  wsClient: null,

  connect() {
    const existing = get().wsClient;
    if (existing?.connected) return;

    const client = new WebSocketClient("/ws/notifications");

    client.onMessage((msg: WSNotificationMessage | { type: string }) => {
      // Ignore welcome/pong control messages from the server
      if (msg.type === "connected" || msg.type === "pong") return;

      const notification = toNotification(msg as WSNotificationMessage);
      if (!notification) return; // guard against unknown message types
      set((state) => ({
        notifications: [notification, ...state.notifications].slice(0, 100),
        unreadCount: state.unreadCount + 1,
      }));

      // If it's a reminder with an agent response, inject it into the chat
      if (msg.type === "reminder_due") {
        const reminderMsg = msg as WSNotificationMessage & { message?: string; conversationId?: string };
        if (reminderMsg.conversationId && reminderMsg.message) {
          const chatStore = useChatStore.getState();
          // Add the assistant response as a chat message
          const assistantMsg = {
            id: generateId(),
            role: "assistant" as const,
            content: reminderMsg.message,
            timestamp: new Date().toISOString(),
          };
          chatStore.loadConversations();
          // If no active conversation, switch to the reminder conversation
          if (!chatStore.activeConversationId) {
            useChatStore.setState({
              activeConversationId: reminderMsg.conversationId,
              messages: [assistantMsg],
            });
          } else {
            // Append to current chat so user sees it immediately
            useChatStore.setState({
              messages: [...chatStore.messages, assistantMsg],
            });
          }
        }
      }
    });

    client.connect();
    set({ wsClient: client });
  },

  disconnect() {
    get().wsClient?.disconnect();
    set({ wsClient: null });
  },

  markRead(id: string) {
    set((state) => ({
      notifications: state.notifications.map((n) =>
        n.id === id ? { ...n, read: true } : n
      ),
      unreadCount: Math.max(0, state.unreadCount - 1),
    }));
  },

  markAllRead() {
    set((state) => ({
      notifications: state.notifications.map((n) => ({ ...n, read: true })),
      unreadCount: 0,
    }));
  },

  dismiss(id: string) {
    set((state) => {
      const notification = state.notifications.find((n) => n.id === id);
      return {
        notifications: state.notifications.filter((n) => n.id !== id),
        unreadCount: notification && !notification.read
          ? Math.max(0, state.unreadCount - 1)
          : state.unreadCount,
      };
    });
  },

  clearAll() {
    set({ notifications: [], unreadCount: 0 });
  },
}));
