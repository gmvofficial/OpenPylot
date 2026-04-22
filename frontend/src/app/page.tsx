"use client";

import Link from "next/link";
import { ArrowRight, Bot, Brain, MessageSquare, Share2, Sparkles, Users, Wrench, Zap } from "lucide-react";
import { Button } from "@/components/ui/button";

const features = [
  {
    icon: MessageSquare,
    title: "Streaming chat",
    description: "Token-by-token responses with tool-use streaming and inline citations.",
  },
  {
    icon: Brain,
    title: "Smart memory",
    description: "SQLite + embeddings + FTS5. Auto-extracts facts, retrieves them on demand.",
  },
  {
    icon: Users,
    title: "Plug-and-play agents",
    description: "Drop a .toml manifest in agents/ to register a new specialist — no rebuild.",
  },
  {
    icon: Wrench,
    title: "Skills system",
    description: "30+ bundled SKILL.md files. Author your own; they route by intent.",
  },
  {
    icon: Share2,
    title: "17 social platforms",
    description: "Compose, schedule, analyze — X, LinkedIn, Bluesky, Mastodon, Threads, and more.",
  },
  {
    icon: Sparkles,
    title: "MCP-ready",
    description: "Connect external tool servers via JSON-RPC. All tools appear as native capabilities.",
  },
];

export default function HomePage() {
  return (
    <div className="h-full overflow-auto">
      <div className="max-w-6xl mx-auto px-6 py-16">
        {/* Hero */}
        <div className="text-center mb-16">
          <div className="inline-flex items-center justify-center w-14 h-14 rounded-2xl bg-accent/10 border border-accent/20 mb-6">
            <Bot className="w-7 h-7 text-accent" />
          </div>
          <h1 className="text-4xl sm:text-5xl font-bold text-foreground tracking-tight">
            Your personal AI,
            <br />
            <span className="text-accent">powered by Rust.</span>
          </h1>
          <p className="mt-5 text-lg text-foreground-secondary max-w-2xl mx-auto leading-relaxed">
            OpenPylot is a local-first, plug-and-play AI assistant. Stream chat, spawn
            sub-agents, remember everything, and script social across 17 platforms —
            all from one binary.
          </p>
          <div className="mt-8 flex items-center justify-center gap-3">
            <Link href="/chat">
              <Button size="lg" className="gap-2">
                Start chatting
                <ArrowRight className="w-4 h-4" />
              </Button>
            </Link>
            <Link href="/setup">
              <Button variant="ghost" size="lg">
                Connect integrations
              </Button>
            </Link>
          </div>
        </div>

        {/* Features grid */}
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {features.map(({ icon: Icon, title, description }) => (
            <div
              key={title}
              className="group relative rounded-xl border border-border bg-background-secondary p-5 transition-colors hover:border-border-hover"
            >
              <div className="flex items-center justify-center w-10 h-10 rounded-lg bg-accent/10 text-accent mb-4 group-hover:bg-accent/15 transition-colors">
                <Icon className="w-5 h-5" />
              </div>
              <h3 className="text-sm font-semibold text-foreground mb-1.5">{title}</h3>
              <p className="text-sm text-foreground-secondary leading-relaxed">{description}</p>
            </div>
          ))}
        </div>

        {/* Quick links */}
        <div className="mt-16 grid sm:grid-cols-3 gap-3">
          {[
            { href: "/dashboard", label: "Dashboard", icon: Zap, hint: "Live status + logs" },
            { href: "/agents", label: "Sub-Agents", icon: Users, hint: "Presets + runs" },
            { href: "/memory", label: "Memory", icon: Brain, hint: "Browse & search" },
          ].map(({ href, label, icon: Icon, hint }) => (
            <Link
              key={href}
              href={href}
              className="flex items-center gap-3 rounded-xl border border-border bg-background-secondary/50 px-4 py-3 transition-colors hover:bg-background-secondary hover:border-border-hover"
            >
              <Icon className="w-4 h-4 text-accent shrink-0" />
              <div className="flex-1 min-w-0">
                <div className="text-sm font-medium text-foreground">{label}</div>
                <div className="text-xs text-foreground-muted truncate">{hint}</div>
              </div>
              <ArrowRight className="w-4 h-4 text-foreground-muted" />
            </Link>
          ))}
        </div>

        <p className="mt-16 text-center text-xs text-foreground-muted">
          OpenPylot v1.0.0-rc1 · <Link href="/settings" className="hover:text-foreground transition-colors">settings</Link>
        </p>
      </div>
    </div>
  );
}
