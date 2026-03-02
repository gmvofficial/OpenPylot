import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";

const badgeVariants = cva(
  "inline-flex items-center gap-1 rounded-full px-2.5 py-0.5 text-xs font-medium transition-colors",
  {
    variants: {
      variant: {
        default: "bg-background-tertiary text-foreground-secondary",
        secondary: "bg-background-tertiary text-foreground-secondary border border-border",
        outline: "border border-border text-foreground-secondary bg-transparent",
        success: "bg-accent-success/15 text-accent-success",
        warning: "bg-accent-warning/15 text-accent-warning",
        error: "bg-accent-error/15 text-accent-error",
        destructive: "bg-accent-error/15 text-accent-error",
        info: "bg-accent-info/15 text-accent-info",
        accent: "bg-accent/15 text-accent",
      },
    },
    defaultVariants: {
      variant: "default",
    },
  }
);

export interface BadgeProps
  extends React.HTMLAttributes<HTMLSpanElement>,
    VariantProps<typeof badgeVariants> {}

export function Badge({ className, variant, ...props }: BadgeProps) {
  return <span className={cn(badgeVariants({ variant }), className)} {...props} />;
}
