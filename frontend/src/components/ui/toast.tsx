"use client";

import * as React from "react";
import { cn } from "@/lib/utils";
import { X } from "lucide-react";

interface ToastProps {
  id: string;
  title: string;
  description?: string;
  variant?: "default" | "success" | "error" | "warning" | "info";
  onDismiss: (id: string) => void;
}

export function Toast({ id, title, description, variant = "default", onDismiss }: ToastProps) {
  const variantStyles = {
    default: "border-border",
    success: "border-accent-success/30",
    error: "border-accent-error/30",
    warning: "border-accent-warning/30",
    info: "border-accent-info/30",
  };

  const iconMap = {
    default: "📋",
    success: "✅",
    error: "❌",
    warning: "⚠️",
    info: "ℹ️",
  };

  React.useEffect(() => {
    const timer = setTimeout(() => onDismiss(id), 5000);
    return () => clearTimeout(timer);
  }, [id, onDismiss]);

  return (
    <div
      className={cn(
        "animate-slide-in-right pointer-events-auto w-80 rounded-lg border bg-background-secondary p-4 shadow-lg",
        variantStyles[variant]
      )}
    >
      <div className="flex items-start gap-3">
        <span className="text-base">{iconMap[variant]}</span>
        <div className="flex-1 min-w-0">
          <p className="text-sm font-medium text-foreground">{title}</p>
          {description && (
            <p className="mt-1 text-sm text-foreground-secondary">{description}</p>
          )}
        </div>
        <button
          onClick={() => onDismiss(id)}
          className="text-foreground-muted hover:text-foreground transition-colors"
        >
          <X className="h-4 w-4" />
        </button>
      </div>
    </div>
  );
}

export function ToastContainer({ children }: { children: React.ReactNode }) {
  return (
    <div className="fixed bottom-4 right-4 z-50 flex flex-col gap-2 pointer-events-none">
      {children}
    </div>
  );
}
