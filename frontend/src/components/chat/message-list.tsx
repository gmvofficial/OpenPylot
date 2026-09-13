"use client";

import * as React from "react";
import { useChatStore } from "@/stores/chat";
import { UserMessage } from "./user-message";
import { AgentMessage } from "./agent-message";
import { StreamingMessage } from "./streaming-message";
import { ArrowDown, ChevronUp } from "lucide-react";
import { Button } from "@/components/ui/button";

/**
 * How many messages render at once.
 *
 * Chat messages are variable-height markdown with syntax highlighting and
 * KaTeX, so index-based virtualisation would need height estimates it cannot
 * get right. Windowing the tail — the part anyone is actually reading — plus
 * `content-visibility` on each row gets the same result without the guessing.
 */
const WINDOW_SIZE = 60;

/** How many more to reveal each time "show earlier" is used. */
const WINDOW_STEP = 60;

/** Distance from the bottom, in px, still counted as "at the bottom". */
const AT_BOTTOM_THRESHOLD = 120;

export function MessageList() {
  const messages = useChatStore((s) => s.messages);
  const isStreaming = useChatStore((s) => s.isStreaming);
  const scrollContainerRef = React.useRef<HTMLDivElement>(null);
  const bottomRef = React.useRef<HTMLDivElement>(null);

  const [visibleCount, setVisibleCount] = React.useState(WINDOW_SIZE);
  const [atBottom, setAtBottom] = React.useState(true);

  // Auto-scrolling is only correct while the reader is at the bottom. Following
  // every token while they have scrolled up to re-read something yanks the view
  // out from under them — so this tracks position rather than assuming it.
  const atBottomRef = React.useRef(true);

  const handleScroll = React.useCallback(() => {
    const el = scrollContainerRef.current;
    if (!el) return;
    const distance = el.scrollHeight - el.scrollTop - el.clientHeight;
    const isAtBottom = distance < AT_BOTTOM_THRESHOLD;
    atBottomRef.current = isAtBottom;
    setAtBottom(isAtBottom);
  }, []);

  React.useEffect(() => {
    if (!atBottomRef.current) return;
    // `auto` during streaming: a smooth scroll per token never finishes before
    // the next one starts, which leaves the view permanently lagging.
    bottomRef.current?.scrollIntoView({ behavior: isStreaming ? "auto" : "smooth" });
  }, [messages, isStreaming]);

  // A new conversation starts at the tail again.
  const conversationId = useChatStore((s) => s.activeConversationId);
  React.useEffect(() => {
    setVisibleCount(WINDOW_SIZE);
    atBottomRef.current = true;
    setAtBottom(true);
  }, [conversationId]);

  const scrollToBottom = () => {
    atBottomRef.current = true;
    setAtBottom(true);
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  };

  const showEarlier = () => {
    const el = scrollContainerRef.current;
    const before = el?.scrollHeight ?? 0;
    setVisibleCount((n) => n + WINDOW_STEP);

    // Revealing messages above the viewport would otherwise jump the reader's
    // position; restoring the offset keeps what they were reading in place.
    requestAnimationFrame(() => {
      if (!el) return;
      el.scrollTop += el.scrollHeight - before;
    });
  };

  const hidden = Math.max(0, messages.length - visibleCount);
  const visible = hidden > 0 ? messages.slice(hidden) : messages;
  const isEmpty = messages.length === 0 && !isStreaming;

  return (
    <div className="relative min-h-0 flex-1 overflow-hidden">
      <div
        ref={scrollContainerRef}
        onScroll={handleScroll}
        className="h-full overflow-y-auto scrollbar-thin px-4 py-6 pr-6"
      >
        <div className="mx-auto max-w-chat space-y-6">
          {isEmpty && <EmptyState />}

          {hidden > 0 && (
            <div className="flex justify-center">
              <Button variant="ghost" size="sm" onClick={showEarlier}>
                <ChevronUp className="mr-1.5 h-3.5 w-3.5" />
                Show {Math.min(hidden, WINDOW_STEP)} earlier message
                {Math.min(hidden, WINDOW_STEP) === 1 ? "" : "s"}
              </Button>
            </div>
          )}

          {visible.map((msg) => (
            // `content-visibility: auto` lets the browser skip layout and paint
            // for rows scrolled out of view; `contain-intrinsic-size` gives it a
            // placeholder height so the scrollbar does not jitter as they are
            // skipped and restored.
            <div
              key={msg.id}
              style={{ contentVisibility: "auto", containIntrinsicSize: "auto 120px" }}
            >
              {msg.role === "user" ? (
                <UserMessage message={msg} />
              ) : (
                <AgentMessage message={msg} />
              )}
            </div>
          ))}

          <StreamingMessage />
          <div ref={bottomRef} />
        </div>
      </div>

      {!atBottom && (
        <div className="absolute bottom-4 left-1/2 -translate-x-1/2">
          <Button
            variant="secondary"
            size="sm"
            onClick={scrollToBottom}
            className="rounded-full shadow-lg"
          >
            <ArrowDown className="mr-1 h-3.5 w-3.5" />
            {isStreaming ? "Jump to latest" : "Scroll to bottom"}
          </Button>
        </div>
      )}
    </div>
  );
}

function EmptyState() {
  return (
    <div className="flex min-h-[calc(100vh-200px)] flex-col items-center justify-center text-center">
      <div className="mb-6 flex h-16 w-16 items-center justify-center rounded-2xl bg-accent/10">
        <span className="text-3xl">🤖</span>
      </div>
      <h3 className="mb-2 text-xl font-semibold text-foreground">Ask anything</h3>
      <p className="mb-8 max-w-md text-sm text-foreground-secondary">
        I can manage your calendar, send emails, take notes, set reminders, search the web, and
        more.
      </p>
      <div className="grid w-full max-w-lg grid-cols-1 gap-3 sm:grid-cols-2">
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
      className="flex items-center gap-3 rounded-xl border border-border bg-background-secondary p-3 text-left text-sm text-foreground-secondary transition-colors hover:border-border-hover hover:text-foreground"
    >
      <span className="text-base">{icon}</span>
      <span>{label}</span>
    </button>
  );
}
