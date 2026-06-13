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
import { Skeleton } from "@/components/ui/skeleton";
import { Separator } from "@/components/ui/separator";
import type { AgentStatus, ScheduledJob, LogEntry, Skill, ToolDefinition, LearningRule } from "@/types";
import { apiClient } from "@/lib/api";
import { formatRelativeTime } from "@/lib/utils";

/** Convert a cron expression into a human-readable description. Falls back to the raw expression for exotic patterns. */
function describeCron(expr: string): string {
  const raw = expr.trim();
  const parts = raw.split(/\s+/);
  // Not a standard 5-part cron — return as-is (may already be human-readable)
  if (parts.length !== 5) return raw;

  const [min, hour, dom, month, dow] = parts;

  const star = (v: string) => v === "*";
  const everyN = (v: string) => /^\*\/(\d+)$/.exec(v);
  const isNum = (v: string) => /^\d+$/.test(v);

  // * * * * *  →  Every minute
  if (star(min) && star(hour) && star(dom) && star(month) && star(dow)) {
    return "Every minute";
  }

  // */N * * * *  →  Every N minutes
  const minEvery = everyN(min);
  if (minEvery && star(hour) && star(dom) && star(month) && star(dow)) {
    const n = parseInt(minEvery[1]);
    return n === 1 ? "Every minute" : `Every ${n} minutes`;
  }

  // 0 * * * *  →  Every hour
  if (min === "0" && star(hour) && star(dom) && star(month) && star(dow)) {
    return "Every hour";
  }

  // 0 */N * * *  →  Every N hours
  const hourEvery = everyN(hour);
  if (min === "0" && hourEvery && star(dom) && star(month) && star(dow)) {
    const n = parseInt(hourEvery[1]);
    return n === 1 ? "Every hour" : `Every ${n} hours`;
  }

  // Helper: format a single hour number as "HH:MM"
  const fmt = (h: string, m: string) => {
    const isMid = h === "0" && m === "0";
    if (isMid) return "midnight";
    return `${h.padStart(2, "0")}:${m.padStart(2, "0")}`;
  };

  // Comma-separated hours on the dot: 0 H,H,H * * *  →  Daily at HH:MM, HH:MM, HH:MM
  if (isNum(min) && hour.includes(",") && star(dom) && star(month) && star(dow)) {
    const times = hour.split(",").map((h) => fmt(h.trim(), min)).join(", ");
    return `Daily at ${times}`;
  }

  // Fixed hour + fixed minute patterns
  if (isNum(min) && isNum(hour)) {
    const timeLabel = fmt(hour, min);

    // Every day at HH:MM
    if (star(dom) && star(month) && star(dow)) {
      return hour === "0" && min === "0" ? "Daily at midnight" : `Daily at ${timeLabel}`;
    }

    // Day-of-week: 0 H * * D
    if (star(dom) && star(month) && /^\d$/.test(dow)) {
      const days = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
      const dayName = days[parseInt(dow)] ?? `weekday ${dow}`;
      return `Every ${dayName} at ${timeLabel}`;
    }

    // Day-of-month: 0 H d * *
    if (isNum(dom) && star(month) && star(dow)) {
      return `Monthly on day ${dom} at ${timeLabel}`;
    }
  }

  return raw; // unknown pattern — show raw cron
}
import {
  Activity,
  Bot,
  Calendar,
  CheckCircle2,
  Clock,
  AlertTriangle,
  XCircle,
  Cpu,
  HardDrive,
  Wifi,
  RefreshCw,
  ChevronRight,
  Play,
  Pause,
  FileText,
  Wrench,
  Zap,
  BrainCircuit,
} from "lucide-react";

/* -------------------------------------------------------------------------- */
/*  Status Cards                                                              */
/* -------------------------------------------------------------------------- */

function StatusCards({ status, loading }: { status: AgentStatus | null; loading: boolean }) {
  if (loading) {
    return (
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        {Array.from({ length: 4 }).map((_, i) => (
          <Card key={i}>
            <CardContent className="flex items-center gap-4 py-4">
              <Skeleton className="h-10 w-10 rounded-lg" />
              <div className="flex-1 space-y-1.5">
                <Skeleton className="h-3 w-20" />
                <Skeleton className="h-5 w-12" />
              </div>
            </CardContent>
          </Card>
        ))}
      </div>
    );
  }

  const cards = [
    {
      label: "Status",
      value: status?.status ?? "unknown",
      icon: Activity,
      color: status?.status === "running" ? "text-accent-success" : "text-amber-400",
    },
    {
      label: "Uptime",
      value: status?.uptime ?? "—",
      icon: Clock,
      color: "text-accent",
    },
    {
      label: "Model",
      value: status?.model ?? "—",
      icon: Cpu,
      color: "text-purple-400",
    },
    {
      label: "Integrations",
      value: `${status?.active_integrations ?? 0} active`,
      icon: Wifi,
      color: "text-teal-400",
    },
  ];

  return (
    <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
      {cards.map((c) => {
        const Icon = c.icon;
        return (
          <Card key={c.label}>
            <CardContent className="flex items-center gap-4 py-4">
              <div className={`rounded-lg bg-background-secondary p-2.5 ${c.color}`}>
                <Icon className="h-5 w-5" />
              </div>
              <div>
                <p className="text-xs font-medium uppercase tracking-wider text-foreground-muted">
                  {c.label}
                </p>
                <p className="text-lg font-semibold capitalize text-foreground">{c.value}</p>
              </div>
            </CardContent>
          </Card>
        );
      })}
    </div>
  );
}

