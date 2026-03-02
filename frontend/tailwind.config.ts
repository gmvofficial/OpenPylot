import type { Config } from "tailwindcss";
import typography from "@tailwindcss/typography";

const config: Config = {
  darkMode: "class",
  content: ["./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        background: "var(--bg-primary)",
        "background-secondary": "var(--bg-secondary)",
        "background-tertiary": "var(--bg-tertiary)",
        "background-input": "var(--bg-input)",
        foreground: "var(--text-primary)",
        "foreground-secondary": "var(--text-secondary)",
        "foreground-muted": "var(--text-muted)",
        accent: {
          DEFAULT: "var(--accent-primary)",
          success: "var(--accent-success)",
          warning: "var(--accent-warning)",
          error: "var(--accent-error)",
          info: "var(--accent-info)",
        },
        border: "var(--border)",
        "border-hover": "var(--border-hover)",
      },
      fontFamily: {
        sans: ["Inter", "system-ui", "sans-serif"],
        mono: ["JetBrains Mono", "Fira Code", "monospace"],
      },
      width: {
        sidebar: "var(--sidebar-width)",
        "side-panel": "var(--side-panel-width)",
      },
      height: {
        header: "var(--header-height)",
      },
      maxWidth: {
        chat: "var(--chat-max-width)",
      },
      keyframes: {
        "fade-in": {
          from: { opacity: "0", transform: "translateY(4px)" },
          to: { opacity: "1", transform: "translateY(0)" },
        },
        "slide-in-right": {
          from: { transform: "translateX(100%)" },
          to: { transform: "translateX(0)" },
        },
        pulse: {
          "0%, 100%": { opacity: "1" },
          "50%": { opacity: "0.5" },
        },
        "thinking-dot": {
          "0%, 80%, 100%": { transform: "scale(0)" },
          "40%": { transform: "scale(1)" },
        },
      },
      animation: {
        "fade-in": "fade-in 0.2s ease-out",
        "slide-in-right": "slide-in-right 0.3s ease-out",
        pulse: "pulse 2s cubic-bezier(0.4, 0, 0.6, 1) infinite",
        "thinking-dot": "thinking-dot 1.4s ease-in-out infinite",
      },
    },
  },
  plugins: [typography],
};

export default config;
