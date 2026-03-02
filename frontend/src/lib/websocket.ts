import { getWsBaseUrl } from "./utils";
import type { WSServerMessage, WSNotificationMessage } from "@/types";

type MessageHandler = (msg: WSServerMessage) => void;
type NotificationHandler = (msg: WSNotificationMessage) => void;

/** Manages a WebSocket connection with auto-reconnect. */
export class WebSocketClient {
  private ws: WebSocket | null = null;
  private url: string;
  private reconnectAttempts = 0;
  private maxReconnectAttempts = 10;
  private reconnectDelay = 1000;
  private handlers: Set<MessageHandler | NotificationHandler> = new Set();
  private openHandlers: Set<() => void> = new Set();
  private closeHandlers: Set<() => void> = new Set();
  private shouldReconnect = true;

  constructor(path: string) {
    this.url = `${getWsBaseUrl()}${path}`;
  }

  connect(): void {
    if (this.ws?.readyState === WebSocket.OPEN) return;

    try {
      this.ws = new WebSocket(this.url);

      this.ws.onopen = () => {
        this.reconnectAttempts = 0;
        this.openHandlers.forEach((h) => h());
      };

      this.ws.onmessage = (event) => {
        try {
          const data = JSON.parse(event.data);
          this.handlers.forEach((h) => h(data));
        } catch {
          console.error("[WS] Failed to parse message:", event.data);
        }
      };

      this.ws.onclose = () => {
        this.closeHandlers.forEach((h) => h());
        if (this.shouldReconnect && this.reconnectAttempts < this.maxReconnectAttempts) {
          const delay = this.reconnectDelay * Math.pow(2, this.reconnectAttempts);
          this.reconnectAttempts++;
          setTimeout(() => this.connect(), delay);
        }
      };

      this.ws.onerror = () => {
        // onclose will fire after onerror
      };
    } catch {
      // Will retry via reconnect logic
    }
  }

  disconnect(): void {
    this.shouldReconnect = false;
    this.ws?.close();
    this.ws = null;
  }

  send(data: unknown): void {
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(data));
    }
  }

  onMessage(handler: MessageHandler | NotificationHandler): () => void {
    this.handlers.add(handler);
    return () => this.handlers.delete(handler);
  }

  onOpen(handler: () => void): () => void {
    this.openHandlers.add(handler);
    return () => this.openHandlers.delete(handler);
  }

  onClose(handler: () => void): () => void {
    this.closeHandlers.add(handler);
    return () => this.closeHandlers.delete(handler);
  }

  get connected(): boolean {
    return this.ws?.readyState === WebSocket.OPEN;
  }
}
