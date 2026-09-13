"use client";

import * as React from "react";
import {
  Plug,
  Plus,
  Trash2,
  Loader2,
  CheckCircle2,
  XCircle,
  Power,
  Wrench,
  X,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { apiClient } from "@/lib/api";
import { useToastStore } from "@/stores/toast";
import { cn } from "@/lib/utils";
import type { McpConfiguredServer } from "@/types";

/**
 * MCP servers — what is configured, what is connected, and what it provides.
 *
 * Configuring one used to mean hand-editing `~/.pylot/mcp-servers.json` with no
 * feedback until the next restart. The list here shows *configured* servers
 * rather than only connected ones, so a disabled or failing server is visible
 * instead of silently absent — which is how a broken MCP setup stays invisible.
 */
export default function McpPage() {
  const [servers, setServers] = React.useState<McpConfiguredServer[] | null>(null);
  const [busy, setBusy] = React.useState<string | null>(null);
  const [adding, setAdding] = React.useState(false);
  const [results, setResults] = React.useState<Record<string, TestResult>>({});
  const addToast = useToastStore((s) => s.addToast);

  const refresh = React.useCallback(async () => {
    try {
      setServers(await apiClient.getMcpConfig());
    } catch {
      setServers([]);
    }
  }, []);

  React.useEffect(() => {
    refresh();
  }, [refresh]);

  const test = async (name: string) => {
    setBusy(name);
    try {
      const result = await apiClient.testMcpServer(name);
      setResults((r) => ({ ...r, [name]: result }));
    } catch (e) {
      setResults((r) => ({
        ...r,
        [name]: { connected: false, message: e instanceof Error ? e.message : String(e), tools: [] },
      }));
    } finally {
      setBusy(null);
    }
  };

  const toggle = async (server: McpConfiguredServer) => {
    setBusy(server.name);
    try {
      await apiClient.setMcpServerEnabled(server.name, !server.enabled);
      await refresh();
      addToast({
        title: server.enabled ? "Disabled" : "Enabled",
        description: "Restart the server for this to take effect.",
        variant: "info",
      });
    } finally {
      setBusy(null);
    }
  };

  const remove = async (name: string) => {
    setBusy(name);
    try {
      await apiClient.deleteMcpServer(name);
      setResults((r) => {
        const next = { ...r };
        delete next[name];
        return next;
      });
      await refresh();
    } finally {
      setBusy(null);
    }
  };

  if (servers === null) {
    return (
      <div className="flex h-full items-center justify-center">
        <Loader2 className="h-5 w-5 animate-spin text-foreground-muted" />
      </div>
    );
  }

  return (
    <div className="mx-auto max-w-3xl px-6 py-8">
      <header className="flex items-start justify-between gap-4">
        <div>
          <h1 className="text-base font-semibold text-foreground">MCP servers</h1>
          <p className="mt-1 max-w-prose text-sm text-foreground-secondary">
            Model Context Protocol servers add their tools to the assistant. They are available
            in chat and in the terminal.
          </p>
        </div>
        <Button size="sm" onClick={() => setAdding(true)}>
          <Plus className="mr-1.5 h-3.5 w-3.5" />
          Add server
        </Button>
      </header>

      {adding && (
        <AddServerForm
          onCancel={() => setAdding(false)}
          onAdded={async () => {
            setAdding(false);
            await refresh();
            addToast({
              title: "Server added",
              description: "Restart the server to connect it.",
              variant: "success",
            });
          }}
        />
      )}

      <div className="mt-6 space-y-3">
        {servers.length === 0 && !adding ? (
          <EmptyState onAdd={() => setAdding(true)} />
        ) : (
          servers.map((server) => (
            <ServerCard
              key={server.name}
              server={server}
              busy={busy === server.name}
              result={results[server.name]}
              onTest={() => test(server.name)}
              onToggle={() => toggle(server)}
              onRemove={() => remove(server.name)}
            />
          ))
        )}
      </div>

      <section className="mt-10 rounded-lg border border-border bg-background-tertiary p-4">
        <h2 className="text-sm font-medium text-foreground">Use OpenPylot from another host</h2>
        <p className="mt-1 text-sm text-foreground-secondary">
          This also works the other way round. Add this to Claude Desktop, Claude Code, or any
          MCP host to reach the assistant, its memory and its skills from there.
        </p>
        <pre className="mt-3 overflow-x-auto rounded bg-background p-3 font-mono text-xs text-foreground-secondary">
{`{"openpylot": {"command": "pylot", "args": ["mcp", "serve"]}}`}
        </pre>
      </section>
    </div>
  );
}

interface TestResult {
  connected: boolean;
  message: string;
  tools: string[];
}

function ServerCard({
  server,
  busy,
  result,
  onTest,
  onToggle,
  onRemove,
}: {
  server: McpConfiguredServer;
  busy: boolean;
  result?: TestResult;
  onTest: () => void;
  onToggle: () => void;
  onRemove: () => void;
}) {
  return (
    <div
      className={cn(
        "rounded-lg border p-4",
        server.enabled ? "border-border bg-background-secondary" : "border-border bg-background"
      )}
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <Plug
              className={cn(
                "h-4 w-4 shrink-0",
                server.connected ? "text-accent-success" : "text-foreground-muted"
              )}
            />
            <span className="font-medium text-foreground">{server.name}</span>
            <span className="rounded bg-background-tertiary px-1.5 py-0.5 font-mono text-[10px] text-foreground-muted">
              {server.transport}
            </span>
            {server.connected ? (
              <span className="rounded bg-accent-success/10 px-1.5 py-0.5 text-[10px] font-medium text-accent-success">
                {server.tool_count} tool{server.tool_count === 1 ? "" : "s"}
              </span>
            ) : server.enabled ? (
              <span className="rounded bg-background-tertiary px-1.5 py-0.5 text-[10px] text-foreground-muted">
                Not connected
              </span>
            ) : (
              <span className="rounded bg-background-tertiary px-1.5 py-0.5 text-[10px] text-foreground-muted">
                Disabled
              </span>
            )}
          </div>
          <p className="mt-1.5 truncate font-mono text-xs text-foreground-muted">{server.target}</p>
        </div>

        <div className="flex shrink-0 gap-1">
          <Button size="sm" variant="ghost" onClick={onTest} disabled={busy} title="Test connection">
            {busy ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Wrench className="h-3.5 w-3.5" />}
          </Button>
          <Button
            size="sm"
            variant="ghost"
            onClick={onToggle}
            disabled={busy}
            title={server.enabled ? "Disable" : "Enable"}
          >
            <Power className={cn("h-3.5 w-3.5", server.enabled && "text-accent-success")} />
          </Button>
          <Button size="sm" variant="ghost" onClick={onRemove} disabled={busy} title="Remove">
            <Trash2 className="h-3.5 w-3.5 text-accent-error" />
          </Button>
        </div>
      </div>

      {result && (
        <div
          className={cn(
            "mt-3 rounded px-3 py-2 text-xs",
            result.connected
              ? "bg-accent-success/10 text-accent-success"
              : "bg-accent-error/10 text-accent-error"
          )}
        >
          <p className="flex items-center gap-1.5 font-medium">
            {result.connected ? (
              <CheckCircle2 className="h-3.5 w-3.5" />
            ) : (
              <XCircle className="h-3.5 w-3.5" />
            )}
            {result.message}
          </p>
          {result.tools.length > 0 && (
            <ul className="mt-2 space-y-0.5 font-mono text-[11px] text-foreground-secondary">
              {result.tools.map((tool) => (
                <li key={tool}>{tool}</li>
              ))}
            </ul>
          )}
        </div>
      )}
    </div>
  );
}

function AddServerForm({ onCancel, onAdded }: { onCancel: () => void; onAdded: () => void }) {
  const [name, setName] = React.useState("");
  const [command, setCommand] = React.useState("");
  const [args, setArgs] = React.useState("");
  const [saving, setSaving] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setSaving(true);
    setError(null);
    try {
      await apiClient.upsertMcpServer({
        name: name.trim(),
        command: command.trim(),
        // Whitespace-separated, which is how these are written on a command line.
        args: args.trim() ? args.trim().split(/\s+/) : undefined,
      });
      onAdded();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <form
      onSubmit={submit}
      className="mt-6 space-y-3 rounded-lg border border-accent/30 bg-background-secondary p-4"
    >
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-medium text-foreground">Add an MCP server</h2>
        <button type="button" onClick={onCancel} aria-label="Cancel" className="text-foreground-muted">
          <X className="h-4 w-4" />
        </button>
      </div>

      <Field
        id="mcp-name"
        label="Name"
        hint="Becomes the tool prefix: mcp_<name>_<tool>"
        value={name}
        onChange={setName}
        placeholder="dbpylot"
      />
      <Field
        id="mcp-command"
        label="Command"
        hint="The executable to run"
        value={command}
        onChange={setCommand}
        placeholder="dbpylot"
      />
      <Field
        id="mcp-args"
        label="Arguments"
        hint="Optional, space-separated"
        value={args}
        onChange={setArgs}
        placeholder="mcp"
      />

      {error && <p className="text-xs text-accent-error">{error}</p>}

      <div className="flex gap-2 pt-1">
        <Button type="submit" size="sm" disabled={saving || !name.trim() || !command.trim()}>
          {saving ? <Loader2 className="mr-1.5 h-3.5 w-3.5 animate-spin" /> : null}
          Add
        </Button>
        <Button type="button" size="sm" variant="ghost" onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </form>
  );
}

function Field({
  id,
  label,
  hint,
  value,
  onChange,
  placeholder,
}: {
  id: string;
  label: string;
  hint: string;
  value: string;
  onChange: (v: string) => void;
  placeholder: string;
}) {
  return (
    <div>
      <label htmlFor={id} className="block text-xs font-medium text-foreground-secondary">
        {label}
      </label>
      <input
        id={id}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        autoComplete="off"
        spellCheck={false}
        className="mt-1 w-full rounded-lg border border-border bg-background-input px-3 py-2 font-mono text-sm text-foreground outline-none focus:border-accent/40 focus:ring-2 focus:ring-accent/30"
      />
      <p className="mt-1 text-[11px] text-foreground-muted">{hint}</p>
    </div>
  );
}

function EmptyState({ onAdd }: { onAdd: () => void }) {
  return (
    <div className="rounded-lg border border-dashed border-border p-8 text-center">
      <Plug className="mx-auto h-7 w-7 text-foreground-muted" />
      <p className="mt-3 text-sm text-foreground-secondary">No MCP servers configured.</p>
      <p className="mx-auto mt-1 max-w-sm text-xs text-foreground-muted">
        OpenDbPylot is the easiest one to start with — it lets the assistant query your database
        in plain English. Install it, then add it here as{" "}
        <code className="font-mono">dbpylot mcp</code>.
      </p>
      <Button size="sm" className="mt-4" onClick={onAdd}>
        <Plus className="mr-1.5 h-3.5 w-3.5" />
        Add server
      </Button>
    </div>
  );
}
