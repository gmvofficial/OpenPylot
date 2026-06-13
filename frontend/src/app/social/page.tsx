"use client";

import { useEffect, useRef, useState } from "react";
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
import { stripMarkdown, linkedinPostUrl } from "@/lib/social-text";
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
  Sparkles,
  Check,
  X,
  ExternalLink,
  Image as ImageIcon,
  FileText,
  Type,
  Upload,
  Search,
  ChevronDown,
  ChevronUp,
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
  publishing,
}: {
  connectedPlatforms: string[];
  onPublish: (
    content: string,
    platforms: string[],
    scheduledAt?: string,
    media?: { kind: "image" | "document"; urls: string[]; title?: string }
  ) => Promise<void> | void;
  publishing: boolean;
}) {
  const [content, setContent] = useState("");
  const [selectedPlatforms, setSelectedPlatforms] = useState<string[]>([]);
  const [scheduleDate, setScheduleDate] = useState("");
  const [isScheduled, setIsScheduled] = useState(false);
  const addToast = useToastStore((s) => s.addToast);

  // ── AI improver state ──────────────────────────────────────────
  const [improving, setImproving] = useState(false);
  const [suggestion, setSuggestion] = useState<string | null>(null);
  const [editedSuggestion, setEditedSuggestion] = useState("");

  // ── Media attachment state ────────────────────────────────────
  // Single source of truth: either null (text-only post) or a populated object.
  const [attachedMedia, setAttachedMedia] = useState<{
    kind: "image" | "document";
    url: string;
    name: string;
    title: string;
  } | null>(null);
  const [uploading, setUploading] = useState(false);
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  // Keep legacy aliases so unchanged call-sites compile without edits.
  const mediaKind: "text" | "image" | "document" = attachedMedia?.kind ?? "text";
  const mediaUrl = attachedMedia?.url ?? "";
  const mediaTitle = attachedMedia?.title ?? "";
  const setMediaKind = (k: "text" | "image" | "document") => {
    if (k === "text") setAttachedMedia(null);
    // Changing kind while a file is attached: keep URL, swap kind.
    else setAttachedMedia((prev) => prev ? { ...prev, kind: k } : null);
  };
  const setMediaUrl = (u: string) =>
    setAttachedMedia((prev) => prev ? { ...prev, url: u } : null);
  const setMediaTitle = (t: string) =>
    setAttachedMedia((prev) => prev ? { ...prev, title: t } : null);

  // Open the native file picker, validate the file type against the chosen
  // media kind, upload to /api/social/upload, then prefill the URL field
  // with the public path the backend returns.
  const handleFilePick = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    // Reset so picking the same file twice still triggers onChange.
    if (e.target) e.target.value = "";
    if (!file) return;

    const isImage = /^image\/(jpeg|png|gif|webp)$/.test(file.type)
      // fallback: some browsers leave type empty — use extension
      || /\.(jpe?g|png|gif|webp)$/i.test(file.name);
    const isPdf = file.type === "application/pdf" || /\.pdf$/i.test(file.name);

    if (!isImage && !isPdf) {
      addToast({
        variant: "warning",
        title: "Unsupported file",
        description: "Pick a JPG, PNG, GIF, WebP image, or a PDF document.",
      });
      return;
    }
    if (file.size > 25 * 1024 * 1024) {
      addToast({
        variant: "warning",
        title: "File too large",
        description: "Max 25 MB.",
      });
      return;
    }

    const kind: "image" | "document" = isImage ? "image" : "document";
    setUploading(true);
    try {
      const res = await apiClient.uploadSocialMedia(file);
      if (res?.error || !res?.url) {
        addToast({
          variant: "error",
          title: "Upload failed",
          description: res?.error || "The server didn't return a URL. Please try again.",
        });
      } else {
        // Set the ENTIRE media state in one atomic update so kind + url are
        // always in sync — this eliminates the "text post with no URLs" bug.
        setAttachedMedia({
          kind,
          url: res.url,
          name: res.original_name || file.name,
          title: "",
        });
        addToast({
          variant: "success",
          title: kind === "image" ? "✅ Image attached!" : "✅ PDF attached!",
          description: `${res.original_name || file.name} (${Math.round((res.size_bytes || file.size) / 1024)} KB) — ready to post`,
        });
      }
    } catch (err) {
      addToast({
        variant: "error",
        title: "Upload failed",
        description: err instanceof Error ? err.message : "Network error",
      });
    } finally {
      setUploading(false);
    }
  };

  const togglePlatform = (p: string) => {
    setSelectedPlatforms((prev) =>
      prev.includes(p) ? prev.filter((x) => x !== p) : [...prev, p]
    );
  };

  const handleSubmit = async () => {
    // Defensive: surface the *real* reason the click did nothing instead
    // of failing silently like the previous build did.
    if (publishing) return;
    if (!content.trim()) {
      addToast({
        variant: "warning",
        title: "Empty post",
        description: "Write something before publishing.",
      });
      return;
    }
    if (selectedPlatforms.length === 0) {
      addToast({
        variant: "warning",
        title: "Pick a platform",
        description:
          connectedPlatforms.length === 0
            ? "No platforms connected. Connect one in Setup → Integrations first."
            : "Select at least one platform to publish to.",
      });
      return;
    }

    // Build media payload directly from attachedMedia state (single source
    // of truth) — no more effectiveKind inference dance.
    let mediaPayload:
      | { kind: "image" | "document"; urls: string[]; title?: string }
      | undefined;

    if (attachedMedia) {
      const { kind, url, title } = attachedMedia;
      if (!url || !/^https?:\/\//i.test(url)) {
        addToast({
          variant: "warning",
          title: "Media URL missing",
          description: "The uploaded file URL is invalid. Please re-attach the file.",
        });
        return;
      }
      if (kind === "document" && !title.trim() && selectedPlatforms.includes("linkedin")) {
        addToast({
          variant: "warning",
          title: "Document title required",
          description: "LinkedIn document posts need a title for the carousel.",
        });
        return;
      }
      // Warn if attached media won't be sent to some selected platforms.
      // LinkedIn: image + PDF. Facebook: image only. Others: text only.
      const mediaUnsupportedPlatforms = selectedPlatforms.filter((p) => {
        if (p === "linkedin") return false;
        if (p === "facebook" && kind === "image") return false;
        return true;
      });
      if (mediaUnsupportedPlatforms.length > 0) {
        addToast({
          variant: "warning",
          title: kind === "document" ? "PDF not supported on some platforms" : "Image not supported on some platforms",
          description: `${mediaUnsupportedPlatforms.join(", ")} will receive text only.`,
        });
      }
      mediaPayload = {
        kind,
        urls: [url],
        title: kind === "document" ? title.trim() : undefined,
      };
    }

    // Strip any markdown the user pasted (LinkedIn/X render text literally).
    const clean = stripMarkdown(content);
    await onPublish(clean, selectedPlatforms, isScheduled ? scheduleDate : undefined, mediaPayload);

    // Reset form on success path. (onPublish handles its own toasts.)
    setContent("");
    setSelectedPlatforms([]);
    setScheduleDate("");
    setIsScheduled(false);
    setSuggestion(null);
    setEditedSuggestion("");
    setAttachedMedia(null);
  };

  const handleImprove = async () => {
    if (!content.trim() || improving) return;
    setImproving(true);
    setSuggestion(null);
    try {
      // Use the first selected platform to tune the prompt; default to LinkedIn.
      const platform = selectedPlatforms[0] || "linkedin";
      const res = await apiClient.improveSocialPost(content, platform);
      if (res?.error || !res?.improved) {
        addToast({
          variant: "error",
          title: "Couldn't get suggestions",
          description:
            res?.error ||
            "The AI editor didn't return any text. Please try again.",
        });
      } else {
        setSuggestion(res.improved);
        setEditedSuggestion(res.improved);
      }
    } catch (e) {
      addToast({
        variant: "error",
        title: "Couldn't get suggestions",
        description: e instanceof Error ? e.message : "Please try again.",
      });
    } finally {
      setImproving(false);
    }
  };

  const acceptSuggestion = () => {
    setContent(editedSuggestion);
    setSuggestion(null);
    addToast({
      variant: "success",
      title: "Draft updated",
      description: "AI suggestion applied. Review then publish.",
    });
  };

  const dismissSuggestion = () => {
    setSuggestion(null);
    setEditedSuggestion("");
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
            disabled={publishing}
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

        {/* AI Improver button row */}
        <div className="flex items-center justify-between">
          <Button
            type="button"
            size="sm"
            variant="ghost"
            onClick={handleImprove}
            disabled={!content.trim() || improving || publishing}
            className="text-purple-300 hover:text-purple-200"
          >
            {improving ? (
              <RefreshCw className="mr-2 h-3.5 w-3.5 animate-spin" />
            ) : (
              <Sparkles className="mr-2 h-3.5 w-3.5" />
            )}
            {improving ? "Thinking…" : "✨ Improve with AI"}
          </Button>
          {content.trim() && (
            <span className="text-[10px] text-foreground-muted">
              Markdown is stripped automatically before posting.
            </span>
          )}
        </div>

        {/* AI Suggestion preview panel */}
        {suggestion !== null && (
          <div className="rounded-lg border border-purple-500/30 bg-purple-500/5 p-3 space-y-2">
            <div className="flex items-center justify-between">
              <span className="flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wide text-purple-300">
                <Sparkles className="h-3.5 w-3.5" />
                Suggested Draft
              </span>
              <span className="text-[10px] text-foreground-muted">
                {editedSuggestion.length} chars · edit before accepting
              </span>
            </div>
            <Textarea
              value={editedSuggestion}
              onChange={(e) => setEditedSuggestion(e.target.value)}
              className="min-h-[100px] resize-none bg-background/50 text-sm"
            />
            <div className="flex items-center justify-end gap-2">
              <Button
                type="button"
                size="sm"
                variant="ghost"
                onClick={dismissSuggestion}
              >
                <X className="mr-1 h-3.5 w-3.5" />
                Dismiss
              </Button>
              <Button
                type="button"
                size="sm"
                variant="ghost"
                onClick={handleImprove}
                disabled={improving}
              >
                <RefreshCw
                  className={`mr-1 h-3.5 w-3.5 ${improving ? "animate-spin" : ""}`}
                />
                Regenerate
              </Button>
              <Button
                type="button"
                size="sm"
                onClick={acceptSuggestion}
                disabled={!editedSuggestion.trim()}
              >
                <Check className="mr-1 h-3.5 w-3.5" />
                Use this
              </Button>
            </div>
          </div>
        )}

        {/* Media attachment — simple upload zone + preview */}
        <div className="space-y-2">
          <label className="text-sm font-medium text-foreground">
            Attach image or PDF
            <span className="ml-2 text-[10px] font-normal text-foreground-muted">
              (LinkedIn only · JPG/PNG/GIF/WebP/PDF · max 25 MB)
            </span>
          </label>

          {/* Hidden file input */}
          <input
            ref={fileInputRef}
            type="file"
            className="hidden"
            accept="image/jpeg,image/png,image/gif,image/webp,application/pdf"
            onChange={handleFilePick}
          />

          {attachedMedia ? (
            /* ── Attached file preview ── */
            <div className="relative rounded-lg border border-accent/40 bg-accent/5 p-3 space-y-2">
              {attachedMedia.kind === "image" ? (
                // eslint-disable-next-line @next/next/no-img-element
                <img
                  src={attachedMedia.url}
                  alt="Attached image preview"
                  className="max-h-48 w-full rounded border border-border object-contain bg-background-secondary"
                  onError={(e) => {
                    (e.currentTarget as HTMLImageElement).alt = "⚠ Preview unavailable — URL may not be public yet";
                  }}
                />
              ) : (
                <div className="flex items-center gap-2 rounded bg-background-secondary p-2">
                  <FileText className="h-5 w-5 text-accent shrink-0" />
                  <span className="truncate text-sm text-foreground">{attachedMedia.name}</span>
                </div>
              )}

              {/* PDF title input */}
              {attachedMedia.kind === "document" && (
                <Input
                  type="text"
                  placeholder="Document title (required for LinkedIn carousel)"
                  value={attachedMedia.title}
                  onChange={(e) => setMediaTitle(e.target.value)}
                  disabled={publishing}
                  className="text-sm"
                  maxLength={100}
                />
              )}

              <div className="flex items-center justify-between">
                <span className="text-[11px] text-foreground-muted truncate">
                  ✅ <strong>{attachedMedia.name}</strong> — will be posted as an image
                </span>
                <button
                  type="button"
                  onClick={() => setAttachedMedia(null)}
                  className="ml-2 shrink-0 rounded p-1 text-foreground-muted hover:text-foreground hover:bg-background-secondary transition-colors"
                  title="Remove attachment"
                >
                  <X className="h-3.5 w-3.5" />
                </button>
              </div>

              <button
                type="button"
                onClick={() => fileInputRef.current?.click()}
                disabled={publishing || uploading}
                className="text-xs text-accent hover:underline disabled:opacity-50"
              >
                {uploading ? "Uploading…" : "Replace with a different file"}
              </button>
            </div>
          ) : (
            /* ── Upload drop zone ── */
            <button
              type="button"
              onClick={() => fileInputRef.current?.click()}
              disabled={publishing || uploading}
              className="w-full flex flex-col items-center justify-center gap-2 rounded-lg border-2 border-dashed border-border hover:border-accent/60 bg-background-secondary/40 hover:bg-accent/5 transition-all p-6 disabled:opacity-50 cursor-pointer"
            >
              {uploading ? (
                <RefreshCw className="h-6 w-6 text-accent animate-spin" />
              ) : (
                <Upload className="h-6 w-6 text-foreground-muted" />
              )}
              <span className="text-sm font-medium text-foreground">
                {uploading ? "Uploading…" : "Click to attach an image or PDF"}
              </span>
              <span className="text-xs text-foreground-muted">
                JPG, PNG, GIF, WebP, or PDF · max 25 MB
              </span>
            </button>
          )}
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
                  type="button"
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
            type="button"
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
            type="button"
            onClick={handleSubmit}
            disabled={publishing || !content.trim() || selectedPlatforms.length === 0}
          >
            {publishing ? (
              <RefreshCw className="mr-2 h-4 w-4 animate-spin" />
            ) : isScheduled ? (
              <Calendar className="mr-2 h-4 w-4" />
            ) : (
              <Send className="mr-2 h-4 w-4" />
            )}
            {publishing
              ? "Publishing…"
              : isScheduled
              ? "Schedule"
              : "Publish Now"}
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}

