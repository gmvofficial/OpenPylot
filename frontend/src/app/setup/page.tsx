"use client";

import { useEffect, useState, useCallback } from "react";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import type { Integration } from "@/types";
import { apiClient } from "@/lib/api";
import { useToastStore } from "@/stores/toast";
import {
  Calendar,
  Mail,
  MessageCircle,
  Send,
  Github,
  Slack,
  Globe,
  CheckCircle2,
  AlertCircle,
  Settings2,
  Zap,
  RefreshCw,
  X,
  Shield,
  Eye,
  EyeOff,
  Twitter,
  Linkedin,
  Facebook,
  Instagram,
  Youtube,
  Rss,
  Hash,
  BookOpen,
  Pen,
  Share2,
  Radio,
  AtSign,
  Video,
  Image as ImageIcon,
  FileText,
  ChevronDown,
  ChevronUp,
  ExternalLink,
  BookOpenCheck,
  Plug,
} from "lucide-react";

/**
 * Release tier for a social-media integration.
 *   1 = stable, available in v1
 *   2 = beta — works but needs setup effort / paid API tier
 *   3 = "coming soon" — provider exists but onboarding too painful for v1
 *       (Meta business verification, ByteDance review, Google sensitive scopes)
 */
type ReleaseTier = 1 | 2 | 3;

interface SetupGuide {
  /** Short summary, e.g. "Sign in with your Google account" */
  summary: string;
  /** Numbered steps the user must follow before / during connect */
  steps: string[];
  /** Optional link to provider docs */
  docsUrl?: string;
  /** Optional label for docsUrl button. Defaults to "Open provider docs". */
  docsLabel?: string;
}

const SERVICE_META: Record<
  string,
  {
    icon: React.ElementType;
    color: string;
    description: string;
    category?: string;
    tier?: ReleaseTier;
    tierNote?: string;
    setupGuide?: SetupGuide;
  }
