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
import { Skeleton } from "@/components/ui/skeleton";
import { Separator } from "@/components/ui/separator";
import { apiClient } from "@/lib/api";
import type { ToolDefinition, Skill } from "@/types";
import {
  Wrench,
  Zap,
  Search,
  Blocks,
  BrainCircuit,
  Globe,
  Calendar,
  Mail,
  MessageSquare,
  FileText,
  Terminal,
  Database,
  RefreshCw,
  Trash2,
} from "lucide-react";

/* -------------------------------------------------------------------------- */
/*  Tool icon resolver                                                        */
/* -------------------------------------------------------------------------- */

function toolIcon(name: string) {
  if (name.includes("bash") || name.includes("terminal")) return Terminal;
  if (name.includes("file") || name.includes("write") || name.includes("read") || name.includes("edit")) return FileText;
  if (name.includes("search") || name.includes("grep") || name.includes("glob")) return Search;
  if (name.includes("web")) return Globe;
  if (name.includes("calendar") || name.includes("meeting")) return Calendar;
  if (name.includes("gmail") || name.includes("email")) return Mail;
  if (name.includes("telegram") || name.includes("message")) return MessageSquare;
  if (name.includes("memory") || name.includes("remember") || name.includes("recall") || name.includes("forget")) return BrainCircuit;
  if (name.includes("knowledge") || name.includes("document") || name.includes("note")) return Database;
  return Wrench;
}

function categoryColor(category: string) {
  switch (category.toLowerCase()) {
    case "coding": return "bg-accent/10 text-accent";
    case "agentic": return "bg-purple-500/10 text-purple-400";
    case "research": return "bg-green-500/10 text-accent-success";
    case "communication": return "bg-amber-500/10 text-amber-400";
    case "productivity": return "bg-teal-500/10 text-teal-400";
    default: return "bg-foreground-muted/10 text-foreground-muted";
  }
}

/* -------------------------------------------------------------------------- */
/*  Tools Section                                                             */
/* -------------------------------------------------------------------------- */

function ToolsSection({
  tools,
  loading,
  filter,
}: {
  tools: ToolDefinition[];
  loading: boolean;
  filter: string;
}) {
  const filtered = tools.filter(
    (t) =>
      t.name.toLowerCase().includes(filter.toLowerCase()) ||
      t.description.toLowerCase().includes(filter.toLowerCase())
  );

  if (loading) {
    return (
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
        {Array.from({ length: 9 }).map((_, i) => (
          <Skeleton key={i} className="h-24 w-full rounded-lg" />
        ))}
      </div>
    );
  }

  return (
    <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
      {filtered.map((tool) => {
        const Icon = toolIcon(tool.name);
        return (
          <Card key={tool.name} className="group hover:border-border-hover transition-colors">
            <CardContent className="flex items-start gap-3 py-3">
              <div className="rounded-lg bg-accent/10 p-2 shrink-0">
                <Icon className="h-4 w-4 text-accent" />
              </div>
              <div className="min-w-0">
                <p className="text-sm font-medium text-foreground truncate">{tool.name}</p>
                <p className="text-xs text-foreground-muted line-clamp-2 mt-0.5">
                  {tool.description}
                </p>
              </div>
            </CardContent>
          </Card>
        );
      })}
      {filtered.length === 0 && (
        <div className="col-span-full py-8 text-center text-sm text-foreground-muted">
          No tools match &quot;{filter}&quot;
        </div>
      )}
    </div>
  );
}

/* -------------------------------------------------------------------------- */
/*  Skills Section                                                            */
/* -------------------------------------------------------------------------- */

