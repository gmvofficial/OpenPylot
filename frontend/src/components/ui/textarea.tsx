import * as React from "react";
import { cn } from "@/lib/utils";

export interface TextareaProps extends React.TextareaHTMLAttributes<HTMLTextAreaElement> {}

export const Textarea = React.forwardRef<HTMLTextAreaElement, TextareaProps>(
  ({ className, ...props }, ref) => (
    <textarea
      className={cn(
        "flex w-full rounded-lg border border-border bg-background-input px-3 py-2 text-sm text-foreground placeholder:text-foreground-muted transition-colors resize-none",
        "focus:outline-none focus:ring-2 focus:ring-accent/50 focus:border-accent/50",
        "disabled:cursor-not-allowed disabled:opacity-50",
        "scrollbar-thin",
        className
      )}
      ref={ref}
      {...props}
    />
  )
);
Textarea.displayName = "Textarea";
