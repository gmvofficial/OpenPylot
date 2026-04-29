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
import { Textarea } from "@/components/ui/textarea";
import { Input } from "@/components/ui/input";
import { useToastStore } from "@/stores/toast";
import { apiClient } from "@/lib/api";
import type { SocialPost, SocialCampaign } from "@/types";
import {
  Send,
  Calendar,
  BarChart3,
  PenLine,
  Clock,
  CheckCircle2,
  AlertCircle,
  RefreshCw,
  Twitter,
  Linkedin,
  Facebook,
  Instagram,
  Youtube,
  AtSign,
  Hash,
  Globe,
  Plus,
  Trash2,
  Eye,
} from "lucide-react";

/* -------------------------------------------------------------------------- */
/*  Platform icons                                                            */
/* -------------------------------------------------------------------------- */
const PLATFORM_ICON: Record<string, React.ElementType> = {
  twitter: Twitter,
  linkedin: Linkedin,
  facebook: Facebook,
  instagram: Instagram,
  youtube: Youtube,
  bluesky: AtSign,
  tiktok: Globe,
  reddit: Hash,
  mastodon: Globe,
  threads: AtSign,
  pinterest: Globe,
  medium: PenLine,
  devto: PenLine,
  hashnode: PenLine,
  wordpress: PenLine,
  discord: Hash,
};

const PLATFORM_COLOR: Record<string, string> = {
  twitter: "text-sky-400",
  linkedin: "text-blue-500",
  facebook: "text-accent",
  instagram: "text-pink-400",
  youtube: "text-red-500",
  bluesky: "text-accent",
  tiktok: "text-rose-400",
  reddit: "text-accent-warning",
  mastodon: "text-purple-400",
  threads: "text-foreground-secondary",
};

const PLATFORM_LABEL: Record<string, string> = {
  twitter: "X (Twitter)",
  linkedin: "LinkedIn",
  facebook: "Facebook",
  instagram: "Instagram",
  youtube: "YouTube",
  bluesky: "Bluesky",
  tiktok: "TikTok",
  reddit: "Reddit",
  mastodon: "Mastodon",
  threads: "Threads",
  pinterest: "Pinterest",
  medium: "Medium",
  devto: "Dev.to",
  hashnode: "Hashnode",
  wordpress: "WordPress",
  discord: "Discord",
};

/* -------------------------------------------------------------------------- */
/*  Platform release tiers                                                    */
/*                                                                            */
/*  Tier 1 — Stable, shown by default in v1                                   */
/*  Tier 2 — Shown with a "Beta" badge & setup warning                        */
/*  Tier 3 — Visible only as "Coming Soon" cards (not selectable)             */
/*                                                                            */
/*  Note: Slack and niche dev-only platforms (mastodon, devto, hashnode,      */
/*  medium, wordpress) live under Integrations, not under Social Media.       */
/* -------------------------------------------------------------------------- */
const TIER1_PLATFORMS = ["linkedin", "discord", "reddit"] as const;
const TIER2_PLATFORMS = ["twitter", "facebook", "bluesky"] as const;
const TIER3_PLATFORMS = ["instagram", "tiktok", "youtube", "threads", "pinterest"] as const;

const SUPPORTED_PLATFORMS: string[] = [...TIER1_PLATFORMS, ...TIER2_PLATFORMS];

const TIER2_NOTE: Record<string, string> = {
  twitter: "Requires X API Basic plan ($100/mo) for write access.",
  facebook: "Paste a Page Access Token from Meta Graph API Explorer.",
  bluesky: "New — uses an app password from Bluesky settings.",
};

/* -------------------------------------------------------------------------- */
/*  Compose / Schedule Post                                                   */
/* -------------------------------------------------------------------------- */