function SkillsSection({
  skills,
  loading,
  filter,
  onToggle,
  onDelete,
}: {
  skills: Skill[];
  loading: boolean;
  filter: string;
  onToggle: (name: string, enabled: boolean) => void;
  onDelete: (name: string) => void;
}) {
  const filtered = skills.filter(
    (s) =>
      s.name.toLowerCase().includes(filter.toLowerCase()) ||
      s.description.toLowerCase().includes(filter.toLowerCase()) ||
      s.category.toLowerCase().includes(filter.toLowerCase())
  );

  // Group by category
  const grouped = filtered.reduce<Record<string, Skill[]>>((acc, skill) => {
    const cat = skill.category || "other";
    (acc[cat] = acc[cat] || []).push(skill);
    return acc;
  }, {});

  // Fixed category order
  const sortedCategories = Object.entries(grouped).sort(([a], [b]) => a.localeCompare(b));

  if (loading) {
    return (
      <div className="space-y-3">
        {Array.from({ length: 4 }).map((_, i) => (
          <Skeleton key={i} className="h-20 w-full rounded-lg" />
        ))}
      </div>
    );
  }

  if (sortedCategories.length === 0) {
    return (
      <div className="py-8 text-center text-sm text-foreground-muted">
        <Zap className="mx-auto mb-2 h-8 w-8" />
        {filter ? `No skills match "${filter}"` : "No skills loaded"}
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {sortedCategories.map(([category, catSkills]) => (
        <div key={category}>
          <h3 className="mb-3 flex items-center gap-2 text-xs font-semibold uppercase tracking-wider text-foreground-muted">
            <Badge className={categoryColor(category)}>{category}</Badge>
            <span>{catSkills.length} skill{catSkills.length !== 1 && "s"}</span>
          </h3>
          <div className="grid gap-3 sm:grid-cols-2">
            {catSkills.map((skill) => (
              <Card key={skill.name} className={`transition-colors ${skill.enabled === false ? "opacity-50" : "hover:border-border-hover"}`}>
                <CardContent className="py-3 space-y-1.5">
                  <div className="flex items-center justify-between">
                    <p className="text-sm font-medium text-foreground">{skill.name}</p>
                    <div className="flex items-center gap-2">
                      <button
                        onClick={() => onToggle(skill.name, skill.enabled === false)}
                        className={`relative inline-flex h-5 w-9 items-center rounded-full transition-colors ${
                          skill.enabled !== false ? "bg-green-500" : "bg-gray-400"
                        }`}
                        title={skill.enabled !== false ? "Disable skill" : "Enable skill"}
                      >
                        <span
                          className={`inline-block h-3.5 w-3.5 transform rounded-full bg-white transition-transform ${
                            skill.enabled !== false ? "translate-x-[18px]" : "translate-x-[2px]"
                          }`}
                        />
                      </button>
                      <button
                        onClick={() => {
                          if (confirm(`Delete skill "${skill.name}"? This cannot be undone.`)) {
                            onDelete(skill.name);
                          }
                        }}
                        className="text-foreground-muted hover:text-red-500 transition-colors"
                        title="Delete skill"
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                      </button>
                    </div>
                  </div>
                  <p className="text-xs text-foreground-muted line-clamp-2">
                    {skill.description}
                  </p>
                  {skill.triggers && skill.triggers.length > 0 && (
                    <div className="flex flex-wrap gap-1 pt-1">
                      {skill.triggers.slice(0, 3).map((t, i) => (
                        <Badge key={i} variant="outline" className="text-[10px]">
                          {t}
                        </Badge>
                      ))}
                    </div>
                  )}
                </CardContent>
              </Card>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}

/* -------------------------------------------------------------------------- */
/*  Tools & Skills Page                                                       */
/* -------------------------------------------------------------------------- */

export default function ToolsPage() {
  const [tools, setTools] = useState<ToolDefinition[]>([]);
  const [skills, setSkills] = useState<Skill[]>([]);
  const [loadingTools, setLoadingTools] = useState(true);
  const [loadingSkills, setLoadingSkills] = useState(true);
  const [filter, setFilter] = useState("");
  const [tab, setTab] = useState<"tools" | "skills">("tools");

  const loadAll = async () => {
    setLoadingTools(true);
    setLoadingSkills(true);
    try {
      const [t, s] = await Promise.allSettled([
        apiClient.getTools(),
        apiClient.getSkills(),
      ]);
      if (t.status === "fulfilled") setTools(t.value);
      if (s.status === "fulfilled") setSkills(Array.isArray(s.value) ? s.value : []);
    } finally {
      setLoadingTools(false);
      setLoadingSkills(false);
    }
  };

  useEffect(() => {
    loadAll();
  }, []);

  const handleToggleSkill = async (name: string, enabled: boolean) => {
    try {
      await apiClient.updateSkill(name, enabled);
      setSkills((prev) =>
        prev.map((s) => (s.name === name ? { ...s, enabled } : s))
      );
    } catch (e) {
      console.error("Failed to toggle skill:", e);
    }
  };

  const handleDeleteSkill = async (name: string) => {
    try {
      await apiClient.deleteSkill(name);
      setSkills((prev) => prev.filter((s) => s.name !== name));
    } catch (e) {
      console.error("Failed to delete skill:", e);
    }
  };

  return (
    <div className="mx-auto max-w-5xl space-y-6 p-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold text-foreground">Tools & Skills</h1>
          <p className="mt-1 text-sm text-foreground-secondary">
            {tools.length} tools and {skills.length} skills available to your agent.
          </p>
        </div>
        <Button variant="outline" size="sm" onClick={loadAll}>
          <RefreshCw className="mr-2 h-4 w-4" />
          Refresh
        </Button>
      </div>

      {/* Tabs + Search */}
      <div className="flex items-center gap-4">
        <div className="flex rounded-lg border border-border bg-background-secondary p-0.5">
          <button
            onClick={() => setTab("tools")}
            className={`rounded-md px-4 py-1.5 text-sm font-medium transition-colors ${
              tab === "tools"
                ? "bg-background text-foreground shadow-sm"
                : "text-foreground-muted hover:text-foreground"
            }`}
          >
            <Wrench className="mr-1.5 inline h-3.5 w-3.5" />
            Tools ({tools.length})
          </button>
          <button
            onClick={() => setTab("skills")}
            className={`rounded-md px-4 py-1.5 text-sm font-medium transition-colors ${
              tab === "skills"
                ? "bg-background text-foreground shadow-sm"
                : "text-foreground-muted hover:text-foreground"
            }`}
          >
            <Zap className="mr-1.5 inline h-3.5 w-3.5" />
            Skills ({skills.length})
          </button>
        </div>
        <div className="relative flex-1 max-w-sm">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-foreground-muted" />
          <Input
            placeholder={`Search ${tab}...`}
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            className="pl-9"
          />
        </div>
      </div>

      {/* Content */}
      {tab === "tools" ? (
        <ToolsSection tools={tools} loading={loadingTools} filter={filter} />
      ) : (
        <SkillsSection
          skills={skills}
          loading={loadingSkills}
          filter={filter}
          onToggle={handleToggleSkill}
          onDelete={handleDeleteSkill}
        />
      )}
    </div>
  );
}