> = {
  // Productivity integrations
  google_calendar: {
    icon: Calendar,
    color: "text-accent",
    description: "Manage events, check schedules, and get reminders for upcoming meetings.",
    category: "productivity",
    tier: 1,
    setupGuide: {
      summary: "Create a Google Cloud OAuth client, paste its credentials, then approve access.",
      steps: [
        "Open Google Cloud Console → APIs & Services → Credentials.",
        "Enable the Google Calendar API for your project.",
        "Click Create credentials → OAuth client ID → Web application.",
        "Add http://localhost:8085/callback as an Authorized redirect URI.",
        "Copy the Client ID and Client Secret.",
        "Click “Connect” here, paste the Client ID + Secret, and submit.",
        "A Google sign-in tab opens — pick your account and approve calendar access.",
        "Status updates to Connected automatically when the redirect returns.",
      ],
      docsUrl: "https://developers.google.com/calendar/api/quickstart/js",
      docsLabel: "Google Calendar OAuth setup",
    },
  },
  gmail: {
    icon: Mail,
    color: "text-accent-error",
    description: "Read, search, compose, and manage your emails directly through the agent.",
    category: "productivity",
    tier: 1,
    setupGuide: {
      summary: "Create a Google Cloud OAuth client, paste its credentials, then approve Gmail access.",
      steps: [
        "Open Google Cloud Console → APIs & Services → Credentials.",
        "Enable the Gmail API for your project.",
        "Click Create credentials → OAuth client ID → Web application.",
        "Add http://localhost:8085/callback as an Authorized redirect URI.",
        "On the OAuth consent screen, add the gmail.readonly / gmail.send / gmail.modify scopes.",
        "Copy the Client ID and Client Secret.",
        "Click “Connect” here, paste the Client ID + Secret, and submit.",
        "A Google sign-in tab opens — pick the Gmail account and approve the requested scopes.",
        "Status updates to Connected automatically.",
      ],
      docsUrl: "https://developers.google.com/gmail/api/auth/scopes",
      docsLabel: "Gmail API scopes",
    },
  },
  telegram: {
    icon: Send,
    color: "text-sky-400",
    description: "Interact with the agent via Telegram bot. Receive notifications and replies.",
    category: "messaging",
    tier: 1,
    setupGuide: {
      summary: "Create a Telegram bot with @BotFather and paste the token here.",
      steps: [
        "Open Telegram and start a chat with @BotFather.",
        "Send /newbot, choose a display name and a unique username ending in ‘bot’.",
        "BotFather replies with an HTTP API token (e.g. 123456:ABC…) — copy it.",
        "(Optional) To receive proactive messages, send /start to your new bot, then visit https://api.telegram.org/bot<TOKEN>/getUpdates and copy the chat ‘id’.",
        "Click “Connect” here, paste the bot token (and optional chat ID), and submit.",
      ],
      docsUrl: "https://core.telegram.org/bots/tutorial",
      docsLabel: "Telegram bot tutorial",
    },
  },
  whatsapp: {
    icon: MessageCircle,
    color: "text-accent-success",
    description: "Send and receive messages through WhatsApp Business API.",
    category: "messaging",
    tier: 3,
    tierNote: "Requires WhatsApp Business account + Meta Business Verification.",
  },
  github: {
    icon: Github,
    color: "text-purple-400",
    description: "Monitor repositories, manage issues, review PRs and track notifications.",
    category: "developer",
    tier: 3,
  },
  slack: {
    icon: Slack,
    color: "text-amber-400",
    description: "Connect to Slack workspaces for messages, channels, and team coordination.",
    category: "messaging",
    tier: 1,
    setupGuide: {
      summary: "Create a Slack app, install it to your workspace, and paste its Bot User OAuth Token.",
      steps: [
        "Open https://api.slack.com/apps and click Create New App → From scratch.",
        "Pick a name and choose the workspace to install into.",
        "Open OAuth & Permissions and add bot scopes: chat:write, channels:read, channels:history, users:read.",
        "Click Install to Workspace and approve the prompt.",
        "Copy the Bot User OAuth Token (starts with xoxb-) shown after install.",
        "Click “Connect” here, paste the token, and submit.",
        "In Slack, invite the bot to a channel with /invite @<your-bot-name>.",
      ],
      docsUrl: "https://api.slack.com/quickstart",
      docsLabel: "Slack app quickstart",
    },
  },
  // Social media platforms
  twitter: {
    icon: Twitter,
    color: "text-sky-400",
    description: "Post tweets, reply to threads, and track engagement on Twitter/X.",
    category: "social",
    tier: 1,
    setupGuide: {
      summary: "Create an X developer app, set it to Read+Write, and paste 4 OAuth 1.0a keys (NOT the Bearer or OAuth 2.0 keys).",
      steps: [
        "Sign in at https://developer.x.com → open the Developer Portal → your Project → your App.",
        "Open the Settings tab → User authentication settings → click Set up / Edit.",
        "Set App permissions = Read and write (or Read, write, and DM). Type = Web App / OAuth 1.0a. Save.",
        "Open the Keys and tokens tab. Under OAuth 1.0 Keys → Consumer Keys: click Show (or Regenerate) and copy:",
        "    • Consumer Key  →  paste into pylot's API Key field",
        "    • Consumer Secret  →  paste into pylot's API Secret field",
        "On the same page, find Authentication Tokens → Access Token and Secret and click Regenerate.",
        "    A dialog appears showing both values once — copy them immediately:",
        "    • Access Token  →  paste into pylot's Access Token field",
        "    • Access Token Secret  →  paste into pylot's Access Token Secret field",
        "    NOTE: you MUST regenerate the Access Token AFTER setting Read+Write, otherwise posting returns 403.",
        "Do NOT use the Bearer Token or the OAuth 2.0 Client ID/Secret — those are different auth types.",
        "Click Connect here and paste all four values.",
      ],
      docsUrl: "https://developer.x.com/en/docs/authentication/oauth-1-0a",
      docsLabel: "X OAuth 1.0a keys guide",
    },
  },
  linkedin: {
    icon: Linkedin,
    color: "text-blue-500",
    description: "Publish professional posts, articles, and track business network engagement.",
    category: "social",
    tier: 1,
    setupGuide: {
      summary: "Create a LinkedIn developer app, generate a member access token, and paste it here.",
      steps: [
        "Open https://www.linkedin.com/developers/apps and create a new app.",
        "Under Products, request access to ‘Share on LinkedIn’ and ‘Sign In with LinkedIn using OpenID Connect’.",
        "Open the Auth tab and use the OAuth 2.0 tools → Generate access token.",
        "Select scopes w_member_social, openid, profile (all three are required so we can post AND auto-detect your profile URN).",
        "Approve the prompt for your own LinkedIn account.",
        "Copy the resulting access token (starts with AQV…).",
        "Click “Connect” here, paste the access token, and submit. We’ll auto-detect your Person ID via /v2/userinfo.",
        "If your token lacks the openid/profile scope, optionally paste your numeric Person ID into the Person ID field — find it by opening your LinkedIn profile while signed in and looking at the value after `/in/` (or use the Profile API).",
      ],
      docsUrl: "https://learn.microsoft.com/en-us/linkedin/shared/authentication/authorization-code-flow",
      docsLabel: "LinkedIn auth docs",
    },
  },
  facebook: {
    icon: Facebook,
    color: "text-accent",
    description: "Manage your Facebook Page posts, schedule content, and view insights.",
    category: "social",
    tier: 1,
    setupGuide: {
      summary: "Get a long-lived PAGE access token (not a User token) with publish permissions.",
      steps: [
        "Go to https://developers.facebook.com/tools/explorer and select your app.",
        "Click 'Generate Access Token'. In the permissions list add ALL three: pages_show_list, pages_read_engagement, pages_manage_posts. Missing any one returns a #200 OAuthException.",
        "CRITICAL: find the blue dropdown showing your app name, switch it to your PAGE name — the token becomes a Page Access Token. Do NOT use the User token from the previous step.",
        "Copy the Page token, then go to https://developers.facebook.com/tools/debug/accesstoken and click 'Extend Access Token' to get a ~60-day token.",
        "Your Page ID is the number shown beside your Page in the Explorer dropdown, or find it at facebook.com/YOUR_PAGE > About > Page transparency.",
        "Paste both the Page ID and the extended Page Access Token here and click Connect.",
      ],
      docsUrl: "https://developers.facebook.com/docs/pages-api/getting-started",
      docsLabel: "Facebook Pages API guide",
    },
  },
  instagram: {
    icon: Instagram,
    color: "text-pink-400",
    description: "Share photos, reels, and stories via the Instagram Graph API.",
    category: "social",
    tier: 3,
    tierNote: "Requires Meta Business Verification + Instagram Business account.",
  },
  bluesky: {
    icon: AtSign,
    color: "text-accent",
    description: "Post to Bluesky's decentralized social network using app passwords.",
    category: "social",
    tier: 3,
    tierNote: "New — uses an app password from Bluesky settings.",
  },
  tiktok: {
    icon: Video,
    color: "text-rose-400",
    description: "Publish short-form video content and track TikTok analytics.",
    category: "social",
    tier: 3,
    tierNote: "Requires ByteDance Content Posting API approval.",
  },
  youtube: {
    icon: Youtube,
    color: "text-red-500",
    description: "Upload videos, manage playlists, and track YouTube channel performance.",
    category: "social",
    tier: 3,
    tierNote: "Requires Google sensitive-scope review for youtube.upload.",
  },
  pinterest: {
    icon: ImageIcon,
    color: "text-accent-error",
    description: "Pin images and ideas to boards for visual discovery and sharing.",
    category: "social",
    tier: 3,
    tierNote: "Requires Pinterest app review for write scope.",
  },
  reddit: {
    icon: Hash,
    color: "text-accent-warning",
    description: "Post to subreddits, reply to threads, and track karma/engagement.",
    category: "social",
    tier: 3,
  },
  threads: {
    icon: AtSign,
    color: "text-foreground-secondary",
    description: "Share short-form text posts on Meta's Threads platform.",
    category: "social",
    tier: 3,
    tierNote: "Threads API still in limited rollout via Meta.",
  },
  mastodon: {
    icon: Radio,
    color: "text-purple-400",
    description: "Post to your Mastodon instance's federated social network.",
    category: "social",
    tier: 3,
    tierNote: "Niche audience — deferred to a future release.",
  },
  discord: {
    icon: Hash,
    color: "text-indigo-400",
    description: "Send messages and updates to Discord channels via bot or webhook.",
    category: "messaging",
    tier: 1,
    setupGuide: {
      summary: "Create a Discord bot, invite it to your server, and paste its bot token here.",
      steps: [
        "Open https://discord.com/developers/applications and click New Application.",
        "Open the Bot tab → Reset Token → copy the bot token (you only see it once).",
        "Under Privileged Gateway Intents, enable Message Content Intent if you want the bot to read messages.",
        "Open OAuth2 → URL Generator: pick scope ‘bot’ and permissions Send Messages + Read Message History.",
        "Open the generated URL and add the bot to your server.",
        "(Optional) In Discord, enable Developer Mode then right-click a channel → Copy Channel ID.",
        "Click “Connect” here, paste the bot token (and optional channel ID), and submit.",
      ],
      docsUrl: "https://discord.com/developers/docs/quick-start/getting-started",
      docsLabel: "Discord bot quickstart",
    },
  },
  medium: {
    icon: BookOpen,
    color: "text-emerald-400",
    description: "Publish long-form articles and stories on Medium.",
    category: "publishing",
    tier: 3,
    tierNote: "Medium deprecated their public posting API — new tokens cannot be issued.",
  },
  devto: {
    icon: FileText,
    color: "text-foreground-secondary",
    description: "Publish developer articles and tutorials on Dev.to.",
    category: "publishing",
    tier: 3,
  },
  hashnode: {
    icon: Pen,
    color: "text-accent",
    description: "Publish developer blog posts on your Hashnode publication.",
    category: "publishing",
    tier: 3,
    tierNote: "Requires a personal access token + your Hashnode publication ID.",
  },
  wordpress: {
    icon: Share2,
    color: "text-cyan-400",
    description: "Create and manage blog posts on your WordPress site.",
    category: "publishing",
    tier: 3,
  },
};

