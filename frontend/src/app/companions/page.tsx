"use client";

import * as React from "react";
import { Database, ExternalLink, Loader2, Play, Square, AlertCircle, Download } from "lucide-react";
import { Button } from "@/components/ui/button";
import { apiClient } from "@/lib/api";
import { getToken } from "@/lib/token";
import { useToastStore } from "@/stores/toast";
import type { Companion } from "@/types";

/**
 * Companion apps — sibling Pylot tools hosted inside OpenPylot.
 *
 * A companion's *tools* arrive over MCP and show up in chat without any UI.
 * What this page is for is a companion's own web interface: OpenDbPylot ships a
 * full SQL workbench that is an app, not a tool call. The backend runs it on a
 * loopback port and proxies it behind OpenPylot's access token, so it renders
 * here instead of being a second program on a second, unprotected port.
 */
export default function CompanionsPage() {
  const [companions, setCompanions] = React.useState<Companion[] | null>(null);
  const [busy, setBusy] = React.useState<string | null>(null);
  const [active, setActive] = React.useState<string | null>(null);
  const addToast = useToastStore((s) => s.addToast);

  const refresh = React.useCallback(async () => {
    try {
      const list = await apiClient.getCompanions();
      setCompanions(list);
      // Open the first running companion so the page is useful on arrival
      // rather than showing an empty frame.
      setActive((current) => current ?? list.find((c) => c.state === "running")?.name ?? null);
    } catch {
      setCompanions([]);
    }
  }, []);

  React.useEffect(() => {
    refresh();
  }, [refresh]);

  const start = async (name: string) => {
    setBusy(name);
    try {
      await apiClient.startCompanion(name);
      await refresh();
      setActive(name);
    } catch (e) {
      addToast({
        title: "Could not start",
        description: e instanceof Error ? e.message : String(e),
        variant: "error",
      });
      await refresh();
    } finally {
      setBusy(null);
    }
  };

  const stop = async (name: string) => {
    setBusy(name);
    try {
      await apiClient.stopCompanion(name);
      if (active === name) setActive(null);
      await refresh();
    } finally {
      setBusy(null);
    }
  };

  if (companions === null) {
    return (
      <div className="flex h-full items-center justify-center">
        <Loader2 className="h-5 w-5 animate-spin text-foreground-muted" />
      </div>
    );
  }

  const running = companions.find((c) => c.name === active && c.state === "running");

  return (
    <div className="flex h-full flex-col">
      <header className="shrink-0 border-b border-border px-6 py-4">
        <h1 className="text-base font-semibold text-foreground">Companions</h1>
        <p className="mt-1 text-sm text-foreground-secondary">
          Other Pylot apps, running inside this one. Their tools are already available in chat;
          this is where their own interfaces appear.
        </p>
      </header>

      <div className="flex min-h-0 flex-1">
        <aside className="w-72 shrink-0 space-y-2 overflow-y-auto border-r border-border p-4">
          {companions.map((companion) => (
            <CompanionCard
              key={companion.name}
              companion={companion}
              busy={busy === companion.name}
              active={active === companion.name}
              onOpen={() => setActive(companion.name)}
              onStart={() => start(companion.name)}
              onStop={() => stop(companion.name)}
            />
          ))}
        </aside>

        <main className="min-w-0 flex-1 bg-background-secondary">
          {running ? (
            <iframe
              key={running.name}
              // The token rides in the query string: an iframe cannot send a
              // header, and the proxy accepts either.
              src={`${running.url}?token=${encodeURIComponent(getToken() ?? "")}`}
              title={running.title}
              className="h-full w-full border-0"
              sandbox="allow-scripts allow-same-origin allow-forms allow-popups allow-downloads"
            />
          ) : (
            <EmptyFrame hasAny={companions.some((c) => c.state !== "not_installed")} />
          )}
        </main>
      </div>
    </div>
  );
}

