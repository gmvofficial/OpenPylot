"use client";

import * as React from "react";
import { Monitor, Moon, Sun } from "lucide-react";
import { apply, getTheme, setTheme, watchSystem, type Theme } from "@/lib/theme";
import { cn } from "@/lib/utils";

const OPTIONS: { value: Theme; label: string; Icon: typeof Sun }[] = [
  { value: "light", label: "Light", Icon: Sun },
  { value: "dark", label: "Dark", Icon: Moon },
  { value: "system", label: "System", Icon: Monitor },
];

/**
 * Three-state theme switch.
 *
 * A two-state toggle cannot express "follow my system", which is what most
 * people actually want — so all three are shown as a segmented control rather
 * than cycled through by a single button, where the current state is a guess.
 */
export function ThemeToggle() {
  // Start at "system" and correct after mount: the server has no idea what the
  // browser stored, and rendering the wrong one first causes a hydration
  // mismatch.
  const [theme, setLocal] = React.useState<Theme>("system");

  React.useEffect(() => {
    setLocal(getTheme());
  }, []);

  // While the preference is "system", follow the OS as it changes — otherwise
  // the app stays light after the desktop switches to dark at sunset.
  React.useEffect(() => {
    if (theme !== "system") return;
    return watchSystem(() => apply("system"));
  }, [theme]);

  const choose = (next: Theme) => {
    setLocal(next);
    setTheme(next);
  };

  return (
    <div
      role="radiogroup"
      aria-label="Colour theme"
      className="flex items-center gap-0.5 rounded-lg border border-border bg-background-tertiary p-0.5"
    >
      {OPTIONS.map(({ value, label, Icon }) => (
        <button
          key={value}
          type="button"
          role="radio"
          aria-checked={theme === value}
          aria-label={label}
          title={label}
          onClick={() => choose(value)}
          className={cn(
            "rounded-md p-1.5 transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/50",
            theme === value
              ? "bg-background text-foreground shadow-sm"
              : "text-foreground-muted hover:text-foreground"
          )}
        >
          <Icon className="h-3.5 w-3.5" />
        </button>
      ))}
    </div>
  );
}