/* -------------------------------------------------------------------------- */
/*  Credential Modal                                                          */
/* -------------------------------------------------------------------------- */

interface CredentialField {
  name: string;
  label: string;
  field_type: string;
  required: boolean;
  placeholder: string;
}

function CredentialModal({
  service,
  fields,
  onSubmit,
  onClose,
  loading,
}: {
  service: string;
  fields: CredentialField[];
  onSubmit: (credentials: Record<string, string>) => void;
  onClose: () => void;
  loading: boolean;
}) {
  const [values, setValues] = useState<Record<string, string>>({});
  const [showPasswords, setShowPasswords] = useState<Record<string, boolean>>({});

  const meta = SERVICE_META[service] ?? { icon: Globe, color: "text-foreground-secondary" };
  const Icon = meta.icon;
  const guide = meta.setupGuide;

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    onSubmit(values);
  };

  const allRequiredFilled = fields
    .filter((f) => f.required)
    .every((f) => values[f.name]?.trim());

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4 backdrop-blur-sm overflow-y-auto">
      <Card
        className={`my-8 w-full ${
          guide ? "max-w-4xl" : "max-w-md"
        } shadow-2xl border-border-hover`}
      >
        <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-4">
          <div className="flex items-center gap-3">
            <div className={`rounded-lg bg-background-secondary p-2.5 ${meta.color}`}>
              <Icon className="h-5 w-5" />
            </div>
            <div>
              <CardTitle className="text-base capitalize">
                Connect {service.replace(/_/g, " ")}
              </CardTitle>
              <CardDescription className="text-xs mt-0.5">
                {guide?.summary ?? "Enter your credentials to connect"}
              </CardDescription>
            </div>
          </div>
          <Button size="icon" variant="ghost" onClick={onClose} className="h-8 w-8">
            <X className="h-4 w-4" />
          </Button>
        </CardHeader>
        <form onSubmit={handleSubmit}>
          <CardContent
            className={`${
              guide
                ? "grid gap-6 md:grid-cols-[1.1fr_1fr]"
                : "space-y-4"
            }`}
          >
            {/* Setup guide panel (left on md+, top on small) */}
            {guide && (
              <div className="order-2 md:order-1 rounded-lg border border-border bg-background-secondary/40 p-4 text-xs">
                <div className="mb-2 flex items-center gap-2 text-foreground">
                  <BookOpenCheck className="h-4 w-4 text-accent" />
                  <p className="font-semibold">Setup guide</p>
                </div>
                <p className="mb-3 text-foreground-secondary">{guide.summary}</p>
                <ol className="ml-4 list-decimal space-y-1.5 text-foreground-secondary">
                  {guide.steps.map((step, i) => (
                    <li key={i} className="leading-relaxed">
                      {step}
                    </li>
                  ))}
                </ol>
                {guide.docsUrl && (
                  <a
                    href={guide.docsUrl}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="mt-3 inline-flex items-center gap-1 text-accent hover:underline"
                  >
                    <ExternalLink className="h-3.5 w-3.5" />
                    {guide.docsLabel || "Open provider docs"}
                  </a>
                )}
              </div>
            )}

            {/* Credential fields (right on md+) */}
            <div className={`${guide ? "order-1 md:order-2 space-y-4" : "space-y-4"}`}>
              {fields.map((field) => (
                <div key={field.name} className="space-y-1.5">
                  <label className="flex items-center gap-2 text-sm font-medium text-foreground">
                    <Shield className="h-3.5 w-3.5 text-foreground-muted" />
                    {field.label}
                    {field.required && <span className="text-accent-error">*</span>}
                  </label>
                  <div className="relative">
                    <Input
                      type={field.field_type === "password" && !showPasswords[field.name] ? "password" : "text"}
                      value={values[field.name] ?? ""}
                      onChange={(e) => setValues({ ...values, [field.name]: e.target.value })}
                      placeholder={field.placeholder}
                      autoComplete="off"
                    />
                    {field.field_type === "password" && (
                      <button
                        type="button"
                        className="absolute right-2 top-1/2 -translate-y-1/2 p-1 text-foreground-muted hover:text-foreground"
                        onClick={() => setShowPasswords({ ...showPasswords, [field.name]: !showPasswords[field.name] })}
                      >
                        {showPasswords[field.name] ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
                      </button>
                    )}
                  </div>
                </div>
              ))}

              <div className="flex items-center justify-end gap-2 pt-2">
                <Button type="button" variant="ghost" onClick={onClose} disabled={loading}>
                  Cancel
                </Button>
                <Button type="submit" disabled={!allRequiredFilled || loading}>
                  {loading ? (
                    <RefreshCw className="mr-2 h-4 w-4 animate-spin" />
                  ) : (
                    <CheckCircle2 className="mr-2 h-4 w-4" />
                  )}
                  {loading ? "Connecting..." : "Connect"}
                </Button>
              </div>
            </div>
          </CardContent>
        </form>
      </Card>
    </div>
  );
}

