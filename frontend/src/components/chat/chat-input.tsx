"use client";

import * as React from "react";
import { Send, Paperclip, Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { useChatStore } from "@/stores/chat";
import { cn } from "@/lib/utils";

export function ChatInput() {
  const [value, setValue] = React.useState("");
  const textareaRef = React.useRef<HTMLTextAreaElement>(null);
  const sendMessage = useChatStore((s) => s.sendMessage);
  const isStreaming = useChatStore((s) => s.isStreaming);
  const isConnected = useChatStore((s) => s.isConnected);

  // Auto-resize textarea
  React.useEffect(() => {
    const textarea = textareaRef.current;
    if (!textarea) return;
    textarea.style.height = "auto";
    textarea.style.height = Math.min(textarea.scrollHeight, 200) + "px";
  }, [value]);

  const handleSubmit = () => {
    const trimmed = value.trim();
    if (!trimmed || isStreaming) return;
    sendMessage(trimmed);
    setValue("");
    // Reset height
    if (textareaRef.current) {
      textareaRef.current.style.height = "auto";
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSubmit();
    }
  };

  // SSE streaming works without a WebSocket connection
  const canSend = value.trim().length > 0 && !isStreaming;

  return (
    <div className="border-t border-border bg-background px-4 py-4">
      <div className="max-w-chat mx-auto">
        <div className="flex items-end gap-2 bg-background-input border border-border rounded-2xl px-4 py-3 focus-within:ring-2 focus-within:ring-accent/30 focus-within:border-accent/30 transition-colors">
          {/* Attach */}
          <Button
            variant="ghost"
            size="icon-sm"
            className="shrink-0 mb-0.5 text-foreground-muted hover:text-foreground"
            title="Attach file"
          >
            <Paperclip className="w-4 h-4" />
          </Button>

          {/* Textarea */}
          <textarea
            ref={textareaRef}
            value={value}
            onChange={(e) => setValue(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Ask anything..."
            disabled={isStreaming}
            rows={2}
            className={cn(
              "flex-1 bg-transparent text-[15px] text-foreground placeholder:text-foreground-muted placeholder:text-center resize-none outline-none py-2 max-h-[200px] min-h-[52px]",
              "disabled:opacity-50"
            )}
          />

          {/* Send */}
          <Button
            variant={canSend ? "default" : "ghost"}
            size="icon-sm"
            onClick={handleSubmit}
            disabled={!canSend}
            className="shrink-0 mb-0.5"
          >
            {isStreaming ? (
              <Loader2 className="w-4 h-4 animate-spin" />
            ) : (
              <Send className="w-4 h-4" />
            )}
          </Button>
        </div>

        {/* Status bar */}
        <div className="flex items-center justify-between mt-1.5 px-1">
          <div className="flex items-center gap-2 text-[10px] text-foreground-muted">
            <span className="flex items-center gap-1">
              <span className={isConnected ? "text-accent-success" : "text-accent-error"}>●</span>
              {isConnected ? "Connected" : "Disconnected"}
            </span>
          </div>
          <div className="text-[10px] text-foreground-muted">
            {isStreaming && "Agent is typing..."}
          </div>
        </div>
      </div>
    </div>
  );
}
