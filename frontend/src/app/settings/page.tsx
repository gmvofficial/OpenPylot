"use client";

import { useEffect, useState } from "react";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Separator } from "@/components/ui/separator";
import { Skeleton } from "@/components/ui/skeleton";
import type { AgentSettings, MemoryFact } from "@/types";
import { apiClient } from "@/lib/api";
import { formatRelativeTime } from "@/lib/utils";
import {
  Settings,
  Brain,
  Save,
  RefreshCw,
  Trash2,
  Edit3,
  Check,
  X,
  Bot,
  Thermometer,
  Cpu,
  User,
  MessageSquare,
  ChevronRight,
} from "lucide-react";

/* -------------------------------------------------------------------------- */
/*  Agent Settings Section                                                    */
/* -------------------------------------------------------------------------- */

function AgentSettingsEditor() {
  const [settings, setSettings] = useState<AgentSettings | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [dirty, setDirty] = useState(false);

  useEffect(() => {
    (async () => {
      try {
        setLoading(true);
        const data = await apiClient.getSettings();
        setSettings(data);
      } finally {
        setLoading(false);
      }
    })();
  }, []);

  const update = (field: keyof AgentSettings, value: string | number) => {
    if (!settings) return;
    setSettings({ ...settings, [field]: value });
    setDirty(true);
  };

  const handleSave = async () => {
    if (!settings || !dirty) return;
    setSaving(true);
    try {
      await apiClient.updateSettings(settings);
      setDirty(false);
    } finally {
      setSaving(false);
    }
  };

  if (loading) {
    return (
      <Card>
        <CardHeader>
          <Skeleton className="h-5 w-32" />
          <Skeleton className="h-3 w-48" />
        </CardHeader>
        <CardContent className="space-y-4">
          {Array.from({ length: 4 }).map((_, i) => (
            <Skeleton key={i} className="h-10 w-full" />
          ))}
        </CardContent>
      </Card>
    );
  }

  if (!settings) return null;

  return (
    <Card>
      <CardHeader className="flex flex-row items-center justify-between space-y-0">
        <div>
          <CardTitle className="flex items-center gap-2">
            <Settings className="h-5 w-5 text-accent" />
            Agent Configuration
          </CardTitle>
          <CardDescription className="mt-1">
            Customize your agent&apos;s behavior and personality.
          </CardDescription>
        </div>
        <Button onClick={handleSave} disabled={!dirty || saving} size="sm">
          <Save className="mr-2 h-4 w-4" />
          {saving ? "Saving..." : "Save"}
        </Button>
      </CardHeader>
      <CardContent className="space-y-5">
        {/* Agent Name */}
        <div className="space-y-1.5">
          <label className="flex items-center gap-2 text-sm font-medium text-foreground">
            <Bot className="h-4 w-4 text-foreground-muted" />
            Agent Name
          </label>
          <Input
            value={settings.agent_name ?? ""}
            onChange={(e) => update("agent_name", e.target.value)}
            placeholder="My Assistant"
          />
        </div>

        {/* User Name */}
        <div className="space-y-1.5">
          <label className="flex items-center gap-2 text-sm font-medium text-foreground">
            <User className="h-4 w-4 text-foreground-muted" />
            Your Name
          </label>
          <Input
            value={settings.user_name ?? ""}
            onChange={(e) => update("user_name", e.target.value)}
            placeholder="Your display name"
          />
        </div>

        {/* Model */}
        <div className="space-y-1.5">
          <label className="flex items-center gap-2 text-sm font-medium text-foreground">
            <Cpu className="h-4 w-4 text-foreground-muted" />
            Model
          </label>
          <Input
            value={settings.model ?? ""}
            onChange={(e) => update("model", e.target.value)}
            placeholder="claude-sonnet-4-20250514"
          />
          <p className="text-xs text-foreground-muted">
            The language model to use (e.g., claude-sonnet-4-20250514, gpt-4o)
          </p>
        </div>

        {/* Temperature */}
        <div className="space-y-1.5">
          <label className="flex items-center gap-2 text-sm font-medium text-foreground">
            <Thermometer className="h-4 w-4 text-foreground-muted" />
            Temperature
          </label>
          <div className="flex items-center gap-3">
            <input
              type="range"
              min="0"
              max="2"
              step="0.1"
              value={settings.temperature ?? 0.7}
              onChange={(e) => update("temperature", parseFloat(e.target.value))}
              className="flex-1 accent-accent"
            />
            <span className="w-10 text-right text-sm font-mono text-foreground-secondary">
              {(settings.temperature ?? 0.7).toFixed(1)}
            </span>
          </div>
        </div>

        {/* System Prompt / Persona */}
        <div className="space-y-1.5">
          <label className="flex items-center gap-2 text-sm font-medium text-foreground">
            <MessageSquare className="h-4 w-4 text-foreground-muted" />
            Persona / System Prompt
          </label>
          <Textarea
            value={settings.persona ?? ""}
            onChange={(e) => update("persona", e.target.value)}
            placeholder="You are a helpful AI assistant named GMV..."
            rows={5}
          />
          <p className="text-xs text-foreground-muted">
            Defines the agent&apos;s personality and behavior guidelines.
          </p>
        </div>
      </CardContent>
    </Card>
  );
}