/* -------------------------------------------------------------------------- */
/*  Disconnect Confirmation Modal                                             */
/* -------------------------------------------------------------------------- */

function DisconnectModal({
  service,
  onConfirm,
  onClose,
  loading,
}: {
  service: string;
  onConfirm: () => void;
  onClose: () => void;
  loading: boolean;
}) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <Card className="w-full max-w-sm mx-4 shadow-2xl">
        <CardHeader>
          <CardTitle className="text-base">Disconnect {service.replace(/_/g, " ")}?</CardTitle>
          <CardDescription>
            This will remove the stored credentials. You can reconnect later.
          </CardDescription>
        </CardHeader>
        <CardContent className="flex items-center justify-end gap-2">
          <Button variant="ghost" onClick={onClose} disabled={loading}>
            Cancel
          </Button>
          <Button variant="destructive" onClick={onConfirm} disabled={loading}>
            {loading ? <RefreshCw className="mr-2 h-4 w-4 animate-spin" /> : null}
            Disconnect
          </Button>
        </CardContent>
      </Card>
    </div>
  );
}

/* -------------------------------------------------------------------------- */
/*  Integration Card                                                          */
/* -------------------------------------------------------------------------- */

function IntegrationCard({
  integration,
  onConnect,
  onDisconnect,
  onTest,
  testResult,
  testing,
}: {
  integration: Integration;
  onConnect: (id: string) => void;
  onDisconnect: (id: string) => void;
  onTest: (id: string) => void;
  testResult?: { healthy: boolean; details: string } | null;
  testing: boolean;
}) {
  const meta = SERVICE_META[integration.service] ?? {
    icon: Globe,
    color: "text-foreground-secondary",
    description: "External service integration.",
  };
  const Icon = meta.icon;
  const isConnected = integration.status === "connected";
  const isError = integration.status === "error";
  const tier = meta.tier;
  const isComingSoon = tier === 3 && !isConnected;
  const isBeta = tier === 2;
  const guide = meta.setupGuide;
  // Open the guide automatically on error so users see remediation steps.
  const [guideOpen, setGuideOpen] = useState<boolean>(isError);

  return (
    <Card
      className={`group relative overflow-hidden transition-all hover:border-border-hover ${
        isComingSoon ? "opacity-70" : ""
      }`}
    >
      {/* status strip */}
      <div
        className={`absolute left-0 top-0 h-full w-1 ${
          isConnected
            ? "bg-green-500"
            : isError
            ? "bg-red-500"
            : isComingSoon
            ? "bg-foreground-muted/30"
            : "bg-border"
        }`}
      />

      <CardHeader className="flex flex-row items-start justify-between space-y-0 pb-3 pl-5">
        <div className="flex items-center gap-3">
          <div className={`rounded-lg bg-background-secondary p-2.5 ${meta.color}`}>
            <Icon className="h-5 w-5" />
          </div>
          <div>
            <div className="flex items-center gap-2">
              <CardTitle className="text-base font-semibold capitalize">
                {integration.service.replace(/_/g, " ")}
              </CardTitle>
              {isBeta && (
                <span className="rounded bg-amber-500/20 px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-wide text-amber-300">
                  beta
                </span>
              )}
              {isComingSoon && (
                <span className="rounded bg-foreground-muted/15 px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-wide text-foreground-muted">
                  soon
                </span>
              )}
            </div>
            <CardDescription className="mt-0.5 text-xs">
              {meta.description}
            </CardDescription>
            {meta.tierNote && (
              <p
                className={`mt-1 text-[11px] ${
                  isComingSoon ? "text-foreground-muted" : "text-amber-300/90"
                }`}
              >
                ⚠ {meta.tierNote}
              </p>
            )}
          </div>
        </div>
        <Badge
          variant={
            isConnected
              ? "success"
              : isError
              ? "destructive"
              : isComingSoon
              ? "outline"
              : "secondary"
          }
        >
          {isConnected && <CheckCircle2 className="mr-1 h-3 w-3" />}
          {isError && <AlertCircle className="mr-1 h-3 w-3" />}
          {isComingSoon ? "coming soon" : integration.status}
        </Badge>
      </CardHeader>

      <CardContent className="space-y-2 pl-5">
        <div className="flex flex-wrap items-center justify-between gap-2">
          {isConnected ? (
            <p className="min-w-0 flex-1 truncate text-xs text-foreground-muted">
              Connected {(integration.connected_at || integration.connectedAt) ? `since ${new Date((integration.connected_at || integration.connectedAt)!).toLocaleDateString()}` : ""}
            </p>
          ) : (
            <p className="min-w-0 flex-1 truncate text-xs text-foreground-muted">
              {isComingSoon ? "Available in a future release" : "Not yet connected"}
            </p>
          )}

          <div className="flex flex-shrink-0 flex-wrap items-center justify-end gap-2">
            {guide && !isComingSoon && (
              <Button
                size="sm"
                variant="ghost"
                onClick={() => setGuideOpen((v) => !v)}
                aria-expanded={guideOpen}
                aria-label={guideOpen ? "Hide setup guide" : "Show setup guide"}
              >
                <BookOpenCheck className="mr-1 h-3.5 w-3.5" />
                <span className="hidden sm:inline">Setup guide</span>
                <span className="sm:hidden">Guide</span>
                {guideOpen ? (
                  <ChevronUp className="ml-1 h-3.5 w-3.5" />
                ) : (
                  <ChevronDown className="ml-1 h-3.5 w-3.5" />
                )}
              </Button>
            )}
            {isConnected && (
              <>
                <Button size="sm" variant="ghost" onClick={() => onTest(integration.service)} disabled={testing}>
                  {testing ? (
                    <RefreshCw className="mr-1 h-3.5 w-3.5 animate-spin" />
                  ) : (
                    <Zap className="mr-1 h-3.5 w-3.5" />
                  )}
                  Test
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => onDisconnect(integration.service)}
                  className="whitespace-nowrap"
                >
                  <Settings2 className="mr-1 h-3.5 w-3.5" />
                  Disconnect
                </Button>
              </>
            )}
            {isError && (
              <Button size="sm" variant="outline" onClick={() => onConnect(integration.service)}>
                <RefreshCw className="mr-1 h-3.5 w-3.5" />
                Reconnect
              </Button>
            )}
            {!isConnected && !isError && !isComingSoon && (
              <Button size="sm" onClick={() => onConnect(integration.service)}>
                <Plug className="mr-1 h-3.5 w-3.5" />
                Connect
              </Button>
            )}
            {isComingSoon && (
              <Button size="sm" variant="outline" disabled>
                Coming soon
              </Button>
            )}
          </div>
        </div>

        {/* Setup guide panel */}
        {guide && guideOpen && !isComingSoon && (
          <div className="mt-2 rounded-lg border border-border bg-background-secondary/40 p-3 text-xs">
            <p className="mb-2 font-medium text-foreground">{guide.summary}</p>
            <ol className="ml-4 list-decimal space-y-1 text-foreground-secondary">
              {guide.steps.map((step, i) => (
                <li key={i}>{step}</li>
              ))}
            </ol>
            {guide.docsUrl && (
              <a
                href={guide.docsUrl}
                target="_blank"
                rel="noopener noreferrer"
                className="mt-3 inline-flex items-center gap-1 text-accent hover:underline"
              >
                <ExternalLink className="h-3.5 w-3.5" />
                {guide.docsLabel || "Open provider docs"}
              </a>
            )}
          </div>
        )}

        {/* Test result display */}
        {testResult && (
          <div className={`rounded-lg px-3 py-2 text-xs ${
            testResult.healthy
              ? "bg-green-500/10 text-green-300 border border-green-500/20"
              : "bg-red-500/10 text-red-300 border border-red-500/20"
          }`}>
            {testResult.healthy ? <CheckCircle2 className="inline mr-1 h-3 w-3" /> : <AlertCircle className="inline mr-1 h-3 w-3" />}
            {testResult.details}
          </div>
        )}
      </CardContent>
    </Card>
  );
}