/* -------------------------------------------------------------------------- */
/*  Jobs Table                                                                */
/* -------------------------------------------------------------------------- */

function JobsTable({
  jobs,
  loading,
  onToggle,
  onRun,
}: {
  jobs: ScheduledJob[];
  loading: boolean;
  onToggle: (id: string) => void;
  onRun: (id: string) => void;
}) {
  if (loading) {
    return (
      <div className="space-y-3">
        {Array.from({ length: 3 }).map((_, i) => (
          <Skeleton key={i} className="h-14 w-full rounded-lg" />
        ))}
      </div>
    );
  }

  if (jobs.length === 0) {
    return (
      <div className="py-8 text-center text-sm text-foreground-muted">
        <Calendar className="mx-auto mb-2 h-8 w-8" />
        No scheduled jobs
      </div>
    );
  }

  return (
    <div className="overflow-hidden rounded-lg border border-border">
      <table className="w-full text-sm">
        <thead>
          <tr className="border-b border-border bg-background-secondary/50">
            <th className="px-4 py-2.5 text-left font-medium text-foreground-muted">Job</th>
            <th className="px-4 py-2.5 text-left font-medium text-foreground-muted">Schedule</th>
            <th className="px-4 py-2.5 text-left font-medium text-foreground-muted">Last Run</th>
            <th className="px-4 py-2.5 text-left font-medium text-foreground-muted">Status</th>
            <th className="px-4 py-2.5 text-right font-medium text-foreground-muted">Actions</th>
          </tr>
        </thead>
        <tbody className="divide-y divide-border">
          {jobs.map((job) => (
            <tr key={job.id} className="hover:bg-background-secondary/30 transition-colors">
              <td className="px-4 py-3 font-medium text-foreground">{job.name}</td>
              <td className="px-4 py-3 text-sm text-foreground-secondary" title={job.schedule}>{describeCron(job.schedule)}</td>
              <td className="px-4 py-3 text-foreground-secondary">
                {job.last_run ? formatRelativeTime(new Date(job.last_run)) : "Never"}
              </td>
              <td className="px-4 py-3">
                <Badge variant={job.enabled ? "success" : "secondary"}>
                  {job.enabled ? "Active" : "Paused"}
                </Badge>
              </td>
              <td className="px-4 py-3 text-right">
                <div className="flex items-center justify-end gap-1">
                  <Button size="icon" variant="ghost" onClick={() => onToggle(job.id)}>
                    {job.enabled ? <Pause className="h-4 w-4" /> : <Play className="h-4 w-4" />}
                  </Button>
                  <Button size="icon" variant="ghost" onClick={() => onRun(job.id)}>
                    <RefreshCw className="h-4 w-4" />
                  </Button>
                </div>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/* -------------------------------------------------------------------------- */
/*  Logs Viewer                                                               */
/* -------------------------------------------------------------------------- */

function LogsViewer({ logs, loading }: { logs: LogEntry[]; loading: boolean }) {
  const levelIcon = (level: string) => {
    switch (level) {
      case "error":
        return <XCircle className="h-3.5 w-3.5 text-accent-error" />;
      case "warn":
        return <AlertTriangle className="h-3.5 w-3.5 text-amber-400" />;
      case "info":
        return <CheckCircle2 className="h-3.5 w-3.5 text-accent" />;
      default:
        return <FileText className="h-3.5 w-3.5 text-foreground-muted" />;
    }
  };

  if (loading) {
    return (
      <div className="space-y-2">
        {Array.from({ length: 5 }).map((_, i) => (
          <Skeleton key={i} className="h-6 w-full" />
        ))}
      </div>
    );
  }

  if (logs.length === 0) {
    return (
      <div className="py-8 text-center text-sm text-foreground-muted">
        <FileText className="mx-auto mb-2 h-8 w-8" />
        No recent logs
      </div>
    );
  }

  return (
    <div className="max-h-80 overflow-y-auto rounded-lg border border-border bg-background font-mono text-xs">
      {logs.map((log) => (
        <div key={log.id} className="flex items-start gap-2 border-b border-border/50 px-3 py-1.5 hover:bg-background-secondary/30">
          <span className="mt-0.5 shrink-0">{levelIcon(log.level)}</span>
          <span className="shrink-0 text-foreground-muted">
            {new Date(log.timestamp).toLocaleTimeString()}
          </span>
          <span className="text-foreground-secondary">{log.message}</span>
        </div>
      ))}
    </div>
  );
}

/* -------------------------------------------------------------------------- */
/*  Dashboard Page                                                            */
/* -------------------------------------------------------------------------- */

export default function DashboardPage() {
  const [status, setStatus] = useState<AgentStatus | null>(null);
  const [jobs, setJobs] = useState<ScheduledJob[]>([]);
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [tools, setTools] = useState<ToolDefinition[]>([]);
  const [skills, setSkills] = useState<Skill[]>([]);
  const [rules, setRules] = useState<LearningRule[]>([]);
  const [loadingStatus, setLoadingStatus] = useState(true);
  const [loadingJobs, setLoadingJobs] = useState(true);
  const [loadingLogs, setLoadingLogs] = useState(true);

  const loadAll = async () => {
    setLoadingStatus(true);
    setLoadingJobs(true);
    setLoadingLogs(true);

    try {
      const [s, j, l, t, sk, lr] = await Promise.allSettled([
        apiClient.getStatus(),
        apiClient.getJobs(),
        apiClient.getLogs({ limit: 50 }),
        apiClient.getTools(),
        apiClient.getSkills(),
        apiClient.getLearningRules(),
      ]);
      if (s.status === "fulfilled") setStatus(s.value);
      if (j.status === "fulfilled") setJobs(j.value);
      if (l.status === "fulfilled") setLogs(l.value);
      if (t.status === "fulfilled") setTools(t.value);
      if (sk.status === "fulfilled") setSkills(Array.isArray(sk.value) ? sk.value : []);
      if (lr.status === "fulfilled") setRules(Array.isArray(lr.value) ? lr.value : []);
    } finally {
      setLoadingStatus(false);
      setLoadingJobs(false);
      setLoadingLogs(false);
    }
  };

  useEffect(() => {
    loadAll();
  }, []);

  const handleToggleJob = async (id: string) => {
    const job = jobs.find((j) => j.id === id);
    if (!job) return;
    await apiClient.updateJob(id, { enabled: !job.enabled });
    setJobs((prev) => prev.map((j) => (j.id === id ? { ...j, enabled: !j.enabled } : j)));
  };

  const handleRunJob = async (id: string) => {
    await apiClient.runJob(id);
    // reload to reflect last_run update
    const updatedJobs = await apiClient.getJobs();
    setJobs(updatedJobs);
  };

  return (
    <div className="mx-auto max-w-6xl space-y-8 p-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold text-foreground">Dashboard</h1>
          <p className="mt-1 text-sm text-foreground-secondary">
            Overview of your agent&apos;s status, jobs, and recent activity.
          </p>
        </div>
        <Button variant="outline" size="sm" onClick={loadAll}>
          <RefreshCw className="mr-2 h-4 w-4" />
          Refresh
        </Button>
      </div>

      {/* Status cards */}
      <StatusCards status={status} loading={loadingStatus} />

      {/* Capabilities summary */}
      <div className="grid gap-4 sm:grid-cols-3">
        <Card>
          <CardContent className="flex items-center gap-4 py-4">
            <div className="rounded-lg bg-accent/10 p-2.5">
              <Wrench className="h-5 w-5 text-accent" />
            </div>
            <div>
              <p className="text-xs font-medium uppercase tracking-wider text-foreground-muted">Tools</p>
              <p className="text-lg font-semibold text-foreground">{tools.length}</p>
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="flex items-center gap-4 py-4">
            <div className="rounded-lg bg-purple-500/10 p-2.5">
              <Zap className="h-5 w-5 text-purple-400" />
            </div>
            <div>
              <p className="text-xs font-medium uppercase tracking-wider text-foreground-muted">Skills</p>
              <p className="text-lg font-semibold text-foreground">{skills.length}</p>
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="flex items-center gap-4 py-4">
            <div className="rounded-lg bg-green-500/10 p-2.5">
              <BrainCircuit className="h-5 w-5 text-accent-success" />
            </div>
            <div>
              <p className="text-xs font-medium uppercase tracking-wider text-foreground-muted">Learned Rules</p>
              <p className="text-lg font-semibold text-foreground">{rules.length}</p>
            </div>
          </CardContent>
        </Card>
      </div>

      <Separator />

      {/* Jobs & Logs side by side */}
      <div className="grid gap-6 lg:grid-cols-2">
        {/* Jobs */}
        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0">
            <div>
              <CardTitle className="text-lg">Scheduled Jobs</CardTitle>
              <CardDescription>Automated tasks that run on a schedule</CardDescription>
            </div>
            <Badge variant="outline">{jobs.length} jobs</Badge>
          </CardHeader>
          <CardContent>
            <JobsTable
              jobs={jobs}
              loading={loadingJobs}
              onToggle={handleToggleJob}
              onRun={handleRunJob}
            />
          </CardContent>
        </Card>

        {/* Logs */}
        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0">
            <div>
              <CardTitle className="text-lg">Recent Logs</CardTitle>
              <CardDescription>Latest agent activity and events</CardDescription>
            </div>
            <Badge variant="outline">{logs.length} entries</Badge>
          </CardHeader>
          <CardContent>
            <LogsViewer logs={logs} loading={loadingLogs} />
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
