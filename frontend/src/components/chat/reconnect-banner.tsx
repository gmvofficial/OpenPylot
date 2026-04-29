"use client";

import { useState } from "react";
import { AlertTriangle, ExternalLink, RefreshCw } from "lucide-react";
import { Card, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { apiClient } from "@/lib/api";
import { useToastStore } from "@/stores/toast";

const SERVICE_LABELS: Record<string, string> = {
  google_calendar: "Google Calendar",
  gmail: "Gmail",
  telegram: "Telegram",
  whatsapp: "WhatsApp",
  github: "GitHub",
  slack: "Slack",
};

/**
 * Inline card rendered inside the chat thread when a tool call fails because
 * the user has no credentials for an integration. Clicking "Reconnect" starts
 * the integration's connect flow (OAuth redirect for Google, credential modal
 * for token-based services).
 *
 * Triggered when the chat store sees a tool error whose payload contains
 * `code === "credentials_missing"` (see Rust `credentials_missing()` helper).
 */
export function ReconnectBanner({
  service,
  message,
}: {
  service: string;
  message?: string;
}) {
  const [busy, setBusy] = useState(false);
  const addToast = useToastStore((s) => s.addToast);
  const label = SERVICE_LABELS[service] ?? service;

  const handleReconnect = async () => {
    try {
      setBusy(true);
      const result = await apiClient.connectIntegration(service);
      if (result.auth_url) {
        // OAuth flow — open Google's consent screen in a new tab.
        window.open(result.auth_url, "_blank", "noopener,noreferrer");
        addToast({
          variant: "info",
          title: "Browser opened",
          description: `Authorise ${label}, then return here and retry your request.`,
        });
      } else if (result.requires_credentials) {
        // Token-based flow — push the user to the integrations page where the
        // credential modal lives.
        window.location.href = `/setup?service=${service}`;
      } else {
        addToast({
          variant: "success",
          title: `${label} connected`,
          description: "You can retry your request now.",
        });
      }
    } catch (e) {
      addToast({
        variant: "error",
        title: "Reconnect failed",
        description: e instanceof Error ? e.message : "Unknown error",
      });
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card className="border-amber-500/40 bg-amber-500/5">
      <CardContent className="flex items-start gap-3 py-3">
        <AlertTriangle className="mt-0.5 h-5 w-5 flex-shrink-0 text-amber-500" />
        <div className="flex-1 space-y-1">
          <div className="text-sm font-medium text-foreground">
            {label} account disconnected
          </div>
          <div className="text-xs text-foreground-secondary">
            {message ??
              `We couldn't reach your ${label} account. Reconnect to continue.`}
          </div>
        </div>
        <Button size="sm" onClick={handleReconnect} disabled={busy}>
          {busy ? (
            <RefreshCw className="mr-2 h-4 w-4 animate-spin" />
          ) : (
            <ExternalLink className="mr-2 h-4 w-4" />
          )}
          Reconnect
        </Button>
      </CardContent>
    </Card>
  );
}