function IntegrationSkeleton() {
  return (
    <Card>
      <CardHeader className="flex flex-row items-start justify-between space-y-0 pb-3 pl-5">
        <div className="flex items-center gap-3">
          <Skeleton className="h-10 w-10 rounded-lg" />
          <div>
            <Skeleton className="h-4 w-28" />
            <Skeleton className="mt-1.5 h-3 w-48" />
          </div>
        </div>
        <Skeleton className="h-5 w-20 rounded-full" />
      </CardHeader>
      <CardContent className="flex items-center justify-between pl-5">
        <Skeleton className="h-3 w-32" />
        <Skeleton className="h-8 w-24 rounded-md" />
      </CardContent>
    </Card>
  );
}

/* -------------------------------------------------------------------------- */
/*  Integrations Page                                                         */
/* -------------------------------------------------------------------------- */

export default function SetupPage() {
  const [integrations, setIntegrations] = useState<Integration[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [credentialModal, setCredentialModal] = useState<{
    service: string;
    fields: CredentialField[];
  } | null>(null);
  const [disconnectModal, setDisconnectModal] = useState<string | null>(null);
  const [connectingService, setConnectingService] = useState<string | null>(null);
  const [testResults, setTestResults] = useState<Record<string, { healthy: boolean; details: string }>>(
    {}
  );
  const [testingService, setTestingService] = useState<string | null>(null);

  const addToast = useToastStore((s) => s.addToast);

  const loadIntegrations = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const data = await apiClient.getIntegrations();
      setIntegrations(data);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to load integrations");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadIntegrations();
  }, [loadIntegrations]);

  // Poll for integration status changes (e.g., after OAuth completes in another tab)
  useEffect(() => {
    const interval = setInterval(async () => {
      try {
        const data = await apiClient.getIntegrations();
        setIntegrations((prev) => {
          // Check for newly connected integrations
          for (const integration of data) {
            const prevIntegration = prev.find((i) => i.service === integration.service);
            if (prevIntegration?.status !== "connected" && integration.status === "connected") {
              addToast({
                variant: "success",
                title: `${integration.service.replace(/_/g, " ")} connected`,
                description: "Integration is now active",
              });
            }
          }
          return data;
        });
      } catch {
        // ignore polling errors
      }
    }, 5000);

    return () => clearInterval(interval);
  }, [addToast]);

  const handleConnect = async (service: string) => {
    try {
      setConnectingService(service);
      const result = await apiClient.connectIntegration(service);

      if (result.requires_credentials && result.credential_fields?.length) {
        // Show credential modal
        setCredentialModal({
          service,
          fields: result.credential_fields,
        });
        setConnectingService(null);
        return;
      }

      const authUrl = result.auth_url || result.authUrl;
      if (authUrl) {
        window.open(authUrl, "_blank", "noopener,noreferrer");
        addToast({
          variant: "info",
          title: "Authorization started",
          description: `Complete the authorization in the opened browser tab for ${service.replace(/_/g, " ")}.`,
        });
      } else {
        addToast({ variant: "success", title: "Connected", description: result.message });
        await loadIntegrations();
      }
    } catch (e) {
      addToast({
        variant: "error",
        title: "Connection failed",
        description: e instanceof Error ? e.message : "Failed to connect integration",
      });
    } finally {
      setConnectingService(null);
    }
  };

  const handleCredentialSubmit = async (credentials: Record<string, string>) => {
    if (!credentialModal) return;
    try {
      setConnectingService(credentialModal.service);
      const result = await apiClient.connectIntegration(credentialModal.service, credentials);

      const authUrl = result.auth_url || result.authUrl;
      if (authUrl) {
        // OAuth flow after providing client credentials
        window.open(authUrl, "_blank", "noopener,noreferrer");
        addToast({
          variant: "info",
          title: "Authorization started",
          description: "Complete the authorization in the opened browser tab.",
        });
      } else {
        addToast({
          variant: "success",
          title: "Connected!",
          description: result.message || `${credentialModal.service.replace(/_/g, " ")} connected successfully`,
        });
      }
      setCredentialModal(null);
      await loadIntegrations();
    } catch (e) {
      addToast({
        variant: "error",
        title: "Connection failed",
        description: e instanceof Error ? e.message : "Failed to connect",
      });
    } finally {
      setConnectingService(null);
    }
  };

  const handleDisconnect = async (service: string) => {
    setDisconnectModal(service);
  };

  const handleConfirmDisconnect = async () => {
    if (!disconnectModal) return;
    try {
      setConnectingService(disconnectModal);
      await apiClient.disconnectIntegration(disconnectModal);
      addToast({
        variant: "success",
        title: "Disconnected",
        description: `${disconnectModal.replace(/_/g, " ")} has been disconnected`,
      });
      // Clear test results for disconnected service
      setTestResults((prev) => {
        const next = { ...prev };
        delete next[disconnectModal];
        return next;
      });
      await loadIntegrations();
    } catch (e) {
      addToast({
        variant: "error",
        title: "Disconnect failed",
        description: e instanceof Error ? e.message : "Failed to disconnect",
      });
    } finally {
      setDisconnectModal(null);
      setConnectingService(null);
    }
  };

  const handleTest = async (service: string) => {
    try {
      setTestingService(service);
      const result = await apiClient.testIntegration(service);
      setTestResults((prev) => ({ ...prev, [service]: result }));
      addToast({
        variant: result.healthy ? "success" : "warning",
        title: result.healthy ? "Test passed" : "Test failed",
        description: result.details,
      });
    } catch (e) {
      const failResult = {
        healthy: false,
        details: e instanceof Error ? e.message : "Test failed",
      };
      setTestResults((prev) => ({ ...prev, [service]: failResult }));
      addToast({
        variant: "error",
        title: "Test failed",
        description: failResult.details,
      });
    } finally {
      setTestingService(null);
    }
  };

  const connected = integrations.filter((i) => i.status === "connected");
  const available = integrations.filter((i) => i.status !== "connected");

  // Group available by category
  const categorize = (list: Integration[]) => {
    const groups: Record<string, Integration[]> = {};
    for (const i of list) {
      const cat = SERVICE_META[i.service]?.category || "other";
      (groups[cat] = groups[cat] || []).push(i);
    }
    // Inside each group, sort by release tier (1 → 2 → 3 → unset)
    for (const key of Object.keys(groups)) {
      groups[key].sort((a, b) => {
        const ta = SERVICE_META[a.service]?.tier ?? 99;
        const tb = SERVICE_META[b.service]?.tier ?? 99;
        return ta - tb;
      });
    }
    return groups;
  };

  const categoryLabels: Record<string, string> = {
    social: "Social Media",
    messaging: "Messaging",
    productivity: "Productivity",
    developer: "Developer",
    publishing: "Publishing & Blogs",
    other: "Other",
  };

  const availableGroups = categorize(available);

  return (
    <div className="mx-auto max-w-4xl space-y-8 p-6">
      {/* Page header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold text-foreground">Integrations</h1>
          <p className="mt-1 text-sm text-foreground-secondary">
            Connect your services to unlock the full power of the agent.
          </p>
        </div>
        <Button variant="outline" size="sm" onClick={loadIntegrations} disabled={loading}>
          <RefreshCw className={`mr-2 h-4 w-4 ${loading ? "animate-spin" : ""}`} />
          Refresh
        </Button>
      </div>

      {error && (
        <Card className="border-red-500/50 bg-red-500/10">
          <CardContent className="flex items-center gap-3 py-3">
            <AlertCircle className="h-5 w-5 text-accent-error" />
            <p className="text-sm text-red-300">{error}</p>
            <Button size="sm" variant="ghost" className="ml-auto" onClick={loadIntegrations}>
              Retry
            </Button>
          </CardContent>
        </Card>
      )}

      {/* Connected section */}
      {connected.length > 0 && (
        <section>
          <h2 className="mb-4 flex items-center gap-2 text-sm font-semibold uppercase tracking-wider text-foreground-muted">
            <CheckCircle2 className="h-4 w-4 text-accent-success" />
            Connected ({connected.length})
          </h2>
          <div className="sm:columns-1 lg:columns-2 lg:gap-4 [column-fill:_balance]">
            {connected.map((integration) => (
              <div key={integration.service} className="break-inside-avoid mb-4">
                <IntegrationCard
                  integration={integration}
                  onConnect={handleConnect}
                  onDisconnect={handleDisconnect}
                  onTest={handleTest}
                  testResult={testResults[integration.service]}
                  testing={testingService === integration.service}
                />
              </div>
            ))}
          </div>
        </section>
      )}

      {/* Available section — grouped by category */}
      {Object.entries(availableGroups).map(([category, items]) => (
        <section key={category}>
          <h2 className="mb-4 flex items-center gap-2 text-sm font-semibold uppercase tracking-wider text-foreground-muted">
            <Zap className="h-4 w-4 text-amber-400" />
            {categoryLabels[category] || category} ({items.length})
          </h2>
          <div className="sm:columns-1 lg:columns-2 lg:gap-4 [column-fill:_balance]">
            {loading
              ? Array.from({ length: 2 }).map((_, i) => (
                  <div key={i} className="break-inside-avoid mb-4">
                    <IntegrationSkeleton />
                  </div>
                ))
              : items.map((integration) => (
                  <div key={integration.service} className="break-inside-avoid mb-4">
                    <IntegrationCard
                      integration={integration}
                      onConnect={handleConnect}
                      onDisconnect={handleDisconnect}
                      onTest={handleTest}
                      testResult={testResults[integration.service]}
                      testing={testingService === integration.service}
                    />
                  </div>
                ))}
          </div>
        </section>
      ))}

      {!loading && integrations.length === 0 && !error && (
        <Card className="py-12 text-center">
          <CardContent>
            <Globe className="mx-auto h-12 w-12 text-foreground-muted" />
            <p className="mt-4 text-lg font-medium text-foreground">No integrations found</p>
            <p className="mt-1 text-sm text-foreground-secondary">
              The backend doesn&apos;t have any integrations configured yet.
            </p>
          </CardContent>
        </Card>
      )}

      {/* Credential Modal */}
      {credentialModal && (
        <CredentialModal
          service={credentialModal.service}
          fields={credentialModal.fields}
          onSubmit={handleCredentialSubmit}
          onClose={() => setCredentialModal(null)}
          loading={connectingService === credentialModal.service}
        />
      )}

      {/* Disconnect Confirmation Modal */}
      {disconnectModal && (
        <DisconnectModal
          service={disconnectModal}
          onConfirm={handleConfirmDisconnect}
          onClose={() => setDisconnectModal(null)}
          loading={connectingService === disconnectModal}
        />
      )}
    </div>
  );
}
