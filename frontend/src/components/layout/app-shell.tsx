"use client";

import * as React from "react";
import { usePathname, useRouter } from "next/navigation";
import { Sidebar } from "./sidebar";
import { Header } from "./header";
import { useChatStore } from "@/stores/chat";
import { useNotificationStore } from "@/stores/notifications";
import { useAppStore } from "@/stores/app";
import { useToastStore } from "@/stores/toast";
import { Toast, ToastContainer } from "@/components/ui/toast";
import { authHeaders, getToken, initToken } from "@/lib/token";

function ToastLayer() {
  const { toasts, dismiss } = useToastStore();
  return (
    <ToastContainer>
      {toasts.map((t) => (
        <Toast
          key={t.id}
          id={t.id}
          title={t.title}
          description={t.description}
          variant={t.variant}
          onDismiss={dismiss}
        />
      ))}
    </ToastContainer>
  );
}

export function AppShell({ children }: { children: React.ReactNode }) {
  const pathname = usePathname();
  const router = useRouter();
  const [setupChecked, setSetupChecked] = React.useState(false);
  const [tokenReady, setTokenReady] = React.useState(false);
  const [missingToken, setMissingToken] = React.useState(false);

  React.useEffect(() => {
    // Capture `?token=…` from the launch URL and persist it BEFORE anything
    // connects — every API call and WebSocket upgrade below needs it.
    initToken();
    setTokenReady(true);

    // Connect WebSockets and fetch initial data
    useChatStore.getState().connect();
    useNotificationStore.getState().connect();
    useChatStore.getState().loadConversations();
    useAppStore.getState().fetchStatus();

    // Periodically refresh status
    const interval = setInterval(() => useAppStore.getState().fetchStatus(), 30000);

    return () => {
      useChatStore.getState().disconnect();
      useNotificationStore.getState().disconnect();
      clearInterval(interval);
    };
  }, []); // Run once on mount — Zustand actions are stable

  // Check if first-run setup is needed
  React.useEffect(() => {
    if (setupChecked) return;
    if (pathname?.startsWith("/setup/wizard")) {
      setSetupChecked(true);
      return;
    }

    if (!tokenReady) return;

    (async () => {
      try {
        const res = await fetch(`${window.location.origin}/api/setup/status`, { headers: authHeaders() });
        if (res.status === 401) {
          setMissingToken(true);
          return;
        }
        if (res.ok) {
          const json = await res.json();
          const data = json.data ?? json;
          if (!data.llm_configured) {
            router.replace("/setup/wizard");
          }
        }
      } catch {
        // Backend not ready yet — skip setup check
      } finally {
        setSetupChecked(true);
      }
    })();
  }, [pathname, router, setupChecked, tokenReady]);

  // Show notifications as toasts
  React.useEffect(() => {
    const unsubscribe = useNotificationStore.subscribe((state, prevState) => {
      if (state.notifications.length > prevState.notifications.length) {
        const newest = state.notifications[0];
        if (newest && !newest.read) {
          useToastStore.getState().addToast({
            title: newest.title,
            description: newest.message,
            variant: newest.type === "error" ? "error" : newest.type === "integration_connected" ? "success" : "info",
          });
        }
      }
    });
    return unsubscribe;
  }, []);

  // The server rejected us for lack of a credential. Nothing in the app will
  // work until the user supplies one, so say exactly how instead of leaving
  // every panel stuck on an empty state.
  if (missingToken) {
    return <TokenGate hasStoredToken={getToken() !== null} />;
  }

  return (
    <div className="flex h-screen w-screen overflow-hidden">
      <Sidebar />
      <div className="flex flex-col flex-1 min-w-0">
        <Header />
        <main className="flex-1 min-h-0 overflow-y-auto scrollbar-thin">{children}</main>
      </div>
      <ToastLayer />
    </div>
  );
}

function TokenGate({ hasStoredToken }: { hasStoredToken: boolean }) {
  const [value, setValue] = React.useState("");

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = value.trim();
    if (!trimmed) return;
    // A full launch URL is easier to paste than the bare token, so accept both.
    const token = trimmed.includes("token=")
      ? new URLSearchParams(trimmed.split("?")[1] ?? "").get("token") ?? trimmed
      : trimmed;
    window.location.href = `${window.location.origin}${window.location.pathname}?token=${encodeURIComponent(token)}`;
  };

  return (
    <div className="flex h-screen w-screen items-center justify-center bg-background px-4">
      <div className="w-full max-w-md">
        <h1 className="text-lg font-semibold text-foreground">
          {hasStoredToken ? "That access token is no longer valid" : "This OpenPylot needs an access token"}
        </h1>
        <p className="mt-2 text-sm text-foreground-secondary">
          {hasStoredToken
            ? "The token was rotated or the server was reinstalled. Get the current one and paste it below."
            : "The local server only answers requests that carry its token, so nothing on your machine can drive the agent without your say-so."}
        </p>
        <div className="mt-4 rounded-lg bg-background-tertiary px-3 py-2">
          <code className="font-mono text-xs text-accent-info">pylot token --url</code>
        </div>
        <form onSubmit={submit} className="mt-4 flex gap-2">
          <input
            id="pylot-token-input"
            value={value}
            onChange={(e) => setValue(e.target.value)}
            placeholder="Paste the token or the full URL"
            autoComplete="off"
            spellCheck={false}
            className="flex-1 rounded-lg border border-border bg-background-input px-3 py-2 font-mono text-sm text-foreground outline-none focus:border-accent/40 focus:ring-2 focus:ring-accent/30"
          />
          <button
            type="submit"
            disabled={!value.trim()}
            className="rounded-lg bg-accent px-4 py-2 text-sm font-medium text-white disabled:opacity-40"
          >
            Connect
          </button>
        </form>
      </div>
    </div>
  );
}
