"use client";

import * as React from "react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import {
  MessageSquare,
  Settings2,
  BookOpen,
  LayoutDashboard,
  ChevronLeft,
  ChevronRight,
  Plus,
  Search,
  Trash2,
  Bot,
  Share2,
  BarChart3,
  Settings,
  Wrench,
  Brain,
  Users,
} from "lucide-react";
import { cn, truncate, formatRelativeTime } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Separator } from "@/components/ui/separator";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Tooltip } from "@/components/ui/tooltip";
import { useChatStore } from "@/stores/chat";
import { useAppStore } from "@/stores/app";

const navItems = [
  { href: "/chat", label: "Chat", icon: MessageSquare },
  { href: "/social", label: "Social Media", icon: Share2 },
  { href: "/agents", label: "Sub-Agents", icon: Users },
  { href: "/memory", label: "Memory", icon: Brain },
  { href: "/setup", label: "Integrations", icon: Settings2 },
  { href: "/knowledge", label: "Knowledge Base", icon: BookOpen },
  { href: "/tools", label: "Tools & Skills", icon: Wrench },
  { href: "/dashboard", label: "Dashboard", icon: LayoutDashboard },
  { href: "/settings", label: "Settings", icon: Settings },
];

export function Sidebar() {
  const pathname = usePathname();
  const { sidebarCollapsed, toggleSidebar, status } = useAppStore();
  const {
    conversations,
    activeConversationId,
    loadConversation,
    newConversation,
    deleteConversation,
  } = useChatStore();
  const [searchQuery, setSearchQuery] = React.useState("");

  const filteredConversations = conversations.filter((c) =>
    c.title.toLowerCase().includes(searchQuery.toLowerCase())
  );

  return (
    <aside
      className={cn(
        "flex flex-col h-full bg-background-secondary border-r border-border transition-all duration-300",
        sidebarCollapsed ? "w-16" : "w-sidebar"
      )}
    >
      {/* Logo / Brand */}
      <div className="flex items-center gap-3 h-header px-4 shrink-0">
        <div className="flex items-center justify-center w-8 h-8 rounded-lg bg-accent/10">
          <Bot className="w-5 h-5 text-accent" />
        </div>
        {!sidebarCollapsed && (
          <div className="flex-1 min-w-0">
            <h1 className="text-sm font-semibold text-foreground truncate">
              OpenPylot
            </h1>
            {status && (
              <p className="text-[10px] text-foreground-muted truncate">
                {status.online ? "🟢" : "🔴"} {status.model}
              </p>
            )}
          </div>
        )}
        <Button
          variant="ghost"
          size="icon-sm"
          onClick={toggleSidebar}
          className="shrink-0"
        >
          {sidebarCollapsed ? (
            <ChevronRight className="w-4 h-4" />
          ) : (
            <ChevronLeft className="w-4 h-4" />
          )}
        </Button>
      </div>

      <Separator />

      {/* Navigation */}
      <nav className="p-2 space-y-1">
        {navItems.map(({ href, label, icon: Icon }) => {
          const isActive =
            pathname === href || pathname?.startsWith(href + "/");
          const link = (
            <Link
              key={href}
              href={href}
              className={cn(
                "flex items-center gap-3 rounded-lg px-3 py-2 text-sm transition-colors",
                isActive
                  ? "bg-accent/10 text-accent font-medium"
                  : "text-foreground-secondary hover:text-foreground hover:bg-background-tertiary"
              )}
            >
              <Icon className="w-4 h-4 shrink-0" />
              {!sidebarCollapsed && <span>{label}</span>}
            </Link>
          );
          return sidebarCollapsed ? (
            <Tooltip key={href} content={label} side="right">
              {link}
            </Tooltip>
          ) : (
            link
          );
        })}
      </nav>

      <Separator className="mx-2" />

      {/* Conversations (only when on chat page and sidebar expanded) */}
      {!sidebarCollapsed && pathname?.startsWith("/chat") && (
        <div className="flex flex-col flex-1 min-h-0 p-2">
          <div className="flex items-center justify-between mb-2 px-1">
            <span className="text-xs font-medium text-foreground-muted uppercase tracking-wider">
              Conversations
            </span>
            <Tooltip content="New conversation" side="top">
              <Button
                variant="ghost"
                size="icon-sm"
                onClick={newConversation}
              >
                <Plus className="w-3.5 h-3.5" />
              </Button>
            </Tooltip>
          </div>

          {conversations.length > 5 && (
            <div className="relative mb-2">
              <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-foreground-muted" />
              <Input
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                placeholder="Search..."
                className="h-7 pl-8 text-xs"
              />
            </div>
          )}

          <ScrollArea className="flex-1 -mx-1">
            <div className="space-y-0.5 px-1">
              {filteredConversations.map((convo) => (
                <div
                  key={convo.id}
                  className={cn(
                    "group flex items-center gap-2 rounded-lg px-2.5 py-2 cursor-pointer transition-colors",
                    activeConversationId === convo.id
                      ? "bg-background-tertiary text-foreground"
                      : "text-foreground-secondary hover:bg-background-tertiary hover:text-foreground"
                  )}
                  onClick={() => loadConversation(convo.id)}
                >
                  <MessageSquare className="w-3.5 h-3.5 shrink-0 opacity-50" />
                  <div className="flex-1 min-w-0">
                    <p className="text-xs font-medium truncate">
                      {truncate(convo.title, 28)}
                    </p>
                    <p className="text-[10px] text-foreground-muted">
                      {formatRelativeTime(convo.updatedAt)}
                    </p>
                  </div>
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      deleteConversation(convo.id);
                    }}
                    className="opacity-0 group-hover:opacity-100 transition-opacity"
                  >
                    <Trash2 className="w-3 h-3 text-foreground-muted hover:text-accent-error" />
                  </button>
                </div>
              ))}
              {filteredConversations.length === 0 && conversations.length > 0 && (
                <p className="text-xs text-foreground-muted text-center py-4">
                  No matching conversations
                </p>
              )}
              {conversations.length === 0 && (
                <p className="text-xs text-foreground-muted text-center py-4">
                  No conversations yet
                </p>
              )}
            </div>
          </ScrollArea>
        </div>
      )}

      {/* Spacer when conversations not shown */}
      {(sidebarCollapsed || !pathname?.startsWith("/chat")) && (
        <div className="flex-1" />
      )}

      <Separator className="mx-2" />

      {/* Bottom: Settings link */}
      <nav className="p-2">
        {(() => {
          const settingsLink = (
            <Link
              href="/settings"
              className={cn(
                "flex items-center gap-3 rounded-lg px-3 py-2 text-sm transition-colors",
                pathname?.startsWith("/settings")
                  ? "bg-accent/10 text-accent font-medium"
                  : "text-foreground-secondary hover:text-foreground hover:bg-background-tertiary"
              )}
            >
              <Settings2 className="w-4 h-4 shrink-0" />
              {!sidebarCollapsed && <span>Settings</span>}
            </Link>
          );
          return sidebarCollapsed ? (
            <Tooltip content="Settings" side="right">
              {settingsLink}
            </Tooltip>
          ) : (
            settingsLink
          );
        })()}
      </nav>

      {/* Status bar */}
      {!sidebarCollapsed && status && (
        <div className="px-4 py-2.5 border-t border-border bg-background text-[10px] text-foreground-muted">
          <div className="flex items-center gap-1.5">
            <span className={status.online ? "text-accent-success" : "text-accent-error"}>●</span>
            <span>{status.online ? "Online" : "Offline"}</span>
            <span className="mx-1">│</span>
            <span>{status.model}</span>
            <span className="mx-1">│</span>
            <span>{status.active_integrations ?? status.integrationCount ?? 0} integrations</span>
          </div>
        </div>
      )}
    </aside>
  );
}
