/**
 * Theme handling: light, dark, or follow the system.
 *
 * The app was hardcoded to dark — `<html className="dark">` was a literal and
 * `color-scheme: dark` was fixed — so a user on a light desktop got a dark app
 * with no way out.
 *
 * Three states, not two. "System" is the default and must stamp *nothing* on
 * the root element, so `prefers-color-scheme` decides; an explicit choice
 * stamps `light` or `dark`, which beats the OS in both directions.
 */

export type Theme = "system" | "light" | "dark";

const STORAGE_KEY = "pylot.theme";

/** The resolved appearance — what is actually on screen. */
export type Resolved = "light" | "dark";

/** Read the saved preference, defaulting to following the system. */
export function getTheme(): Theme {
  if (typeof window === "undefined") return "system";
  try {
    const saved = window.localStorage.getItem(STORAGE_KEY);
    if (saved === "light" || saved === "dark" || saved === "system") return saved;
  } catch {
    /* storage blocked — fall through to the default */
  }
  return "system";
}

/** What the OS currently prefers. */
export function systemTheme(): Resolved {
  if (typeof window === "undefined" || !window.matchMedia) return "light";
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

/** What a preference resolves to right now. */
export function resolve(theme: Theme): Resolved {
  return theme === "system" ? systemTheme() : theme;
}

/**
 * Stamp the root element for `theme`.
 *
 * "System" removes both classes rather than adding one, so the CSS media query
 * is what decides — adding a class here would freeze the app at whatever the OS
 * happened to prefer when the page loaded.
 */
export function apply(theme: Theme): void {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  root.classList.remove("light", "dark");
  if (theme !== "system") {
    root.classList.add(theme);
  }
}

/** Save and apply a preference. */
export function setTheme(theme: Theme): void {
  apply(theme);
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(STORAGE_KEY, theme);
  } catch {
    /* in-memory only for this session */
  }
}

/**
 * Watch for OS theme changes while the preference is "system".
 *
 * Without this, switching the OS to dark at night leaves the app light until
 * the page is reloaded. Returns an unsubscribe function.
 */
export function watchSystem(onChange: (resolved: Resolved) => void): () => void {
  if (typeof window === "undefined" || !window.matchMedia) return () => {};
  const query = window.matchMedia("(prefers-color-scheme: dark)");
  const handler = (e: MediaQueryListEvent) => onChange(e.matches ? "dark" : "light");
  query.addEventListener("change", handler);
  return () => query.removeEventListener("change", handler);
}

/**
 * Inline script that stamps the theme before the first paint.
 *
 * React cannot do this: it runs after hydration, so the page would render in
 * the default theme and then visibly snap to the chosen one. This has to be a
 * blocking script in the document head.
 */
export const BOOT_SCRIPT = `(function(){try{var t=localStorage.getItem(${JSON.stringify(
  STORAGE_KEY
)});if(t==="light"||t==="dark"){document.documentElement.classList.add(t)}}catch(e){}})()`;
