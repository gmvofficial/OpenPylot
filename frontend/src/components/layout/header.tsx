"use client";

import * as React from "react";
import { usePathname } from "next/navigation";
import { Bell, Settings } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { useNotificationStore } from "@/stores/notifications";
import Link from "next/link";

const pageTitles: Record<string, string> = {
  "/chat": "Chat",
  "/setup": "Integrations",
  "/knowledge": "Knowledge Base",
  "/dashboard": "Dashboard",
  "/dashboard/jobs": "Scheduled Jobs",
  "/dashboard/logs": "Logs",
  "/settings": "Settings",
  "/settings/memory": "Memory Management",
};

export function Header() {
  const pathname = usePathname();
  const { unreadCount, notifications, markAllRead } = useNotificationStore();
  const [showNotifications, setShowNotifications] = React.useState(false);
  const dropdownRef = React.useRef<HTMLDivElement>(null);

  // Resolve page title
  let title = "GMV Agent";
  for (const [path, t] of Object.entries(pageTitles)) {
    if (pathname === path || pathname?.startsWith(path + "/")) {
      title = t;
      break;
    }
  }

  // Close dropdown on outside click
  React.useEffect(() => {
    function handleClickOutside(e: MouseEvent) {
      if (dropdownRef.current && !dropdownRef.current.contains(e.target as Node)) {
        setShowNotifications(false);
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, []);

  return (
    <header className="flex items-center justify-between h-header px-6 border-b border-border bg-background shrink-0">
      <div>
        <h2 className="text-base font-semibold text-foreground">{title}</h2>
      </div>

      <div className="flex items-center gap-2">
        {/* Notification bell */}
        <div className="relative" ref={dropdownRef}>
          <Button
            variant="ghost"
            size="icon"
            onClick={() => setShowNotifications(!showNotifications)}
            className="relative"
          >
            <Bell className="w-4 h-4" />
            {unreadCount > 0 && (
              <span className="absolute -top-0.5 -right-0.5 flex items-center justify-center w-4 h-4 text-[10px] font-bold text-white bg-accent-error rounded-full">
                {unreadCount > 9 ? "9+" : unreadCount}
              </span>
            )}
          </Button>

          {/* Notification dropdown */}
          {showNotifications && (
            <div className="absolute right-0 top-full mt-2 w-80 max-h-96 overflow-auto bg-background-secondary border border-border rounded-xl shadow-xl z-50 animate-fade-in">
              <div className="flex items-center justify-between px-4 py-3 border-b border-border">
                <span className="text-sm font-medium text-foreground">
                  Notifications
                </span>
                {unreadCount > 0 && (
                  <button
                    onClick={markAllRead}
                    className="text-xs text-accent hover:underline"
                  >
                    Mark all read
                  </button>
                )}
              </div>
              {notifications.length === 0 ? (
                <div className="px-4 py-8 text-center text-sm text-foreground-muted">
                  No notifications
                </div>
              ) : (
                <div className="divide-y divide-border">
                  {notifications.slice(0, 10).map((n) => (
                    <div
                      key={n.id}
                      className={`px-4 py-3 text-sm ${n.read ? "opacity-60" : ""}`}
                    >
                      <p className="font-medium text-foreground">{n.title}</p>
                      <p className="text-xs text-foreground-secondary mt-0.5">
                        {n.message}
                      </p>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>

        {/* Settings shortcut */}
        <Link href="/settings">
          <Button variant="ghost" size="icon">
            <Settings className="w-4 h-4" />
          </Button>
        </Link>
      </div>
    </header>
  );
}