/* -------------------------------------------------------------------------- */
/*  Memory Management                                                         */
/* -------------------------------------------------------------------------- */

function MemoryManager() {
  const [facts, setFacts] = useState<MemoryFact[]>([]);
  const [loading, setLoading] = useState(true);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editValue, setEditValue] = useState("");

  const loadMemory = async () => {
    try {
      setLoading(true);
      const data = await apiClient.getMemory();
      setFacts(data);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadMemory();
  }, []);

  const handleEdit = (fact: MemoryFact) => {
    setEditingId(fact.id);
    setEditValue(fact.content);
  };

  const handleSaveEdit = async () => {
    if (!editingId) return;
    await apiClient.updateMemoryFact(editingId, editValue);
    setFacts((prev) =>
      prev.map((f) => (f.id === editingId ? { ...f, content: editValue } : f))
    );
    setEditingId(null);
    setEditValue("");
  };

  const handleDelete = async (id: string) => {
    await apiClient.deleteMemoryFact(id);
    setFacts((prev) => prev.filter((f) => f.id !== id));
  };

  return (
    <Card>
      <CardHeader className="flex flex-row items-center justify-between space-y-0">
        <div>
          <CardTitle className="flex items-center gap-2">
            <Brain className="h-5 w-5 text-purple-400" />
            Memory
          </CardTitle>
          <CardDescription className="mt-1">
            Facts the agent has learned about you over time.
          </CardDescription>
        </div>
        <div className="flex items-center gap-2">
          <Badge variant="outline">{facts.length} facts</Badge>
          <Button size="sm" variant="ghost" onClick={loadMemory}>
            <RefreshCw className="h-4 w-4" />
          </Button>
        </div>
      </CardHeader>
      <CardContent>
        {loading ? (
          <div className="space-y-2">
            {Array.from({ length: 4 }).map((_, i) => (
              <Skeleton key={i} className="h-10 w-full" />
            ))}
          </div>
        ) : facts.length === 0 ? (
          <div className="py-8 text-center text-sm text-foreground-muted">
            <Brain className="mx-auto mb-2 h-8 w-8" />
            No memories stored yet. Chat with the agent to build its memory.
          </div>
        ) : (
          <div className="divide-y divide-border rounded-lg border border-border">
            {facts.map((fact) => (
              <div
                key={fact.id}
                className="flex items-center gap-3 px-4 py-3 hover:bg-background-secondary/30 transition-colors"
              >
                {editingId === fact.id ? (
                  <>
                    <Input
                      className="flex-1"
                      value={editValue}
                      onChange={(e) => setEditValue(e.target.value)}
                      autoFocus
                      onKeyDown={(e) => {
                        if (e.key === "Enter") handleSaveEdit();
                        if (e.key === "Escape") setEditingId(null);
                      }}
                    />
                    <Button size="icon" variant="ghost" onClick={handleSaveEdit}>
                      <Check className="h-4 w-4 text-green-400" />
                    </Button>
                    <Button size="icon" variant="ghost" onClick={() => setEditingId(null)}>
                      <X className="h-4 w-4" />
                    </Button>
                  </>
                ) : (
                  <>
                    <div className="flex-1">
                      <p className="text-sm text-foreground">{fact.content}</p>
                      <div className="mt-0.5 flex items-center gap-2">
                        <Badge variant="outline" className="text-xs">
                          {fact.category}
                        </Badge>
                        {fact.created_at && (
                          <span className="text-xs text-foreground-muted">
                            {formatRelativeTime(new Date(fact.created_at))}
                          </span>
                        )}
                      </div>
                    </div>
                    <Button
                      size="icon"
                      variant="ghost"
                      className="h-7 w-7"
                      onClick={() => handleEdit(fact)}
                    >
                      <Edit3 className="h-3.5 w-3.5" />
                    </Button>
                    <Button
                      size="icon"
                      variant="ghost"
                      className="h-7 w-7 text-foreground-muted hover:text-red-400"
                      onClick={() => handleDelete(fact.id)}
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </Button>
                  </>
                )}
              </div>
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  );
}

/* -------------------------------------------------------------------------- */
/*  Settings Page                                                             */
/* -------------------------------------------------------------------------- */

export default function SettingsPage() {
  return (
    <div className="mx-auto max-w-3xl space-y-8 p-6">
      <div>
        <h1 className="text-2xl font-bold text-foreground">Settings</h1>
        <p className="mt-1 text-sm text-foreground-secondary">
          Configure your agent and manage its memory.
        </p>
      </div>

      <AgentSettingsEditor />

      <Separator />

      <MemoryManager />
    </div>
  );
}
