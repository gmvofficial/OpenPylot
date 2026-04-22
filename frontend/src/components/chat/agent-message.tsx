"use client";

import { useState } from "react";
import { formatTime } from "@/lib/utils";
import type { Message } from "@/types";
import { MarkdownRenderer } from "./markdown-renderer";
import { ToolCallCard } from "./tool-call-card";
import { Bot, ThumbsUp, ThumbsDown, Copy, Check } from "lucide-react";
import { apiClient } from "@/lib/api";

interface AgentMessageProps {
  message: Message;
}

export function AgentMessage({ message }: AgentMessageProps) {
  const [feedback, setFeedback] = useState<"up" | "down" | null>(null);
  const [copied, setCopied] = useState(false);

  const handleFeedback = async (rating: "up" | "down") => {
    const newFeedback = feedback === rating ? null : rating;
    setFeedback(newFeedback);
    if (newFeedback) {
      try {
        await apiClient.submitFeedback({
          session_id: "current",
          turn_id: message.id,
          rating: newFeedback === "up" ? 1 : -1,
        });
      } catch {
        // non-critical
      }
    }
  };

  const handleCopy = () => {
    navigator.clipboard.writeText(message.content);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="flex gap-3 animate-fade-in group">
      {/* Avatar */}
      <div className="flex items-start pt-0.5 shrink-0">
        <div className="flex items-center justify-center w-7 h-7 rounded-lg bg-accent/10">
          <Bot className="w-4 h-4 text-accent" />
        </div>
      </div>

      {/* Content */}
      <div className="flex-1 min-w-0 max-w-[85%] md:max-w-[75%]">
        {/* Tool calls */}
        {message.toolCalls && message.toolCalls.length > 0 && (
          <div className="mb-1">
            {message.toolCalls.map((tc, i) => (
              <ToolCallCard key={tc.id || i} toolCall={tc} />
            ))}
          </div>
        )}

        {/* Text content */}
        {message.content && (
          <div className="bg-background-secondary rounded-2xl rounded-tl-md px-4 py-3">
            <MarkdownRenderer content={message.content} />
          </div>
        )}

        {/* Footer: timestamp + actions */}
        <div className="flex items-center gap-2 mt-1 px-1">
          <span className="text-[10px] text-foreground-muted">
            {formatTime(message.timestamp)}
          </span>
          <div className="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity">
            <button
              onClick={handleCopy}
              className="p-1 rounded hover:bg-background-secondary text-foreground-muted hover:text-foreground transition-colors"
              title="Copy"
            >
              {copied ? (
                <Check className="h-3 w-3 text-green-400" />
              ) : (
                <Copy className="h-3 w-3" />
              )}
            </button>
            <button
              onClick={() => handleFeedback("up")}
              className={`p-1 rounded hover:bg-background-secondary transition-colors ${
                feedback === "up"
                  ? "text-green-400"
                  : "text-foreground-muted hover:text-foreground"
              }`}
              title="Good response"
            >
              <ThumbsUp className="h-3 w-3" />
            </button>
            <button
              onClick={() => handleFeedback("down")}
              className={`p-1 rounded hover:bg-background-secondary transition-colors ${
                feedback === "down"
                  ? "text-red-400"
                  : "text-foreground-muted hover:text-foreground"
              }`}
              title="Bad response"
            >
              <ThumbsDown className="h-3 w-3" />
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
