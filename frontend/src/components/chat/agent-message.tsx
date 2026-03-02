"use client";

import { formatTime } from "@/lib/utils";
import type { Message } from "@/types";
import { MarkdownRenderer } from "./markdown-renderer";
import { ToolCallCard } from "./tool-call-card";
import { Bot } from "lucide-react";

interface AgentMessageProps {
  message: Message;
}

export function AgentMessage({ message }: AgentMessageProps) {
  return (
    <div className="flex gap-3 animate-fade-in">
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

        <div className="flex mt-1 px-1">
          <span className="text-[10px] text-foreground-muted">
            {formatTime(message.timestamp)}
          </span>
        </div>
      </div>
    </div>
  );
}