function CompanionCard({
  companion,
  busy,
  active,
  onOpen,
  onStart,
  onStop,
}: {
  companion: Companion;
  busy: boolean;
  active: boolean;
  onOpen: () => void;
  onStart: () => void;
  onStop: () => void;
}) {
  const installed = companion.state !== "not_installed";
  const running = companion.state === "running";

  return (
    <div
      className={[
        "rounded-lg border p-3 transition-colors",
        active && running
          ? "border-accent/40 bg-accent/5"
          : "border-border bg-background-tertiary",
      ].join(" ")}
    >
      <div className="flex items-start gap-2">
        <Database className="mt-0.5 h-4 w-4 shrink-0 text-accent" />
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="truncate text-sm font-medium text-foreground">{companion.title}</span>
            <StateBadge state={companion.state} />
          </div>
          <p className="mt-1 text-xs leading-relaxed text-foreground-muted">
            {companion.description}
          </p>
        </div>
      </div>

      {companion.state === "failed" && companion.error && (
        <p className="mt-2 flex gap-1.5 rounded bg-accent-error/10 px-2 py-1.5 text-xs text-accent-error">
          <AlertCircle className="mt-0.5 h-3 w-3 shrink-0" />
          <span>{companion.error}</span>
        </p>
      )}

      <div className="mt-3 flex gap-2">
        {!installed ? (
          <p className="flex items-center gap-1.5 text-xs text-foreground-muted">
            <Download className="h-3 w-3" />
            <code className="font-mono">cargo install open{companion.name}</code>
          </p>
        ) : running ? (
          <>
            {!active && (
              <Button size="sm" variant="default" onClick={onOpen}>
                <ExternalLink className="mr-1.5 h-3 w-3" />
                Open
              </Button>
            )}
            <Button size="sm" variant="ghost" onClick={onStop} disabled={busy}>
              {busy ? (
                <Loader2 className="h-3 w-3 animate-spin" />
              ) : (
                <>
                  <Square className="mr-1.5 h-3 w-3" />
                  Stop
                </>
              )}
            </Button>
          </>
        ) : (
          <Button size="sm" variant="default" onClick={onStart} disabled={busy}>
            {busy ? (
              <>
                <Loader2 className="mr-1.5 h-3 w-3 animate-spin" />
                Starting…
              </>
            ) : (
              <>
                <Play className="mr-1.5 h-3 w-3" />
                Start
              </>
            )}
          </Button>
        )}
      </div>
    </div>
  );
}

function StateBadge({ state }: { state: Companion["state"] }) {
  const look: Record<Companion["state"], [string, string]> = {
    running: ["Running", "text-accent-success bg-accent-success/10"],
    starting: ["Starting", "text-accent-warning bg-accent-warning/10"],
    stopped: ["Stopped", "text-foreground-muted bg-background-secondary"],
    not_installed: ["Not installed", "text-foreground-muted bg-background-secondary"],
    failed: ["Failed", "text-accent-error bg-accent-error/10"],
  };
  const [label, classes] = look[state];
  return (
    <span className={`shrink-0 rounded px-1.5 py-0.5 text-[10px] font-medium ${classes}`}>
      {label}
    </span>
  );
}

function EmptyFrame({ hasAny }: { hasAny: boolean }) {
  return (
    <div className="flex h-full items-center justify-center p-8">
      <div className="max-w-sm text-center">
        <Database className="mx-auto h-8 w-8 text-foreground-muted" />
        <p className="mt-3 text-sm text-foreground-secondary">
          {hasAny
            ? "Start a companion to use it here."
            : "No companions installed yet."}
        </p>
        {!hasAny && (
          <p className="mt-2 text-xs text-foreground-muted">
            OpenDbPylot lets you ask your database questions in plain English.
            Install it with <code className="font-mono">cargo install opendbpylot</code>.
          </p>
        )}
      </div>
    </div>
  );
}
