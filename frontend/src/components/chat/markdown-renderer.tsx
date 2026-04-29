"use client";

import * as React from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import rehypeKatex from "rehype-katex";
import { cn } from "@/lib/utils";

interface MarkdownRendererProps {
  content: string;
  className?: string;
}

/**
 * Agents emit math using LaTeX's *standard* delimiters, which `remark-math`
 * does NOT understand on its own:
 *
 *     \[ \text{TF}(t,d) = \frac{a}{b} \]      ← block math (escaped brackets)
 *     \( \log_2 N \)                          ← inline math (escaped parens)
 *
 * Some models also emit the bare bracket form on its own line:
 *
 *     [ \text{TF}(t,d) = \frac{a}{b} ]        ← block math (bare, line-anchored)
 *
 * We rewrite those into the dollar-delimited form `$$…$$` / `$…$` BEFORE
 * handing off to ReactMarkdown.
 *
 * Important non-goals (these used to break things — don't do them):
 *   - Do NOT rewrite bare `(…)` — that destroys `\left( … \right)` inside
 *     math and any prose like `(see below)`.
 *   - Do NOT rewrite inline `[…]` — that breaks markdown links `[text](url)`.
 *   - Do NOT touch existing `$…$` / `$$…$$` — already valid.
 */
function preprocessBracketMath(input: string): string {
  let out = input;

  // 1. Escaped LaTeX block math:   \[ … \]   →   $$ … $$
  //    Non-greedy, multi-line. Surrounding newlines ensure remark-math
  //    treats it as a block.
  out = out.replace(/\\\[\s*([\s\S]*?)\s*\\\]/g, (_m, inner: string) => `\n$$${inner}$$\n`);

  // 2. Escaped LaTeX inline math:  \( … \)   →   $ … $
  //    `[\s\S]*?` is non-greedy so it stops at the first `\)`.
  out = out.replace(/\\\(\s*([\s\S]*?)\s*\\\)/g, (_m, inner: string) => `$${inner}$`);

  // 3. Bare block math on its own line:   ^[ … ]$   →   $$ … $$
  //    Anchored to start/end of line and gated on containing a LaTeX command
  //    (e.g. `\frac`, `\text`) so ordinary "[ …text… ]" prose is left alone.
  //    /m flag lets `^` and `$` match line boundaries.
  out = out.replace(
    /^[ \t]*\[\s*([\s\S]*?)\s*\][ \t]*$/gm,
    (full, inner: string) =>
      /\\[A-Za-z]+/.test(inner) ? `\n$$${inner}$$\n` : full,
  );

  return out;
}

export function MarkdownRenderer({ content, className }: MarkdownRendererProps) {
  const prepared = React.useMemo(() => preprocessBracketMath(content), [content]);

  return (
    <div className={cn("prose-chat", className)}>
      <ReactMarkdown
        remarkPlugins={[remarkGfm, remarkMath]}
        rehypePlugins={[[rehypeKatex, { strict: false, throwOnError: false }]]}
        components={{
          // Custom code block rendering
          code({ className, children, ...props }) {
            const match = /language-(\w+)/.exec(className || "");
            const isInline = !match && !className;

            if (isInline) {
              return (
                <code
                  className="bg-background-tertiary text-accent-info px-1.5 py-0.5 rounded text-sm font-mono"
                  {...props}
                >
                  {children}
                </code>
              );
            }

            return (
              <div className="relative group my-3">
                {match && (
                  <div className="flex items-center justify-between px-4 py-2 bg-background rounded-t-lg border border-b-0 border-border text-xs text-foreground-muted">
                    <span>{match[1]}</span>
                    <CopyButton
                      text={String(children).replace(/\n$/, "")}
                    />
                  </div>
                )}
                <pre
                  className={cn(
                    "bg-background-tertiary p-4 overflow-x-auto text-sm font-mono border border-border",
                    match ? "rounded-b-lg rounded-t-none" : "rounded-lg"
                  )}
                >
                  <code className={className} {...props}>
                    {children}
                  </code>
                </pre>
              </div>
            );
          },
          // Make links open in new tab
          a({ href, children, ...props }) {
            return (
              <a
                href={href}
                target="_blank"
                rel="noopener noreferrer"
                className="text-accent underline underline-offset-2 hover:text-accent/80 transition-colors"
                {...props}
              >
                {children}
              </a>
            );
          },
        }}
      >
        {prepared}
      </ReactMarkdown>
    </div>
  );
}

/** Small copy button for code blocks */
function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = React.useState(false);

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // Fallback
    }
  };

  return (
    <button
      onClick={handleCopy}
      className="text-xs text-foreground-muted hover:text-foreground transition-colors"
    >
      {copied ? "Copied!" : "Copy"}
    </button>
  );
}
