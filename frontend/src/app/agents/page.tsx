"use client";

import { useEffect, useState, useCallback, useRef } from "react";
import { apiClient } from "@/lib/api";
import { useToastStore } from "@/stores/toast";
import type { AgentPreset } from "@/types";
import { MarkdownRenderer } from "@/components/chat/markdown-renderer";

interface SubAgent {
  id: string;
  name: string;
  agent_type: string;
  status: string;
  started_at?: string;
  completed_at?: string;
  result?: string;
  error?: string;
  interval_secs?: number;
}

interface SubAgentRun {
  id: number;
  sub_agent_id: string;
  run_number: number;
  timestamp: string;
  output_text: string;
}

export default function AgentsPage() {
  const [agents, setAgents] = useState<SubAgent[]>([]);
  const [presets, setPresets] = useState<AgentPreset[]>([]);
  const [presetsUserDir, setPresetsUserDir] = useState<string | undefined>();
  const [loading, setLoading] = useState(true);
  const [showSpawn, setShowSpawn] = useState(false);
  const [spawnName, setSpawnName] = useState("");
  const [spawnTask, setSpawnTask] = useState("");
  const [spawnPreset, setSpawnPreset] = useState<string>("");
  const [spawnInterval, setSpawnInterval] = useState<number | null>(null);
  const [spawnIntervalCustom, setSpawnIntervalCustom] = useState<string>("");
  const [spawnIntervalUnit, setSpawnIntervalUnit] = useState<"minutes" | "hours" | "days">("minutes");
  const [spawning, setSpawning] = useState(false);
  const [selectedAgent, setSelectedAgent] = useState<SubAgent | null>(null);
  const prevStatusMap = useRef<Record<string, string>>({});
  // run-history per agent id, newest-first; persisted server-side in SQLite.
  const [runsByAgent, setRunsByAgent] = useState<Record<string, SubAgentRun[]>>({});

  const fetchAgents = useCallback(async () => {
    try {
      const res = (await apiClient.getSubAgents()) as {
        agents?: SubAgent[];
      };
      const fetched = res?.agents || [];

      // Detect status transitions → toast notifications
      const addToast = useToastStore.getState().addToast;
      for (const agent of fetched) {
        const prev = prevStatusMap.current[agent.id];
        if (prev && prev !== agent.status) {
          if (agent.status === "Completed") {
            addToast({ title: `Agent "${agent.name}" completed`, description: "Results are ready.", variant: "success" });
          } else if (agent.status === "Failed") {
            addToast({ title: `Agent "${agent.name}" failed`, description: agent.error || "Unknown error", variant: "error" });
          } else if (agent.status === "TimedOut") {
            addToast({ title: `Agent "${agent.name}" timed out`, variant: "error" });
          }
        }
        prevStatusMap.current[agent.id] = agent.status;
      }

      setAgents(fetched);
    } catch {
      // ignore
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchAgents();
    const interval = setInterval(fetchAgents, 3000);
    return () => clearInterval(interval);
  }, [fetchAgents]);

  // Load plug-and-play agent presets once
  useEffect(() => {
    apiClient
      .getAgentPresets()
      .then((r) => {
        setPresets(r?.presets ?? []);
        setPresetsUserDir(r?.user_dir);
      })
      .catch(() => {});
  }, []);

  const handlePresetChange = async (name: string) => {
    const prevPreset = presets.find((p) => p.name === spawnPreset);
    setSpawnPreset(name);
    if (!name) return;
    const preset = presets.find((p) => p.name === name);
    if (preset) {
      // Always update name when switching presets (overwrite if name was
      // empty or was set by a previous preset selection).
      if (!spawnName.trim() || (prevPreset && spawnName === prevPreset.name)) {
        setSpawnName(preset.name);
      }
    }
  };

  const handleSpawn = async () => {
    if (!spawnName.trim() || !spawnTask.trim()) return;
    setSpawning(true);
    try {
      const preset = presets.find((p) => p.name === spawnPreset);
      // Resolve interval: quick-pick OR custom — never silently fall through to one-shot
      let intervalSecs: number | undefined;
      if (spawnInterval !== null) {
        intervalSecs = spawnInterval;
      } else if (spawnIntervalCustom !== "") {
        // Custom mode is active — require a valid positive number
        const val = parseFloat(spawnIntervalCustom.trim());
        if (!isNaN(val) && val > 0) {
          const multipliers: Record<string, number> = { minutes: 60, hours: 3600, days: 86400 };
          intervalSecs = Math.round(val * multipliers[spawnIntervalUnit]);
        }
        // If val is invalid / empty, intervalSecs stays undefined → one-shot
      }
      console.log("[spawn] intervalSecs resolved to:", intervalSecs, "| custom:", spawnIntervalCustom, "| unit:", spawnIntervalUnit, "| quickpick:", spawnInterval);
      await apiClient.spawnSubAgent({
        name: spawnName.trim(),
        task: spawnTask.trim(),
        model: preset?.model_override ?? undefined,
        interval_secs: intervalSecs,
      });
      setSpawnName("");
      setSpawnTask("");
      setSpawnPreset("");
      setSpawnInterval(null);
      setSpawnIntervalCustom("");
      setSpawnIntervalUnit("minutes");
      setShowSpawn(false);
      fetchAgents();
    } catch {
      // ignore
    } finally {
      setSpawning(false);
    }
  };

  const cancelAgent = async (id: string) => {
    await apiClient.cancelSubAgent(id);
    fetchAgents();
  };

  // Pull the full run history (DB-backed, persists across page refresh).
  const fetchRuns = useCallback(async (id: string) => {
    try {
      const res = await apiClient.getSubAgentRuns(id);
      setRunsByAgent((prev) => ({ ...prev, [id]: res?.runs ?? [] }));
    } catch {
      // ignore
    }
  }, []);

  // When an agent card is expanded, load its history. Also poll while the agent
  // is Running so new runs appear without a manual refresh.
  useEffect(() => {
    if (!selectedAgent) return;
    fetchRuns(selectedAgent.id);
    if (selectedAgent.status !== "Running") return;
    const t = setInterval(() => fetchRuns(selectedAgent.id), 3000);
    return () => clearInterval(t);
  }, [selectedAgent, fetchRuns]);

  const clearHistory = async (id: string) => {
    if (!confirm("Clear all run history for this sub-agent? The agent itself stays active.")) return;
    await apiClient.clearSubAgentHistory(id);
    setRunsByAgent((prev) => ({ ...prev, [id]: [] }));
  };

  const deleteAgent = async (id: string) => {
    if (!confirm("Delete this sub-agent and ALL of its run history? This cannot be undone.")) return;
    await apiClient.deleteSubAgentPermanent(id);
    setRunsByAgent((prev) => {
      const next = { ...prev };
      delete next[id];
      return next;
    });
    if (selectedAgent?.id === id) setSelectedAgent(null);
    fetchAgents();
  };

  const statusColor = (status: string) => {
    switch (status) {
      case "Running": return "text-accent";
      case "Completed": return "text-accent-success";
      case "Failed": return "text-accent-error";
      case "Cancelled": return "text-foreground-muted";
      case "TimedOut": return "text-accent-warning";
      case "Pending": return "text-accent-warning";
      default: return "text-foreground-secondary";
    }
  };

  const formatInterval = (secs: number) => {
    if (secs < 3600) return `every ${Math.round(secs / 60)} min`;
    if (secs < 86400) return `every ${(secs / 3600) % 1 === 0 ? secs / 3600 : (secs / 3600).toFixed(1)} hr`;
    const days = secs / 86400;
    return `every ${days % 1 === 0 ? days : days.toFixed(1)} day${days !== 1 ? "s" : ""}`;
  };

  const statusIcon = (status: string) => {
    switch (status) {
      case "Running": return "⚡";
      case "Completed": return "✅";
      case "Failed": return "❌";
      case "Cancelled": return "⏹";
      case "TimedOut": return "⏰";
      case "Pending": return "⏳";
      default: return "🤖";
    }
  };

  const summary = {
    total: agents.length,
    running: agents.filter((a) => a.status === "Running").length,
    completed: agents.filter((a) => a.status === "Completed").length,
    failed: agents.filter((a) => a.status === "Failed" || a.status === "TimedOut").length,
  };

  return (
    <div className="p-6 max-w-6xl mx-auto space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold text-foreground">Sub-Agents</h1>
          <p className="text-sm text-foreground-secondary mt-1">
            Spawn, monitor, and manage autonomous sub-agents
          </p>
        </div>
        <div className="flex gap-2">
          <button
            onClick={fetchAgents}
            className="px-4 py-2 bg-background-tertiary hover:bg-background-tertiary text-foreground rounded-lg text-sm transition-colors"
          >
            Refresh
          </button>
          <button
            onClick={() => setShowSpawn(!showSpawn)}
            className="px-4 py-2 bg-accent hover:bg-accent text-foreground rounded-lg text-sm transition-colors font-medium"
          >
            + Spawn Agent
          </button>
        </div>
      </div>

      {/* Summary Cards */}
      {agents.length > 0 && (
        <div className="grid grid-cols-4 gap-3">
          {[
            { label: "Total", value: summary.total, color: "text-foreground" },
            { label: "Running", value: summary.running, color: "text-accent" },
            { label: "Completed", value: summary.completed, color: "text-accent-success" },
            { label: "Failed", value: summary.failed, color: "text-accent-error" },
          ].map((s) => (
            <div key={s.label} className="bg-background-secondary/50 border border-border rounded-lg p-3 text-center">
              <div className={`text-2xl font-bold ${s.color}`}>{s.value}</div>
              <div className="text-xs text-foreground-secondary mt-1">{s.label}</div>
            </div>
          ))}
        </div>
      )}

      {/* Spawn Form */}
      {showSpawn && (
        <div className="bg-background-secondary border border-border rounded-xl p-5 space-y-4">
          <h3 className="text-sm font-semibold text-foreground uppercase tracking-wide">Spawn New Agent</h3>

          {/* Preset picker */}
          {presets.length > 0 && (
            <div>
              <label className="block text-xs text-foreground-secondary mb-2">
                Preset <span className="text-foreground-muted">(optional, plug-and-play)</span>
              </label>
              <div className="flex flex-wrap gap-2">
                <button
                  type="button"
                  onClick={() => handlePresetChange("")}
                  className={`px-3 py-1.5 rounded-lg text-xs border transition-colors ${
                    spawnPreset === ""
                      ? "border-accent bg-accent/10 text-accent"
                      : "border-border-hover bg-background text-foreground-secondary hover:border-border-hover"
                  }`}
                >
                  None
                </button>
                {presets.map((p) => (
                  <button
                    key={p.name}
                    type="button"
                    onClick={() => handlePresetChange(p.name)}
                    title={p.description}
                    className={`px-3 py-1.5 rounded-lg text-xs border transition-colors flex items-center gap-1.5 ${
                      spawnPreset === p.name
                        ? "border-accent bg-accent/10 text-accent"
                        : "border-border-hover bg-background text-foreground-secondary hover:border-border-hover"
                    }`}
                  >
                    <span className="font-medium">{p.name}</span>
                    <span className="text-foreground-muted">· {p.agent_type}</span>
                    {p.source !== "bundled" && (
                      <span className="text-[10px] text-amber-400 uppercase">{p.source}</span>
                    )}
                  </button>
                ))}
              </div>
              {spawnPreset && (
                <p className="mt-2 text-xs text-foreground-secondary">
                  {presets.find((p) => p.name === spawnPreset)?.description}
                </p>
              )}
              {presetsUserDir && (
                <p className="mt-1 text-[11px] text-foreground-muted">
                  Drop custom <code className="bg-background px-1 rounded">.toml</code> manifests in{" "}
                  <code className="bg-background px-1 rounded">{presetsUserDir}</code>
                </p>
              )}
            </div>
          )}

          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div>
              <label className="block text-xs text-foreground-secondary mb-1">Agent Name</label>
              <input
                type="text"
                value={spawnName}
                onChange={(e) => setSpawnName(e.target.value)}
                placeholder="e.g. researcher, code-reviewer"
                className="w-full bg-background border border-border-hover rounded-lg px-3 py-2 text-sm text-foreground placeholder:text-foreground-muted focus:outline-none focus:border-accent"
              />
            </div>
            <div className="flex items-end">
              <button
                onClick={handleSpawn}
                disabled={spawning || !spawnName.trim() || !spawnTask.trim()}
                className="px-6 py-2 bg-accent hover:bg-accent disabled:opacity-40 disabled:cursor-not-allowed text-foreground rounded-lg text-sm font-medium transition-colors"
              >
                {spawning ? "Spawning..." : "Spawn"}
              </button>
            </div>
          </div>
          <div>
            <label className="block text-xs text-foreground-secondary mb-1">Task Description</label>
            <textarea
              value={spawnTask}
              onChange={(e) => setSpawnTask(e.target.value)}
              placeholder="Describe what the sub-agent should do..."
              rows={3}
              className="w-full bg-background border border-border-hover rounded-lg px-3 py-2 text-sm text-foreground placeholder:text-foreground-muted focus:outline-none focus:border-accent resize-none"
            />
          </div>

          {/* ── Schedule / Recurrence ─────────────────────────────── */}
          <div>
            <label className="block text-xs text-foreground-secondary mb-2">
              Run Schedule <span className="text-foreground-muted">(how often should the agent run?)</span>
            </label>
            <div className="flex flex-wrap gap-2 mb-3">
              {[
                { label: "Once", value: null },
                { label: "Every 2 min", value: 120 },
                { label: "Every 5 min", value: 300 },
                { label: "Every 30 min", value: 1800 },
                { label: "Every 1 hr", value: 3600 },
                { label: "Every 6 hr", value: 21600 },
                { label: "Every 12 hr", value: 43200 },
                { label: "Every day", value: 86400 },
              ].map((opt) => (
                <button
                  key={opt.label}
                  type="button"
                  onClick={() => { setSpawnInterval(opt.value); setSpawnIntervalCustom(""); }}
                  className={`px-3 py-1.5 rounded-lg text-xs border transition-colors ${
                    spawnInterval === opt.value && !spawnIntervalCustom
                      ? "border-accent bg-accent/10 text-accent"
                      : "border-border-hover bg-background text-foreground-secondary hover:border-border-hover"
                  }`}
                >
                  {opt.label}
                </button>
              ))}
              <button
                type="button"
                onClick={() => { setSpawnInterval(null); setSpawnIntervalCustom(" "); }}
                className={`px-3 py-1.5 rounded-lg text-xs border transition-colors ${
                  spawnIntervalCustom !== ""
                    ? "border-accent bg-accent/10 text-accent"
                    : "border-border-hover bg-background text-foreground-secondary hover:border-border-hover"
                }`}
              >
                Custom
              </button>
            </div>
            {spawnIntervalCustom !== "" && (
              <div className="space-y-2">
                <div className="flex items-center gap-2">
                  <span className="text-xs text-foreground-secondary">Every</span>
                  <input
                    type="number"
                    min="1"
                    value={spawnIntervalCustom.trim()}
                    onChange={(e) => setSpawnIntervalCustom(e.target.value)}
                    placeholder="2"
                    className="w-20 bg-background border border-border-hover rounded-lg px-2 py-1.5 text-sm text-foreground placeholder:text-foreground-muted focus:outline-none focus:border-accent"
                    autoFocus
                  />
                  <select
                    value={spawnIntervalUnit}
                    onChange={(e) => setSpawnIntervalUnit(e.target.value as "minutes" | "hours" | "days")}
                    className="bg-background border border-border-hover rounded-lg px-2 py-1.5 text-sm text-foreground focus:outline-none focus:border-accent"
                  >
                    <option value="minutes">minutes</option>
                    <option value="hours">hours</option>
                    <option value="days">days</option>
                  </select>
                </div>
                {/* Always show the computed seconds so user can verify */}
                {(() => {
                  const raw = spawnIntervalCustom.trim();
                  const val = parseFloat(raw);
                  if (!raw || isNaN(val) || val <= 0) {
                    return (
                      <p className="text-xs text-accent-error">Enter a number greater than 0.</p>
                    );
                  }
                  const multipliers: Record<string, number> = { minutes: 60, hours: 3600, days: 86400 };
                  const secs = Math.round(val * multipliers[spawnIntervalUnit]);
                  const human = secs < 3600
                    ? `${Math.round(secs / 60)} minute(s)`
                    : secs < 86400
                    ? `${(secs / 3600).toFixed(1)} hour(s)`
                    : `${(secs / 86400).toFixed(1)} day(s)`;
                  return (
                    <p className="text-xs text-accent">
                      ✓ Agent will run every <strong>{human}</strong> ({secs}s)
                    </p>
                  );
                })()}
              </div>
            )}
          </div>
        </div>
      )}

      {/* Agent List */}
      {loading ? (
        <div className="text-foreground-secondary text-center py-12">Loading agents...</div>
      ) : agents.length === 0 ? (
        <div className="text-center py-16 bg-background-secondary/50 rounded-xl border border-border">
          <div className="text-4xl mb-4">🤖</div>
          <h3 className="text-lg font-medium text-foreground mb-2">No Sub-Agents Yet</h3>
          <p className="text-foreground-secondary text-sm mb-4">
            Spawn your first sub-agent using the button above, or ask in chat:
          </p>
          <code className="text-xs bg-background text-foreground-secondary px-3 py-1.5 rounded">
            &quot;Spawn a sub-agent named researcher to find the latest AI news&quot;
          </code>
        </div>
      ) : (
        <div className="space-y-3">
          {agents.map((agent) => (
            <div
              key={agent.id}
              onClick={() => setSelectedAgent(selectedAgent?.id === agent.id ? null : agent)}
              className={`bg-background-secondary border rounded-lg p-4 transition-colors cursor-pointer ${
                selectedAgent?.id === agent.id
                  ? "border-accent"
                  : "border-border hover:border-border-hover"
              }`}
            >
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-3">
                  <div className="text-xl">{statusIcon(agent.status)}</div>
                  <div>
                    <h3 className="font-medium text-foreground">{agent.name}</h3>
                    <div className="flex items-center gap-2 text-xs text-foreground-secondary mt-1">
                      <span className={statusColor(agent.status)}>{agent.status}</span>
                      {agent.agent_type && (
                        <>
                          <span>·</span>
                          <span>{agent.agent_type}</span>
                        </>
                      )}
                      {agent.interval_secs ? (
                        <>
                          <span>·</span>
                          <span className="text-accent-warning">🔁 {formatInterval(agent.interval_secs)}</span>
                        </>
                      ) : null}
                      {agent.started_at && (
                        <>
                          <span>·</span>
                          <span>{new Date(agent.started_at).toLocaleString()}</span>
                        </>
                      )}
                    </div>
                  </div>
                </div>
                <div className="flex items-center gap-2">
                  <span className="text-xs text-foreground-muted font-mono">{agent.id.slice(0, 8)}</span>
                  {(agent.status === "Running" || agent.status === "Pending") && (
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        cancelAgent(agent.id);
                      }}
                      className="px-3 py-1.5 bg-accent-warning/20 hover:bg-accent-warning/40 text-accent-warning rounded text-xs transition-colors"
                    >
                      Cancel
                    </button>
                  )}
                  {agent.status !== "Running" && agent.status !== "Pending" && (
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        deleteAgent(agent.id);
                      }}
                      className="px-3 py-1.5 bg-accent-error/20 hover:bg-accent-error/40 text-accent-error rounded text-xs transition-colors"
                    >
                      Delete
                    </button>
                  )}
                </div>
              </div>

              {/* Expanded Detail */}
              {selectedAgent?.id === agent.id && (
                <div className="mt-4 pt-4 border-t border-border space-y-3">
                  <div className="grid grid-cols-2 gap-4 text-xs">
                    <div>
                      <span className="text-foreground-muted">ID</span>
                      <p className="text-foreground-secondary font-mono mt-0.5">{agent.id}</p>
                    </div>
                    <div>
                      <span className="text-foreground-muted">Status</span>
                      <p className={`mt-0.5 font-medium ${statusColor(agent.status)}`}>{agent.status}</p>
                    </div>
                    {agent.started_at && (
                      <div>
                        <span className="text-foreground-muted">Started</span>
                        <p className="text-foreground-secondary mt-0.5">{new Date(agent.started_at).toLocaleString()}</p>
                      </div>
                    )}
                    {agent.completed_at && (
                      <div>
                        <span className="text-foreground-muted">Completed</span>
                        <p className="text-foreground-secondary mt-0.5">{new Date(agent.completed_at).toLocaleString()}</p>
                      </div>
                    )}
                  </div>
                  {agent.error && (
                    <div>
                      <span className="text-xs text-foreground-muted">Error</span>
                      <div className="mt-1 p-3 bg-red-900/20 border border-red-800/50 rounded text-sm text-red-300">
                        {agent.error}
                      </div>
                    </div>
                  )}

                  {/* Run history — appended, never overwritten. Newest first. */}
                  <div onClick={(e) => e.stopPropagation()}>
                    <div className="flex items-center justify-between mb-2">
                      <span className="text-xs text-foreground-muted">
                        Run History {runsByAgent[agent.id] ? `(${runsByAgent[agent.id].length})` : ""}
                      </span>
                      {runsByAgent[agent.id]?.length > 0 && (
                        <button
                          onClick={() => clearHistory(agent.id)}
                          className="px-2 py-1 text-[11px] bg-background-tertiary hover:bg-accent-warning/20 hover:text-accent-warning text-foreground-secondary rounded transition-colors"
                        >
                          Clear History
                        </button>
                      )}
                    </div>
                    {runsByAgent[agent.id]?.length ? (
                      <div className="space-y-2 max-h-96 overflow-y-auto">
                        {runsByAgent[agent.id].map((run) => (
                          <div
                            key={run.id}
                            className="border border-border rounded-lg bg-background"
                          >
                            <div className="flex items-center justify-between px-3 py-2 border-b border-border text-xs">
                              <span className="font-medium text-foreground">Run #{run.run_number}</span>
                              <span className="text-foreground-muted">
                                {new Date(run.timestamp).toLocaleString()}
                              </span>
                            </div>
                            <div className="p-3 text-sm text-foreground-secondary max-h-64 overflow-y-auto">
                              <MarkdownRenderer content={run.output_text} />
                            </div>
                          </div>
                        ))}
                      </div>
                    ) : (
                      <div className="text-xs text-foreground-muted italic px-1">
                        No completed runs yet.
                      </div>
                    )}
                  </div>
                </div>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