function ComposeCard({
  connectedPlatforms,
  onPublish,
}: {
  connectedPlatforms: string[];
  onPublish: (content: string, platforms: string[], scheduledAt?: string) => void;
}) {
  const [content, setContent] = useState("");
  const [selectedPlatforms, setSelectedPlatforms] = useState<string[]>([]);
  const [scheduleDate, setScheduleDate] = useState("");
  const [isScheduled, setIsScheduled] = useState(false);

  const togglePlatform = (p: string) => {
    setSelectedPlatforms((prev) =>
      prev.includes(p) ? prev.filter((x) => x !== p) : [...prev, p]
    );
  };

  const handleSubmit = () => {
    if (!content.trim() || selectedPlatforms.length === 0) return;
    onPublish(content, selectedPlatforms, isScheduled ? scheduleDate : undefined);
    setContent("");
    setSelectedPlatforms([]);
    setScheduleDate("");
    setIsScheduled(false);
  };

  const charCount = content.length;
  const isOverLimit = charCount > 280; // Twitter limit indicator

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-base">
          <PenLine className="h-5 w-5 text-accent" />
          Compose Post
        </CardTitle>
        <CardDescription>Write once, publish everywhere.</CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        {/* Content */}
        <div className="relative">
          <Textarea
            placeholder="What's on your mind?"
            value={content}
            onChange={(e) => setContent(e.target.value)}
            className="min-h-[120px] resize-none"
          />
          <span
            className={`absolute bottom-2 right-3 text-xs ${
              isOverLimit ? "text-accent-error" : "text-foreground-muted"
            }`}
          >
            {charCount}
            {selectedPlatforms.includes("twitter") && " / 280"} 
          </span>
        </div>

        {/* Platform selection */}
        <div className="space-y-2">
          <label className="text-sm font-medium text-foreground">
            Select platforms
          </label>
          <div className="flex flex-wrap gap-2">
            {connectedPlatforms.map((p) => {
              const Icon = PLATFORM_ICON[p] || Globe;
              const isSelected = selectedPlatforms.includes(p);
              const isBeta = (TIER2_PLATFORMS as readonly string[]).includes(p);
              return (
                <button
                  key={p}
                  onClick={() => togglePlatform(p)}
                  title={TIER2_NOTE[p]}
                  className={`flex items-center gap-1.5 rounded-full px-3 py-1.5 text-xs font-medium transition-all border ${
                    isSelected
                      ? "bg-accent/20 border-accent text-accent"
                      : "bg-background-secondary border-border text-foreground-muted hover:border-border-hover"
                  }`}
                >
                  <Icon className="h-3.5 w-3.5" />
                  {PLATFORM_LABEL[p] ?? p}
                  {isBeta && (
                    <span className="ml-1 rounded bg-amber-500/20 px-1 py-0.5 text-[9px] uppercase tracking-wide text-amber-300">
                      beta
                    </span>
                  )}
                </button>
              );
            })}
            {connectedPlatforms.length === 0 && (
              <p className="text-xs text-foreground-muted">
                No platforms connected.{" "}
                <a href="/setup" className="text-accent hover:underline">
                  Connect one →
                </a>
              </p>
            )}
          </div>
        </div>

        {/* Schedule toggle */}
        <div className="flex items-center gap-3">
          <button
            onClick={() => setIsScheduled(!isScheduled)}
            className={`flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs border transition-all ${
              isScheduled
                ? "bg-amber-500/10 border-amber-500/30 text-amber-300"
                : "bg-background-secondary border-border text-foreground-muted"
            }`}
          >
            <Clock className="h-3.5 w-3.5" />
            Schedule
          </button>
          {isScheduled && (
            <Input
              type="datetime-local"
              value={scheduleDate}
              onChange={(e) => setScheduleDate(e.target.value)}
              className="flex-1 text-xs"
            />
          )}
        </div>

        {/* Actions */}
        <div className="flex items-center justify-end gap-2">
          <Button
            onClick={handleSubmit}
            disabled={!content.trim() || selectedPlatforms.length === 0}
          >
            {isScheduled ? (
              <Calendar className="mr-2 h-4 w-4" />
            ) : (
              <Send className="mr-2 h-4 w-4" />
            )}
            {isScheduled ? "Schedule" : "Publish Now"}
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}

/* -------------------------------------------------------------------------- */
/*  Recent Posts                                                              */
/* -------------------------------------------------------------------------- */

