"use client";

import { MarkdownRenderer } from "./markdown-renderer";
import { ToolCallCard } from "./tool-call-card";
import { Bot } from "lucide-react";
import { useChatStore } from "@/stores/chat";

/** Renders the in-progress streaming message from the agent */
export function StreamingMessage() {
  const isStreaming = useChatStore((s) => s.isStreaming);
  const streamingContent = useChatStore((s) => s.streamingContent);
  const streamingToolCalls = useChatStore((s) => s.streamingToolCalls);

  if (!isStreaming) return null;

  const hasContent = streamingContent.length > 0;
  const hasToolCalls = streamingToolCalls.length > 0;
  const isThinking = !hasContent && !hasToolCalls;

  return (
    <div className="flex gap-3 animate-fade-in">
      {/* Avatar */}
      <div className="flex items-start pt-0.5 shrink-0">
        <div className="flex items-center justify-center w-7 h-7 rounded-lg bg-accent/10">
          <Bot className="w-4 h-4 text-accent" />
        </div>
      </div>

      <div className="flex-1 min-w-0 max-w-[85%] md:max-w-[75%]">
        {/* Tool calls in progress */}
        {hasToolCalls && (
          <div className="mb-1">
            {streamingToolCalls.map((tc, i) => (
              <ToolCallCard key={tc.id || i} toolCall={tc} />
            ))}
          </div>
        )}

        {/* Streaming text or thinking indicator */}
        <div className="bg-background-secondary rounded-2xl rounded-tl-md px-4 py-3">
          {hasContent ? (
            <>
              <MarkdownRenderer content={streamingContent} />
              {/* Blinking cursor */}
              <span className="inline-block w-2 h-4 bg-foreground-muted animate-pulse ml-0.5 align-middle" />
            </>
          ) : isThinking ? (
            <div className="thinking-dots flex items-center gap-1 py-1">
              <span />
              <span />
              <span />
            </div>
          ) : null}
        </div>
      </div>
    </div>
  );
}
