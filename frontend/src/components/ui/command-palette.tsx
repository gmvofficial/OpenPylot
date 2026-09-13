"use client";

import * as React from "react";
import { createPortal } from "react-dom";
import { useRouter } from "next/navigation";
import {
  BarChart3,
  BookOpen,
  Bot,
  Brain,
  Database,
  LayoutDashboard,
  MessageSquare,
  Moon,
  Plus,
  Search,
  Settings,
  Settings2,
  Share2,
  Sun,
  Monitor,
  Users,
  Wrench,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { setTheme } from "@/lib/theme";
import { useChatStore } from "@/stores/chat";

/**
 * ⌘K command palette.
 *
 * The app has ten routes and an unbounded list of conversations, and no
 * keyboard path to any of them — every competitor in this category ships one,
 * and for an agent UI it is the primary way to move, not a power-user extra.
 *
 * Matching is subsequence-based rather than substring, so "ktb" finds
 * "Knowledge Base" the way a fuzzy finder does.
 */

interface Command {
  id: string;
  label: string;
  /** Extra words that should match this command without being displayed. */
  keywords?: string;
  group: string;
  Icon: typeof Search;
  run: () => void;
}

export function CommandPalette() {
  const router = useRouter();
  const [open, setOpen] = React.useState(false);
  const [query, setQuery] = React.useState("");
  const [selected, setSelected] = React.useState(0);
  const [mounted, setMounted] = React.useState(false);
  const inputRef = React.useRef<HTMLInputElement>(null);
  const listRef = React.useRef<HTMLDivElement>(null);

  const conversations = useChatStore((s) => s.conversations);
  const loadConversation = useChatStore((s) => s.loadConversation);
  const newConversation = useChatStore((s) => s.newConversation);

  React.useEffect(() => setMounted(true), []);

  const commands = React.useMemo<Command[]>(() => {
    const go = (href: string) => () => router.push(href);
    const navigation: Command[] = [
      { id: "chat", label: "Chat", group: "Go to", Icon: MessageSquare, run: go("/chat") },
      { id: "social", label: "Social Media", group: "Go to", Icon: Share2, run: go("/social") },
      { id: "agents", label: "Sub-Agents", group: "Go to", Icon: Users, run: go("/agents") },
      { id: "memory", label: "Memory", group: "Go to", Icon: Brain, run: go("/memory") },
      { id: "setup", label: "Integrations", group: "Go to", Icon: Settings2, run: go("/setup") },
      { id: "knowledge", label: "Knowledge Base", group: "Go to", Icon: BookOpen, run: go("/knowledge") },
      { id: "companions", label: "Companions", keywords: "database dbpylot sql", group: "Go to", Icon: Database, run: go("/companions") },
      { id: "tools", label: "Tools & Skills", group: "Go to", Icon: Wrench, run: go("/tools") },
      { id: "dashboard", label: "Dashboard", group: "Go to", Icon: LayoutDashboard, run: go("/dashboard") },
      { id: "settings", label: "Settings", group: "Go to", Icon: Settings, run: go("/settings") },
    ];

    const actions: Command[] = [
      {
        id: "new-chat",
        label: "New conversation",
        keywords: "start fresh clear",
        group: "Actions",
        Icon: Plus,
        run: () => {
          newConversation();
          router.push("/chat");
        },
      },
      { id: "theme-light", label: "Switch to light theme", keywords: "appearance colour color", group: "Actions", Icon: Sun, run: () => setTheme("light") },
      { id: "theme-dark", label: "Switch to dark theme", keywords: "appearance colour color", group: "Actions", Icon: Moon, run: () => setTheme("dark") },
      { id: "theme-system", label: "Match system theme", keywords: "appearance auto", group: "Actions", Icon: Monitor, run: () => setTheme("system") },
      { id: "usage", label: "View usage and cost", group: "Actions", Icon: BarChart3, run: go("/dashboard") },
    ];

    // Recent conversations, so switching between them is a keystroke.
    const recent: Command[] = conversations.slice(0, 20).map((c) => ({
      id: `conversation-${c.id}`,
      label: c.title || "Untitled conversation",
      group: "Conversations",
      Icon: Bot,
      run: () => {
        loadConversation(c.id);
        router.push("/chat");
      },
    }));

    return [...actions, ...navigation, ...recent];
  }, [router, conversations, loadConversation, newConversation]);

  const matches = React.useMemo(() => rank(commands, query), [commands, query]);

  // Open with ⌘K / Ctrl+K, close with Escape.
  React.useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "k" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        setOpen((o) => !o);
        return;
      }
      if (e.key === "Escape") setOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // Reset each time it opens, so it never reopens mid-search.
  React.useEffect(() => {
    if (!open) return;
    setQuery("");
    setSelected(0);
    const id = requestAnimationFrame(() => inputRef.current?.focus());
    return () => cancelAnimationFrame(id);
  }, [open]);

  // Keep the highlighted row in view when arrowing past the fold.
  React.useEffect(() => {
    listRef.current
      ?.querySelector<HTMLElement>(`[data-index="${selected}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [selected]);

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelected((i) => (matches.length ? (i + 1) % matches.length : 0));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelected((i) => (matches.length ? (i - 1 + matches.length) % matches.length : 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      const command = matches[selected];
      if (command) {
        command.run();
        setOpen(false);
      }
    }
  };

  if (!mounted || !open) return null;

  return createPortal(
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/40 pt-[12vh] backdrop-blur-sm"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) setOpen(false);
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Command palette"
        className="mx-4 w-full max-w-lg overflow-hidden rounded-xl border border-border bg-background shadow-2xl"
      >
        <div className="flex items-center gap-3 border-b border-border px-4">
          <Search className="h-4 w-4 shrink-0 text-foreground-muted" />
          <input
            id="command-palette-input"
            ref={inputRef}
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setSelected(0);
            }}
            onKeyDown={onKeyDown}
            placeholder="Search commands and conversations…"
            autoComplete="off"
            spellCheck={false}
            className="flex-1 bg-transparent py-3.5 text-sm text-foreground outline-none placeholder:text-foreground-muted"
          />
          <kbd className="shrink-0 rounded border border-border px-1.5 py-0.5 font-mono text-[10px] text-foreground-muted">
            esc
          </kbd>
        </div>

        <div ref={listRef} className="max-h-80 overflow-y-auto p-2 scrollbar-thin">
          {matches.length === 0 ? (
            <p className="px-3 py-8 text-center text-sm text-foreground-muted">
              Nothing matches “{query}”.
            </p>
          ) : (
            groupBy(matches).map(([group, items]) => (
              <div key={group} className="mb-1">
                <p className="px-3 py-1.5 text-[10px] font-medium uppercase tracking-wider text-foreground-muted">
                  {group}
                </p>
                {items.map(({ command, index }) => (
                  <button
                    key={command.id}
                    data-index={index}
                    type="button"
                    onMouseEnter={() => setSelected(index)}
                    onClick={() => {
                      command.run();
                      setOpen(false);
                    }}
                    className={cn(
                      "flex w-full items-center gap-3 rounded-lg px-3 py-2 text-left text-sm transition-colors",
                      index === selected
                        ? "bg-accent/10 text-accent"
                        : "text-foreground-secondary hover:bg-background-tertiary"
                    )}
                  >
                    <command.Icon className="h-4 w-4 shrink-0" />
                    <span className="truncate">{command.label}</span>
                  </button>
                ))}
              </div>
            ))
          )}
        </div>
      </div>
    </div>,
    document.body
  );
}

/** Rank commands against a query, dropping non-matches. */
function rank(commands: Command[], query: string): Command[] {
  const q = query.trim().toLowerCase();
  if (!q) return commands;

  const scored: { command: Command; score: number }[] = [];
  for (const command of commands) {
    const haystack = `${command.label} ${command.keywords ?? ""}`.toLowerCase();
    const score = subsequenceScore(q, haystack);
    if (score !== null) scored.push({ command, score });
  }
  scored.sort((a, b) => b.score - a.score);
  return scored.map((s) => s.command);
}

/**
 * Score `query` as a subsequence of `text`, or null if it does not match.
 *
 * Subsequence rather than substring, so "ktb" finds "Knowledge Base" — the
 * behaviour people expect from a fuzzy finder. Adjacent and word-initial
 * matches score higher, so the obvious candidate leads.
 */
function subsequenceScore(query: string, text: string): number | null {
  if (text.startsWith(query)) return 1000 - text.length;

  let score = 0;
  let cursor = 0;
  let previous = -2;

  for (const ch of query) {
    const found = text.indexOf(ch, cursor);
    if (found === -1) return null;
    score += 1;
    if (found === previous + 1) score += 3;
    if (found === 0 || text[found - 1] === " ") score += 2;
    previous = found;
    cursor = found + 1;
  }
  return score;
}

/** Group ranked commands while keeping their global index for keyboard nav. */
function groupBy(commands: Command[]): [string, { command: Command; index: number }[]][] {
  const groups = new Map<string, { command: Command; index: number }[]>();
  commands.forEach((command, index) => {
    const list = groups.get(command.group) ?? [];
    list.push({ command, index });
    groups.set(command.group, list);
  });
  return [...groups.entries()];
}