function PostCard({ post, onDelete }: { post: SocialPost; onDelete: (id: string) => void }) {
  const Icon = PLATFORM_ICON[post.platform] || Globe;
  const color = PLATFORM_COLOR[post.platform] || "text-foreground-secondary";

  return (
    <Card className="group relative overflow-hidden">
      <div
        className={`absolute left-0 top-0 h-full w-1 ${
          post.status === "published"
            ? "bg-green-500"
            : post.status === "scheduled"
            ? "bg-amber-500"
            : post.status === "failed"
            ? "bg-red-500"
            : "bg-border"
        }`}
      />
      <CardContent className="flex items-start gap-3 py-3 pl-5">
        <div className={`rounded-lg bg-background-secondary p-2 ${color}`}>
          <Icon className="h-4 w-4" />
        </div>
        <div className="flex-1 min-w-0 space-y-1">
          <div className="flex items-center justify-between">
            <Badge
              variant={
                post.status === "published"
                  ? "success"
                  : post.status === "scheduled"
                  ? "secondary"
                  : post.status === "failed"
                  ? "destructive"
                  : "secondary"
              }
              className="text-[10px]"
            >
              {post.status}
            </Badge>
            <span className="text-[10px] text-foreground-muted">
              {post.published_at
                ? new Date(post.published_at).toLocaleDateString()
                : post.scheduled_at
                ? `Scheduled: ${new Date(post.scheduled_at).toLocaleDateString()}`
                : "Draft"}
            </span>
          </div>
          <p className="text-sm text-foreground line-clamp-2">{post.content}</p>
          {post.analytics && (
            <div className="flex items-center gap-3 text-[10px] text-foreground-muted">
              <span>❤️ {post.analytics.likes}</span>
              <span>🔄 {post.analytics.shares}</span>
              <span>💬 {post.analytics.comments}</span>
              <span>👁 {post.analytics.impressions.toLocaleString()}</span>
            </div>
          )}
        </div>
        <button
          onClick={() => onDelete(post.id)}
          className="opacity-0 group-hover:opacity-100 transition-opacity p-1 text-foreground-muted hover:text-accent-error"
        >
          <Trash2 className="h-3.5 w-3.5" />
        </button>
      </CardContent>
    </Card>
  );
}

/* -------------------------------------------------------------------------- */
/*  Social Media Page                                                         */
/* -------------------------------------------------------------------------- */

