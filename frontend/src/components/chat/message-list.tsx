"use client";

import * as React from "react";
import { useChatStore } from "@/stores/chat";
import { UserMessage } from "./user-message";
import { AgentMessage } from "./agent-message";
import { StreamingMessage } from "./streaming-message";
import { ArrowDown } from "lucide-react";
import { Button } from "@/components/ui/button";

export function MessageList() {
  const messages = useChatStore((s) => s.messages);
  const isStreaming = useChatStore((s) => s.isStreaming);
  const scrollContainerRef = React.useRef<HTMLDivElement>(null);
  const bottomRef = React.useRef<HTMLDivElement>(null);
  const [showScrollButton, setShowScrollButton] = React.useState(false);

  // Auto-scroll to bottom on new messages
  React.useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, isStreaming]);

  // Show "scroll to bottom" button when user scrolls up
  const handleScroll = React.useCallback(() => {
    const el = scrollContainerRef.current;
    if (!el) return;
    const { scrollHeight, scrollTop, clientHeight } = el;
    const isNearBottom = scrollHeight - scrollTop - clientHeight < 100;
    setShowScrollButton(!isNearBottom);
  }, []);

  const scrollToBottom = () => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  };

  const isEmpty = messages.length === 0 && !isStreaming;

  return (
    <div className="relative flex-1 min-h-0 overflow-hidden">
      <div
        ref={scrollContainerRef}
        onScroll={handleScroll}
        className="h-full overflow-y-auto scrollbar-thin px-4 pr-6 py-6"
      >
        <div className="max-w-chat mx-auto space-y-6">
          {isEmpty && <EmptyState />}
          {messages.map((msg) =>
            msg.role === "user" ? (
              <UserMessage key={msg.id} message={msg} />
            ) : (
              <AgentMessage key={msg.id} message={msg} />
            )
          )}
          <StreamingMessage />
          <div ref={bottomRef} />
        </div>
      </div>

      {/* Scroll to bottom button */}
      {showScrollButton && (
        <div className="absolute bottom-4 left-1/2 -translate-x-1/2">
          <Button
            variant="secondary"
            size="sm"
            onClick={scrollToBottom}
            className="rounded-full shadow-lg"
          >
            <ArrowDown className="w-3.5 h-3.5 mr-1" />
            New messages
          </Button>
        </div>
      )}
    </div>
  );
}

function EmptyState() {
  return (
    <div className="flex flex-col items-center justify-center min-h-[calc(100vh-200px)] text-center">
      <div className="flex items-center justify-center w-16 h-16 rounded-2xl bg-accent/10 mb-6">
        <span className="text-3xl">🤖</span>
      </div>
      <h3 className="text-xl font-semibold text-foreground mb-2">
        Ask anything
      </h3>
      <p className="text-sm text-foreground-secondary max-w-md mb-8">
        I can manage your calendar, send emails, take notes, set reminders,
        search the web, and more.
      </p>
      <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 max-w-lg w-full">
        {[
          { icon: "📅", label: "What meetings do I have today?" },
          { icon: "📧", label: "Check my unread emails" },
          { icon: "📝", label: "Create a note about..." },
          { icon: "⏰", label: "Remind me to..." },
        ].map((suggestion) => (
          <SuggestionCard key={suggestion.label} {...suggestion} />
        ))}
      </div>
    </div>
  );
}

function SuggestionCard({ icon, label }: { icon: string; label: string }) {
  const sendMessage = useChatStore((s) => s.sendMessage);

  return (
    <button
      onClick={() => sendMessage(label)}
      className="flex items-center gap-3 rounded-xl border border-border bg-background-secondary p-3 text-left text-sm text-foreground-secondary hover:text-foreground hover:border-border-hover transition-colors"
    >
      <span className="text-base">{icon}</span>
      <span>{label}</span>
    </button>
  );
}
