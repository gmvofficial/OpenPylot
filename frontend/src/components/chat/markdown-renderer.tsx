"use client";

import * as React from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { cn } from "@/lib/utils";

interface MarkdownRendererProps {
  content: string;
  className?: string;
}

export function MarkdownRenderer({ content, className }: MarkdownRendererProps) {
  return (
    <div className={cn("prose-chat", className)}>
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
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
        {content}
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
