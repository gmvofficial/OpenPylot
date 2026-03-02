"use client";

import * as React from "react";
import { X, Calendar, FileText, Bell, Code, ExternalLink } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Badge } from "@/components/ui/badge";
import { useChatStore } from "@/stores/chat";
import type { ToolCall } from "@/types";
import { cn } from "@/lib/utils";

export function SidePanel() {
  const { isSidePanelOpen, sidePanelContent, setSidePanelContent, toggleSidePanel } = useChatStore();

  if (!isSidePanelOpen) return null;

  return (
    <div className="w-side-panel border-l border-border bg-background-secondary h-full flex flex-col animate-slide-in-right">
      {/* Header */}
      <div className="flex items-center justify-between px-4 h-12 border-b border-border shrink-0">
        <h3 className="text-sm font-medium text-foreground">Tool Result</h3>
        <Button
          variant="ghost"
          size="icon-sm"
          onClick={() => setSidePanelContent(null)}
        >
          <X className="w-4 h-4" />
        </Button>
      </div>

      {/* Content */}
      <ScrollArea className="flex-1">
        {sidePanelContent ? (
          <ToolResultView toolCall={sidePanelContent} />
        ) : (
          <div className="flex items-center justify-center h-64 text-sm text-foreground-muted">
            Select a tool result to view details
          </div>
        )}
      </ScrollArea>
    </div>
  );
}

function ToolResultView({ toolCall }: { toolCall: ToolCall }) {
  const toolName = toolCall.name;
  const result = toolCall.result;

  // Determine icon based on tool name
  const icon = toolName.includes("calendar") || toolName.includes("meeting")
    ? Calendar
    : toolName.includes("note")
      ? FileText
      : toolName.includes("reminder")
        ? Bell
        : Code;

  const Icon = icon;

  const displayName = toolName
    .replace(/_/g, " ")
    .replace(/\b\w/g, (c) => c.toUpperCase());

  // Try to parse result as JSON for structured display
  let parsedResult: Record<string, unknown> | null = null;
  try {
    if (result) {
      parsedResult = JSON.parse(result);
    }
  } catch {
    // Not JSON, display as text
  }

  return (
    <div className="p-4 space-y-4">
      {/* Tool header */}
      <div className="flex items-center gap-3">
        <div className="flex items-center justify-center w-10 h-10 rounded-lg bg-accent/10">
          <Icon className="w-5 h-5 text-accent" />
        </div>
        <div>
          <h4 className="text-sm font-medium text-foreground">{displayName}</h4>
          <Badge variant="success" className="mt-1">
            Completed
            {toolCall.durationMs && ` · ${(toolCall.durationMs / 1000).toFixed(1)}s`}
          </Badge>
        </div>
      </div>

      {/* Arguments */}
      <div>
        <h5 className="text-xs font-medium text-foreground-muted uppercase tracking-wider mb-2">
          Input
        </h5>
        <div className="bg-background rounded-lg p-3 space-y-2">
          {Object.entries(toolCall.arguments).map(([key, value]) => (
            <div key={key} className="flex gap-2 text-xs">
              <span className="text-foreground-muted font-mono shrink-0">{key}:</span>
              <span className="text-foreground-secondary break-all">
                {typeof value === "object" ? JSON.stringify(value) : String(value)}
              </span>
            </div>
          ))}
        </div>
      </div>

      {/* Result */}
      {result && (
        <div>
          <h5 className="text-xs font-medium text-foreground-muted uppercase tracking-wider mb-2">
            Result
          </h5>
          <div className="bg-background rounded-lg p-3">
            {parsedResult ? (
              <div className="space-y-2">
                {Object.entries(parsedResult).map(([key, value]) => (
                  <div key={key} className="flex gap-2 text-xs">
                    <span className="text-foreground-muted font-mono shrink-0">{key}:</span>
                    <span className="text-foreground-secondary break-all">
                      {typeof value === "object" ? JSON.stringify(value, null, 2) : String(value)}
                    </span>
                  </div>
                ))}
              </div>
            ) : (
              <p className="text-xs text-foreground-secondary whitespace-pre-wrap">
                {result}
              </p>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
