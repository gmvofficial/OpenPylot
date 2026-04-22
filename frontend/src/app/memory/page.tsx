"use client";

import { useEffect, useState } from "react";
import { apiClient } from "@/lib/api";

interface MemoryUnit {
  id: string;
  type: string;
  content: string;
  summary?: string;
  importance: number;
  confidence: number;
  access_count: number;
  entities: string[];
  topics: string[];
  tags: string[];
  created_at: string;
  updated_at: string;
}

export default function MemoryPage() {
  const [units, setUnits] = useState<MemoryUnit[]>([]);
  const [loading, setLoading] = useState(true);
  const [filter, setFilter] = useState<string>("all");

  useEffect(() => {
    const fetchAll = async () => {
      const allUnits: MemoryUnit[] = [];

      // Fetch from legacy SmartMemory (/api/memory) — where remember_fact stores data
      try {
        const legacyMemories = await apiClient.getMemory();
        if (Array.isArray(legacyMemories)) {
          for (const m of legacyMemories) {
            allUnits.push({
              id: m.id,
              type: m.category || "general",
              content: m.content,
              summary: undefined,
              importance: 0.5,
              confidence: 1.0,
              access_count: 0,
              entities: [],
              topics: [],
              tags: [],
              created_at: m.created_at || m.createdAt || new Date().toISOString(),
              updated_at: m.created_at || m.createdAt || new Date().toISOString(),
            });
          }
        }
      } catch {
        // legacy endpoint unavailable
      }

      // Fetch from Memory v2 (/api/memory/v2/units)
      try {
        const res = await apiClient.getMemoryUnits() as { units?: MemoryUnit[] };
        if (res?.units && Array.isArray(res.units)) {
          allUnits.push(...res.units);
        }
      } catch {
        // v2 endpoint unavailable
      }

      setUnits(allUnits);
      setLoading(false);
    };
    fetchAll();
  }, []);

  const typeColors: Record<string, string> = {
    episodic: "bg-accent/20 text-accent",
    semantic: "bg-purple-500/20 text-purple-300",
    preference: "bg-green-500/20 text-green-300",
    project_state: "bg-yellow-500/20 text-yellow-300",
    working_summary: "bg-orange-500/20 text-orange-300",
    procedural_observation: "bg-pink-500/20 text-pink-300",
  };

  const knownTypes = ["episodic", "semantic", "preference", "project_state", "working_summary", "procedural_observation"];
  const extraTypes = [...new Set(units.map(u => u.type).filter(t => !knownTypes.includes(t)))];
  const memoryTypes = ["all", ...knownTypes, ...extraTypes];
  const filtered = filter === "all" ? units : units.filter(u => u.type === filter);

  return (
    <div className="p-6 max-w-6xl mx-auto">
      <div className="mb-6">
        <h1 className="text-2xl font-bold text-foreground">Memory</h1>
        <p className="text-sm text-foreground-secondary mt-1">
          Browse the agent&apos;s long-term memory ({units.length} units)
        </p>
      </div>

      {/* Type filters */}
      <div className="flex flex-wrap gap-2 mb-6">
        {memoryTypes.map((type) => (
          <button
            key={type}
            onClick={() => setFilter(type)}
            className={`px-3 py-1.5 rounded-lg text-xs font-medium transition-colors ${
              filter === type
                ? "bg-accent text-foreground"
                : "bg-background-secondary text-foreground-secondary hover:bg-background-tertiary"
            }`}
          >
            {type === "all" ? "All" : type.replace("_", " ")}
            {type !== "all" && (
              <span className="ml-1 opacity-60">
                ({units.filter(u => u.type === type).length})
              </span>
            )}
          </button>
        ))}
      </div>

      {loading ? (
        <div className="text-foreground-secondary text-center py-12">Loading memory...</div>
      ) : filtered.length === 0 ? (
        <div className="text-center py-16 bg-background-secondary/50 rounded-xl border border-border">
          <div className="text-4xl mb-4">🧠</div>
          <h3 className="text-lg font-medium text-foreground mb-2">No Memories Yet</h3>
          <p className="text-foreground-secondary text-sm">
            The agent stores memories automatically during conversations.
          </p>
        </div>
      ) : (
        <div className="space-y-3">
          {filtered.map((unit) => (
            <div
              key={unit.id}
              className="bg-background-secondary border border-border rounded-lg p-4 hover:border-border-hover transition-colors"
            >
              <div className="flex items-start justify-between gap-4">
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2 mb-2">
                    <span className={`px-2 py-0.5 rounded text-xs font-medium ${typeColors[unit.type] || "bg-background-tertiary text-foreground-secondary"}`}>
                      {unit.type.replace("_", " ")}
                    </span>
                    <span className="text-xs text-foreground-muted">
                      importance: {(unit.importance * 100).toFixed(0)}%
                    </span>
                    {unit.access_count > 0 && (
                      <span className="text-xs text-foreground-muted">
                        accessed {unit.access_count}x
                      </span>
                    )}
                  </div>
                  <p className="text-sm text-foreground leading-relaxed">
                    {unit.summary || unit.content}
                  </p>
                  {unit.entities.length > 0 && (
                    <div className="flex flex-wrap gap-1 mt-2">
                      {unit.entities.map((e, i) => (
                        <span key={i} className="px-1.5 py-0.5 bg-background-tertiary text-foreground-secondary rounded text-xs">
                          {e}
                        </span>
                      ))}
                    </div>
                  )}
                  {unit.topics.length > 0 && (
                    <div className="flex flex-wrap gap-1 mt-1">
                      {unit.topics.map((t, i) => (
                        <span key={i} className="px-1.5 py-0.5 bg-accent/10 text-accent rounded text-xs">
                          #{t}
                        </span>
                      ))}
                    </div>
                  )}
                </div>
                <div className="text-xs text-foreground-muted whitespace-nowrap">
                  {new Date(unit.created_at).toLocaleDateString()}
                </div>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
