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

  React.useEffect(() => {
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

    (async () => {
      try {
        const res = await fetch(`${window.location.origin}/api/setup/status`);
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
  }, [pathname, router, setupChecked]);

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
