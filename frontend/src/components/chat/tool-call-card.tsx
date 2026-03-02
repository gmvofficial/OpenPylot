"use client";

import * as React from "react";
import { cn } from "@/lib/utils";
import { ChevronDown, ChevronRight, CheckCircle2, XCircle, Loader2, Clock } from "lucide-react";
import type { ToolCall } from "@/types";
import { useChatStore } from "@/stores/chat";

interface ToolCallCardProps {
  toolCall: ToolCall;
}

const toolIcons: Record<string, string> = {
  create_calendar_event: "📅",
  list_calendar_events: "📅",
  create_meeting: "🤝",
  gmail_search: "📧",
  gmail_send: "✉️",
  gmail_reply: "↩️",
  gmail_draft_create: "📝",
  gmail_draft_send: "📤",
  gmail_get: "📬",
  create_note: "📝",
  list_notes: "📋",
  search_notes: "🔍",
  delete_note: "🗑️",
  set_reminder: "⏰",
  list_reminders: "📋",
  complete_reminder: "✅",
  send_telegram_message: "✈️",
  get_telegram_updates: "✈️",
  send_whatsapp_message: "💬",
};

export function ToolCallCard({ toolCall }: ToolCallCardProps) {
  const [expanded, setExpanded] = React.useState(false);
  const setSidePanelContent = useChatStore((s) => s.setSidePanelContent);

  const icon = toolIcons[toolCall.name] || "🔧";

  const statusConfig = {
    pending: {
      icon: <Clock className="w-3.5 h-3.5 text-foreground-muted" />,
      label: "Pending",
      color: "text-foreground-muted",
    },
    running: {
      icon: <Loader2 className="w-3.5 h-3.5 text-accent animate-spin" />,
      label: "Running",
      color: "text-accent",
    },
    success: {
      icon: <CheckCircle2 className="w-3.5 h-3.5 text-accent-success" />,
      label: toolCall.durationMs ? `${(toolCall.durationMs / 1000).toFixed(1)}s` : "Done",
      color: "text-accent-success",
    },
    error: {
      icon: <XCircle className="w-3.5 h-3.5 text-accent-error" />,
      label: "Failed",
      color: "text-accent-error",
    },
  };

  const status = statusConfig[toolCall.status];
  const toolDisplayName = toolCall.name.replace(/_/g, " ");

  return (
    <div className="my-2 rounded-lg border border-border bg-background overflow-hidden">
      {/* Header */}
      <button
        onClick={() => setExpanded(!expanded)}
        className="flex items-center gap-2 w-full px-3 py-2 text-left hover:bg-background-tertiary transition-colors"
      >
        {expanded ? (
          <ChevronDown className="w-3.5 h-3.5 text-foreground-muted shrink-0" />
        ) : (
          <ChevronRight className="w-3.5 h-3.5 text-foreground-muted shrink-0" />
        )}
        <span className="text-sm">{icon}</span>
        <span className="text-xs font-medium text-foreground-secondary capitalize flex-1 min-w-0 truncate">
          {toolDisplayName}
        </span>
        <span className="flex items-center gap-1.5 shrink-0">
          {status.icon}
          <span className={cn("text-xs", status.color)}>{status.label}</span>
        </span>
      </button>

      {/* Expandable body */}
      {expanded && (
        <div className="px-3 pb-3 border-t border-border/50">
          {/* Arguments */}
          {Object.keys(toolCall.arguments).length > 0 && (
            <div className="mt-2">
              <p className="text-[10px] font-medium text-foreground-muted uppercase tracking-wider mb-1">
                Arguments
              </p>
              <pre className="text-xs text-foreground-secondary bg-background-tertiary rounded-md p-2 overflow-x-auto font-mono">
                {JSON.stringify(toolCall.arguments, null, 2)}
              </pre>
            </div>
          )}

          {/* Result */}
          {toolCall.result && (
            <div className="mt-2">
              <p className="text-[10px] font-medium text-foreground-muted uppercase tracking-wider mb-1">
                Result
              </p>
              <pre className="text-xs text-foreground-secondary bg-background-tertiary rounded-md p-2 overflow-x-auto font-mono whitespace-pre-wrap">
                {toolCall.result}
              </pre>
            </div>
          )}

          {/* View in side panel */}
          {toolCall.status === "success" && toolCall.result && (
            <button
              onClick={() => setSidePanelContent(toolCall)}
              className="mt-2 text-xs text-accent hover:underline"
            >
              View in panel →
            </button>
          )}
        </div>
      )}
    </div>
  );
}