/* -------------------------------------------------------------------------- */
/*  Recent Posts                                                              */
/* -------------------------------------------------------------------------- */

/**
 * A "group" is one or more posts that share the same content and were
 * published at roughly the same time (same minute) to different platforms.
 * We render them as a single card showing all platform icons side-by-side.
 */
interface PostGroup {
  key: string;
  posts: SocialPost[];      // one per platform in this group
  content: string;
  status: string;           // worst-case status across platforms
  published_at?: string;
  scheduled_at?: string;
}

function groupPosts(posts: SocialPost[]): PostGroup[] {
  // Group posts that share identical content published within the same minute.
  const groups: PostGroup[] = [];
  const used = new Set<string>();

  for (let i = 0; i < posts.length; i++) {
    const a = posts[i];
    if (used.has(a.id)) continue;

    const group: PostGroup = {
      key: a.id,
      posts: [a],
      content: a.content,
      status: a.status,
      published_at: a.published_at,
      scheduled_at: a.scheduled_at,
    };

    for (let j = i + 1; j < posts.length; j++) {
      const b = posts[j];
      if (used.has(b.id)) continue;
      // Same content + both published/failed within same minute → group them
      if (
        b.content === a.content &&
        b.platform !== a.platform &&
        ["published", "failed", "draft"].includes(b.status) &&
        ["published", "failed", "draft"].includes(a.status)
      ) {
        // Only group posts that were likely created together (adjacent in list)
        if (j <= i + 3) {
          group.posts.push(b);
          used.add(b.id);
          // Worst-case status: failed > draft > scheduled > published
          const rank = (s: string) =>
            s === "failed" ? 3 : s === "draft" ? 2 : s === "scheduled" ? 1 : 0;
          if (rank(b.status) > rank(group.status)) group.status = b.status;
          if (!group.published_at && b.published_at) group.published_at = b.published_at;
        }
      }
    }

    used.add(a.id);
    groups.push(group);
  }
  return groups;
}