export default function SocialPage() {
  const [posts, setPosts] = useState<SocialPost[]>([]);
  const [campaigns, setCampaigns] = useState<SocialCampaign[]>([]);
  const [connectedPlatforms, setConnectedPlatforms] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);
  const addToast = useToastStore((s) => s.addToast);

  const loadData = async () => {
    setLoading(true);
    try {
      const [integrations, socialPosts, socialCampaigns] = await Promise.allSettled([
        apiClient.getIntegrations(),
        apiClient.getSocialPosts(),
        apiClient.getSocialCampaigns(),
      ]);

      if (integrations.status === "fulfilled") {
        const connected = integrations.value
          .filter(
            (i) => i.status === "connected" && SUPPORTED_PLATFORMS.includes(i.service)
          )
          .map((i) => i.service);
        setConnectedPlatforms(connected);
      }

      if (socialPosts.status === "fulfilled") {
        setPosts(Array.isArray(socialPosts.value) ? socialPosts.value : []);
      }

      if (socialCampaigns.status === "fulfilled") {
        setCampaigns(Array.isArray(socialCampaigns.value) ? socialCampaigns.value : []);
      }
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadData();
  }, []);

  const handlePublish = async (
    content: string,
    platforms: string[],
    scheduledAt?: string
  ) => {
    try {
      // Create a post for each selected platform via the social API
      const results = await Promise.allSettled(
        platforms.map((platform) =>
          apiClient.createSocialPost({
            platform,
            content,
            hashtags: content.match(/#\w+/g) || undefined,
          })
        )
      );

      const succeeded = results.filter((r) => r.status === "fulfilled").length;
      const failed = results.filter((r) => r.status === "rejected").length;

      if (succeeded > 0) {
        addToast({
          variant: "success",
          title: scheduledAt ? "Post scheduled" : "Post created",
          description: `Created on ${succeeded} platform(s)${failed > 0 ? `, ${failed} failed` : ""}`,
        });
        // Reload posts from server
        loadData();
      } else {
        addToast({
          variant: "error",
          title: "Failed to publish",
          description: "Could not create post on any platform",
        });
      }
    } catch (e) {
      addToast({
        variant: "error",
        title: "Failed to publish",
        description: e instanceof Error ? e.message : "Unknown error",
      });
    }
  };

  const handleDelete = (id: string) => {
    setPosts((prev) => prev.filter((p) => p.id !== id));
  };

  return (
    <div className="mx-auto max-w-4xl space-y-8 p-6">
      {/* Header */}
      <div>
        <h1 className="text-2xl font-bold text-foreground">Social Media</h1>
        <p className="mt-1 text-sm text-foreground-secondary">
          Compose, schedule, and track your social media posts across all platforms.
        </p>
      </div>

      {/* Quick stats */}
      <div className="grid grid-cols-2 gap-4 sm:grid-cols-4">
        <Card className="py-3">
          <CardContent className="flex items-center gap-3">
            <div className="rounded-lg bg-green-500/10 p-2">
              <CheckCircle2 className="h-4 w-4 text-accent-success" />
            </div>
            <div>
              <p className="text-lg font-semibold text-foreground">{connectedPlatforms.length}</p>
              <p className="text-[10px] text-foreground-muted">Connected</p>
            </div>
          </CardContent>
        </Card>
        <Card className="py-3">
          <CardContent className="flex items-center gap-3">
            <div className="rounded-lg bg-accent/10 p-2">
              <Send className="h-4 w-4 text-accent" />
            </div>
            <div>
              <p className="text-lg font-semibold text-foreground">
                {posts.filter((p) => p.status === "published").length}
              </p>
              <p className="text-[10px] text-foreground-muted">Published</p>
            </div>
          </CardContent>
        </Card>
        <Card className="py-3">
          <CardContent className="flex items-center gap-3">
            <div className="rounded-lg bg-amber-500/10 p-2">
              <Clock className="h-4 w-4 text-amber-400" />
            </div>
            <div>
              <p className="text-lg font-semibold text-foreground">
                {posts.filter((p) => p.status === "scheduled").length}
              </p>
              <p className="text-[10px] text-foreground-muted">Scheduled</p>
            </div>
          </CardContent>
        </Card>
        <Card className="py-3">
          <CardContent className="flex items-center gap-3">
            <div className="rounded-lg bg-purple-500/10 p-2">
              <BarChart3 className="h-4 w-4 text-purple-400" />
            </div>
            <div>
              <p className="text-lg font-semibold text-foreground">
                {posts.reduce((sum, p) => sum + (p.analytics?.impressions || 0), 0).toLocaleString()}
              </p>
              <p className="text-[10px] text-foreground-muted">Impressions</p>
            </div>
          </CardContent>
        </Card>
      </div>

      {/* Compose */}
      <ComposeCard connectedPlatforms={connectedPlatforms} onPublish={handlePublish} />

      {/* Recent Posts */}
      <section>
        <h2 className="mb-4 flex items-center gap-2 text-sm font-semibold uppercase tracking-wider text-foreground-muted">
          <Clock className="h-4 w-4" />
          Recent Posts ({posts.length})
        </h2>
        {posts.length === 0 ? (
          <Card className="py-12 text-center">
            <CardContent>
              <PenLine className="mx-auto h-10 w-10 text-foreground-muted" />
              <p className="mt-3 text-sm text-foreground-secondary">
                No posts yet. Compose your first post above!
              </p>
            </CardContent>
          </Card>
        ) : (
          <div className="space-y-3">
            {posts.map((post) => (
              <PostCard key={post.id} post={post} onDelete={handleDelete} />
            ))}
          </div>
        )}
      </section>

      {/* Coming Soon platforms */}
      <section>
        <h2 className="mb-4 flex items-center gap-2 text-sm font-semibold uppercase tracking-wider text-foreground-muted">
          <Plus className="h-4 w-4" />
          Coming Soon
        </h2>
        <p className="mb-3 text-xs text-foreground-muted">
          These platforms require deeper API approvals (Meta Business Verification,
          ByteDance review, Google sensitive scopes). They&apos;ll arrive in a future
          release.
        </p>
        <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 md:grid-cols-5">
          {TIER3_PLATFORMS.map((p) => {
            const Icon = PLATFORM_ICON[p] || Globe;
            const color = PLATFORM_COLOR[p] || "text-foreground-muted";
            return (
              <Card
                key={p}
                className="opacity-60 cursor-not-allowed"
                aria-disabled
              >
                <CardContent className="flex flex-col items-center gap-2 py-4">
                  <Icon className={`h-5 w-5 ${color}`} />
                  <span className="text-xs font-medium text-foreground-secondary">
                    {PLATFORM_LABEL[p] ?? p}
                  </span>
                  <span className="rounded bg-foreground-muted/10 px-1.5 py-0.5 text-[9px] uppercase tracking-wide text-foreground-muted">
                    soon
                  </span>
                </CardContent>
              </Card>
            );
          })}
        </div>
      </section>
    </div>
  );
}
