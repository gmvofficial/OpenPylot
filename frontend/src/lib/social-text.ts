/**
 * Strip markdown formatting → clean plain text.
 *
 * Mirror of the Rust `crate::social::strip_markdown` helper. Used by the
 * compose UI before sending content to the backend so what the user sees
 * in the textarea is exactly what gets posted to LinkedIn / X / Threads.
 */
export function stripMarkdown(input: string): string {
  if (!input) return "";

  let s = input;

  // Drop images (LinkedIn etc. need a real upload).
  s = s.replace(/!\[[^\]]*\]\(([^)]+)\)/g, "");

  // Fenced code blocks → keep inner text only.
  s = s.replace(/```[a-zA-Z0-9_+-]*\n?([\s\S]*?)```/g, "$1");

  // Inline code → unwrap.
  s = s.replace(/`([^`]+)`/g, "$1");

  // Links → "label (url)".
  s = s.replace(/\[([^\]]+)\]\(([^)]+)\)/g, "$1 ($2)");

  // Bold / strike.
  s = s.replace(/\*\*([^*]+?)\*\*/g, "$1");
  s = s.replace(/__([^_]+?)__/g, "$1");
  s = s.replace(/~~([^~]+?)~~/g, "$1");

  // Italic — must run AFTER bold so we don't eat one side of `**word**`.
  s = s.replace(/(^|[^*])\*([^*\n]+?)\*(?!\*)/g, "$1$2");
  s = s.replace(/(^|[^_])_([^_\n]+?)_(?!_)/g, "$1$2");

  // Headings.
  s = s.replace(/^\s{0,3}#{1,6}\s+/gm, "");

  // Blockquotes.
  s = s.replace(/^\s{0,3}>\s?/gm, "");

  // Bullet lists → unicode bullet.
  s = s.replace(/^\s*[-*+]\s+/gm, "• ");

  // Ordered lists — drop the "1. " prefix.
  s = s.replace(/^\s*\d+\.\s+/gm, "");

  // Horizontal rules.
  s = s.replace(/^[-*_]{3,}\s*$/gm, "");

  // Collapse 3+ blank lines → 2.
  s = s.replace(/\n{3,}/g, "\n\n");

  return s.trim();
}

/**
 * Convert a LinkedIn UGC post URN (`urn:li:share:7…` or
 * `urn:li:ugcPost:7…`) into a public browser URL.
 * Returns `null` when the input isn't a recognisable URN.
 */
export function linkedinPostUrl(urnOrId: string | null | undefined): string | null {
  if (!urnOrId) return null;
  const s = urnOrId.trim();
  if (s.startsWith("urn:li:share:") || s.startsWith("urn:li:ugcPost:")) {
    return `https://www.linkedin.com/feed/update/${s}/`;
  }
  if (/^\d+$/.test(s)) {
    return `https://www.linkedin.com/feed/update/urn:li:share:${s}/`;
  }
  return null;
}