function PostCard({ group, onDelete }: { group: PostGroup; onDelete: (id: string) => void }) {
  const { posts, content, status, published_at, scheduled_at } = group;

  return (
    <Card className="group relative overflow-hidden">
      <div
        className={`absolute left-0 top-0 h-full w-1 ${
          status === "published"
            ? "bg-green-500"
            : status === "scheduled"
            ? "bg-amber-500"
            : status === "failed"
            ? "bg-red-500"
            : "bg-border"
        }`}
      />
      <CardContent className="flex items-start gap-3 py-3 pl-5">
        {/* Platform icons — one per post in the group */}
        <div className="flex flex-col gap-1 shrink-0">
          {posts.map((p) => {
            const Icon = PLATFORM_ICON[p.platform] || Globe;
            const color = PLATFORM_COLOR[p.platform] || "text-foreground-secondary";
            return (
              <div key={p.id} className={`rounded-lg bg-background-secondary p-1.5 ${color}`}>
                <Icon className="h-3.5 w-3.5" />
              </div>
            );
          })}
        </div>

        <div className="flex-1 min-w-0 space-y-1">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-1.5 flex-wrap">
              <Badge
                variant={
                  status === "published"
                    ? "success"
                    : status === "scheduled"
                    ? "secondary"
                    : status === "failed"
                    ? "destructive"
                    : "secondary"
                }
                className="text-[10px]"
              >
                {status}
              </Badge>
              {/* Per-platform status badges when they differ */}
              {posts.length > 1 && posts.some((p) => p.status !== posts[0].status) &&
                posts.map((p) => (
                  <span key={p.id} className="text-[9px] text-foreground-muted">
                    {PLATFORM_LABEL[p.platform] ?? p.platform}: {p.status}
                  </span>
                ))}
            </div>
            <span className="text-[10px] text-foreground-muted">
              {published_at
                ? new Date(published_at).toLocaleDateString()
                : scheduled_at
                ? `Scheduled: ${new Date(scheduled_at).toLocaleDateString()}`
                : "Draft"}
            </span>
          </div>

          <p className="text-sm text-foreground line-clamp-2">{content}</p>

          {/* Analytics (show for first post that has them) */}
          {posts.find((p) => p.analytics) && (() => {
            const a = posts.find((p) => p.analytics)!.analytics!;
            return (
              <div className="flex items-center gap-3 text-[10px] text-foreground-muted">
                <span>❤️ {a.likes}</span>
                <span>🔄 {a.shares}</span>
                <span>💬 {a.comments}</span>
                <span>👁 {a.impressions.toLocaleString()}</span>
              </div>
            );
          })()}

          {/* View links for each published platform */}
          <div className="flex flex-wrap gap-x-3 gap-y-0.5">
            {posts.map((p) => {
              const url =
                p.post_url ||
                (p.platform === "linkedin" ? linkedinPostUrl(p.platform_post_id) : null);
              if (p.status !== "published" || !url) return null;
              return (
                <a
                  key={p.id}
                  href={url}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="inline-flex items-center gap-1 text-[11px] font-medium text-accent hover:underline"
                >
                  <ExternalLink className="h-3 w-3" />
                  View on {PLATFORM_LABEL[p.platform] ?? p.platform}
                </a>
              );
            })}
          </div>
        </div>

        {/* Delete button — deletes all posts in the group */}
        <button
          type="button"
          onClick={() => posts.forEach((p) => onDelete(p.id))}
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
  const [publishing, setPublishing] = useState(false);
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
        setPosts(socialPosts.value ?? []);
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
    scheduledAt?: string,
    media?: { kind: "image" | "document"; urls: string[]; title?: string }
  ) => {
    setPublishing(true);
    try {
      // For each platform: 1) create a draft, 2) if not scheduled → publish immediately.
      const perPlatform = await Promise.allSettled(
        platforms.map(async (platform) => {
          // Media support per platform:
          //   linkedin  → image ✅  document/PDF ✅
          //   facebook  → image ✅  document ❌ (falls back to text)
          //   others    → text only (media stripped)
          let mediaForPlatform: typeof media | undefined;
          if (media) {
            if (platform === "linkedin") {
              mediaForPlatform = media; // full support
            } else if (platform === "facebook" && media.kind === "image") {
              mediaForPlatform = media; // image only
            }
            // all other platforms: undefined (text-only)
          }

          // Step 1 — create draft
          const draft = (await apiClient.createSocialPost({
            platform,
            content,
            hashtags: content.match(/#\w+/g) || undefined,
            content_type: mediaForPlatform?.kind ?? "text",
            media_urls: mediaForPlatform?.urls,
            title: mediaForPlatform?.title,
          })) as { id?: string; error?: string };

          if (!draft?.id || draft.error) {
            throw new Error(draft?.error || `${platform}: failed to create post`);
          }

          // Step 2 — publish now (only if not scheduled).
          // NOTE: backend scheduler does not yet auto-publish at scheduledAt;
          //       for v1 a scheduled post is stored as a draft for manual publish.
          if (!scheduledAt) {
            const published = await apiClient.publishSocialPost(draft.id);
            if (published?.error || published?.status !== "published") {
              throw new Error(
                published?.error || `${platform}: provider rejected the post`
              );
            }
            return {
              platform,
              url:
                published.post_url ||
                (platform === "linkedin"
                  ? linkedinPostUrl(published.platform_post_id)
                  : null),
            };
          }
          return { platform, url: null as string | null };
        })
      );

      const succeeded = perPlatform.filter(
        (r): r is PromiseFulfilledResult<{ platform: string; url: string | null }> =>
          r.status === "fulfilled"
      );
      const failed = perPlatform.filter(
        (r): r is PromiseRejectedResult => r.status === "rejected"
      );

      if (succeeded.length > 0) {
        // If we have a clickable URL (LinkedIn), surface it in the toast so
        // the user gets a one-click "View on LinkedIn" affordance.
        const firstUrl = succeeded.map((r) => r.value.url).find((u) => !!u);
        addToast({
          variant: "success",
          title: scheduledAt
            ? "Saved as draft"
            : succeeded.length === platforms.length
            ? "Published successfully"
            : "Partially published",
          description: scheduledAt
            ? "Scheduling is coming in a future release — your post is saved as a draft you can publish manually."
            : firstUrl
            ? `Posted to ${succeeded.length} platform(s). View it: ${firstUrl}`
            : `Posted to ${succeeded.length} platform(s)${
                failed.length > 0 ? `, ${failed.length} failed` : ""
              }`,
        });
      }

      if (failed.length > 0) {
        const firstError =
          failed[0].reason instanceof Error
            ? failed[0].reason.message
            : String(failed[0].reason);
        addToast({
          variant: succeeded.length === 0 ? "error" : "warning",
          title:
            succeeded.length === 0
              ? "Failed to publish"
              : `${failed.length} platform(s) failed`,
          description: firstError,
        });
      }

      // Reload posts from server so statuses reflect reality
      loadData();
    } catch (e) {
      addToast({
        variant: "error",
        title: "Failed to publish",
        description: e instanceof Error ? e.message : "Unknown error",
      });
    } finally {
      setPublishing(false);
    }
  };

  const handleDelete = async (id: string) => {
    // Optimistically remove from local state
    setPosts((prev) => prev.filter((p) => p.id !== id));
    try {
      const result = await apiClient.deleteSocialPost(id);
      // The backend always returns HTTP 200 — check the payload for
      // server-side errors (e.g. "Social media manager not enabled",
      // "Post not found") and revert if the deletion actually failed.
      if (result?.error) {
        throw new Error(result.error);
      }
      addToast({ variant: "success", title: "Post deleted" });
    } catch (e) {
      // Revert the optimistic removal so the post reappears in the UI.
      await loadData();
      addToast({
        variant: "error",
        title: "Failed to delete post",
        description: e instanceof Error ? e.message : "Unknown error",
      });
    }
  };

  // ── Recent Posts filter state ─────────────────────────────
  const [searchQuery, setSearchQuery] = useState("");
  const [filterStatus, setFilterStatus] = useState<"all" | "published" | "draft" | "scheduled" | "failed">("all");
  const [filterDateFrom, setFilterDateFrom] = useState("");
  const [filterDateTo, setFilterDateTo] = useState("");
  const [showAll, setShowAll] = useState(false);

  const filteredPosts = posts.filter((p) => {
    if (filterStatus !== "all" && p.status !== filterStatus) return false;
    if (searchQuery && !p.content.toLowerCase().includes(searchQuery.toLowerCase())) return false;
    const date = p.published_at || p.scheduled_at;
    if (filterDateFrom && date && date < filterDateFrom) return false;
    if (filterDateTo && date && date > filterDateTo + "T23:59:59") return false;
    return true;
  });

  // Group same-content multi-platform posts into one card (newest first)
  const filteredGroups = groupPosts(filteredPosts);
  const visibleGroups = showAll ? filteredGroups : filteredGroups.slice(0, 10);

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
      <ComposeCard
        connectedPlatforms={connectedPlatforms}
        onPublish={handlePublish}
        publishing={publishing}
      />

      {/* Recent Posts */}
      <section>
        {/* Header + filters */}
        <div className="mb-4 space-y-3">
          <h2 className="flex items-center gap-2 text-sm font-semibold uppercase tracking-wider text-foreground-muted">
            <Clock className="h-4 w-4" />
            Recent Posts ({filteredGroups.length}{filteredGroups.length !== groupPosts(posts).length ? ` of ${groupPosts(posts).length}` : ""})
          </h2>

          {/* Search + status filter */}
          <div className="flex flex-wrap gap-2">
            <div className="relative flex-1 min-w-[180px]">
              <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-foreground-muted" />
              <input
                type="text"
                placeholder="Search posts…"
                value={searchQuery}
                onChange={(e) => { setSearchQuery(e.target.value); setShowAll(false); }}
                className="w-full rounded-md border border-border bg-background pl-8 pr-3 py-1.5 text-xs text-foreground placeholder:text-foreground-muted focus:outline-none focus:ring-1 focus:ring-accent"
              />
            </div>
            <select
              value={filterStatus}
              onChange={(e) => { setFilterStatus(e.target.value as typeof filterStatus); setShowAll(false); }}
              className="rounded-md border border-border bg-background px-2 py-1.5 text-xs text-foreground focus:outline-none focus:ring-1 focus:ring-accent"
            >
              <option value="all">All statuses</option>
              <option value="published">Published</option>
              <option value="draft">Draft</option>
              <option value="scheduled">Scheduled</option>
              <option value="failed">Failed</option>
            </select>
          </div>

          {/* Date range filter */}
          <div className="flex flex-wrap items-center gap-2 text-xs text-foreground-muted">
            <Calendar className="h-3.5 w-3.5" />
            <span>From</span>
            <input
              type="date"
              value={filterDateFrom}
              onChange={(e) => { setFilterDateFrom(e.target.value); setShowAll(false); }}
              className="rounded-md border border-border bg-background px-2 py-1 text-xs text-foreground focus:outline-none focus:ring-1 focus:ring-accent"
            />
            <span>To</span>
            <input
              type="date"
              value={filterDateTo}
              onChange={(e) => { setFilterDateTo(e.target.value); setShowAll(false); }}
              className="rounded-md border border-border bg-background px-2 py-1 text-xs text-foreground focus:outline-none focus:ring-1 focus:ring-accent"
            />
            {(filterDateFrom || filterDateTo || searchQuery || filterStatus !== "all") && (
              <button
                type="button"
                onClick={() => { setFilterDateFrom(""); setFilterDateTo(""); setSearchQuery(""); setFilterStatus("all"); setShowAll(false); }}
                className="ml-1 flex items-center gap-1 text-accent hover:underline"
              >
                <X className="h-3 w-3" /> Clear
              </button>
            )}
          </div>
        </div>

        {/* Post list */}
        {filteredPosts.length === 0 ? (
          <Card className="py-12 text-center">
            <CardContent>
              <PenLine className="mx-auto h-10 w-10 text-foreground-muted" />
              <p className="mt-3 text-sm text-foreground-secondary">
                {posts.length === 0 ? "No posts yet. Compose your first post above!" : "No posts match your filters."}
              </p>
            </CardContent>
          </Card>
        ) : (
          <>
            <div className="space-y-3">
              {visibleGroups.map((group) => (
                <PostCard key={group.key} group={group} onDelete={handleDelete} />
              ))}
            </div>
            {filteredGroups.length > 10 && (
              <button
                type="button"
                onClick={() => setShowAll((v) => !v)}
                className="mt-3 flex w-full items-center justify-center gap-1.5 rounded-md border border-border py-2 text-xs text-foreground-muted hover:text-foreground transition-colors"
              >
                {showAll ? (
                  <><ChevronUp className="h-3.5 w-3.5" /> Show less</>
                ) : (
                  <><ChevronDown className="h-3.5 w-3.5" /> Show {filteredGroups.length - 10} more</>
                )}
              </button>
            )}
          </>
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
