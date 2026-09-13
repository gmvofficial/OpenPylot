"use client";

import * as React from "react";
import { Send, Paperclip, Loader2, X, FileText, Image as ImageIcon, Square } from "lucide-react";
import { Button } from "@/components/ui/button";
import { useChatStore } from "@/stores/chat";
import { useToastStore } from "@/stores/toast";
import { apiClient } from "@/lib/api";
import { cn, formatBytes } from "@/lib/utils";

/** Largest single upload the backend accepts (DefaultBodyLimit is 100MB). */
const MAX_FILE_BYTES = 100 * 1024 * 1024;

/** An attachment as it moves from picked → uploaded. */
interface Attachment {
  id: string;
  name: string;
  size: number;
  type: string;
  /** Set once the upload finishes; this is what gets sent with the message. */
  url?: string;
  error?: string;
  uploading: boolean;
}

export function ChatInput() {
  const [value, setValue] = React.useState("");
  const [attachments, setAttachments] = React.useState<Attachment[]>([]);
  const [dragging, setDragging] = React.useState(false);
  const textareaRef = React.useRef<HTMLTextAreaElement>(null);
  const fileInputRef = React.useRef<HTMLInputElement>(null);
  // Nested dragenter/dragleave fire constantly; count them so the overlay does
  // not flicker as the pointer crosses child elements.
  const dragDepth = React.useRef(0);

  const sendMessage = useChatStore((s) => s.sendMessage);
  const isStreaming = useChatStore((s) => s.isStreaming);
  const isConnected = useChatStore((s) => s.isConnected);
  const stopStreaming = useChatStore((s) => s.stopStreaming);
  const addToast = useToastStore((s) => s.addToast);

  React.useEffect(() => {
    const textarea = textareaRef.current;
    if (!textarea) return;
    textarea.style.height = "auto";
    textarea.style.height = Math.min(textarea.scrollHeight, 200) + "px";
  }, [value]);

  const upload = React.useCallback(
    async (files: FileList | File[]) => {
      const picked = Array.from(files);
      if (picked.length === 0) return;

      const tooBig = picked.filter((f) => f.size > MAX_FILE_BYTES);
      for (const file of tooBig) {
        addToast({
          title: "File is too large",
          description: `${file.name} is ${formatBytes(file.size)}; the limit is ${formatBytes(
            MAX_FILE_BYTES
          )}.`,
          variant: "error",
        });
      }

      const accepted = picked.filter((f) => f.size <= MAX_FILE_BYTES);
      const pending: Attachment[] = accepted.map((file) => ({
        id: `${file.name}-${file.size}-${Date.now()}-${Math.random()}`,
        name: file.name,
        size: file.size,
        type: file.type,
        uploading: true,
      }));
      setAttachments((current) => [...current, ...pending]);

      await Promise.all(
        accepted.map(async (file, i) => {
          const id = pending[i].id;
          try {
            const result = await apiClient.uploadSocialMedia(file);
            setAttachments((current) =>
              current.map((a) =>
                a.id === id
                  ? { ...a, uploading: false, url: result.url, error: result.error }
                  : a
              )
            );
          } catch (e) {
            setAttachments((current) =>
              current.map((a) =>
                a.id === id
                  ? { ...a, uploading: false, error: e instanceof Error ? e.message : String(e) }
                  : a
              )
            );
          }
        })
      );
    },
    [addToast]
  );

  // Paste an image straight into the composer, the way every chat app works.
  React.useEffect(() => {
    const onPaste = (e: ClipboardEvent) => {
      const files = Array.from(e.clipboardData?.files ?? []);
      if (files.length === 0) return;
      e.preventDefault();
      upload(files);
    };
    const el = textareaRef.current;
    el?.addEventListener("paste", onPaste);
    return () => el?.removeEventListener("paste", onPaste);
  }, [upload]);

  const handleSubmit = () => {
    const trimmed = value.trim();
    const uploaded = attachments.filter((a) => a.url);
    if ((!trimmed && uploaded.length === 0) || isStreaming) return;
    if (attachments.some((a) => a.uploading)) return;

    // Attachment URLs ride along in the message text so the agent can fetch
    // them with the tools it already has, rather than needing a new channel.
    const references = uploaded.map((a) => `\n[Attached: ${a.name}](${a.url})`).join("");
    sendMessage(trimmed + references);

    setValue("");
    setAttachments([]);
    if (textareaRef.current) textareaRef.current.style.height = "auto";
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSubmit();
    }
  };

  const uploading = attachments.some((a) => a.uploading);
  const canSend =
    !isStreaming && !uploading && (value.trim().length > 0 || attachments.some((a) => a.url));

  return (
    <div
      className="relative border-t border-border bg-background px-4 py-4"
      onDragEnter={(e) => {
        e.preventDefault();
        dragDepth.current += 1;
        if (e.dataTransfer.types.includes("Files")) setDragging(true);
      }}
      onDragOver={(e) => e.preventDefault()}
      onDragLeave={(e) => {
        e.preventDefault();
        dragDepth.current -= 1;
        if (dragDepth.current <= 0) setDragging(false);
      }}
      onDrop={(e) => {
        e.preventDefault();
        dragDepth.current = 0;
        setDragging(false);
        if (e.dataTransfer.files.length) upload(e.dataTransfer.files);
      }}
    >
      {dragging && (
        <div className="pointer-events-none absolute inset-2 z-10 flex items-center justify-center rounded-xl border-2 border-dashed border-accent bg-background/90">
          <p className="text-sm font-medium text-accent">Drop to attach</p>
        </div>
      )}

      <div className="mx-auto max-w-chat">
        {attachments.length > 0 && (
          <div className="mb-2 flex flex-wrap gap-2">
            {attachments.map((attachment) => (
              <AttachmentChip
                key={attachment.id}
                attachment={attachment}
                onRemove={() =>
                  setAttachments((current) => current.filter((a) => a.id !== attachment.id))
                }
              />
            ))}
          </div>
        )}

        <div className="flex items-end gap-2 rounded-2xl border border-border bg-background-input px-4 py-3 transition-colors focus-within:border-accent/30 focus-within:ring-2 focus-within:ring-accent/30">
          <input
            id="chat-file-input"
            ref={fileInputRef}
            type="file"
            multiple
            hidden
            onChange={(e) => {
              if (e.target.files) upload(e.target.files);
              // Reset so picking the same file twice still fires onChange.
              e.target.value = "";
            }}
          />
          <Button
            variant="ghost"
            size="icon-sm"
            type="button"
            className="mb-0.5 shrink-0 text-foreground-muted hover:text-foreground"
            title="Attach a file"
            aria-label="Attach a file"
            onClick={() => fileInputRef.current?.click()}
          >
            <Paperclip className="h-4 w-4" />
          </Button>

          <textarea
            id="chat-composer"
            ref={textareaRef}
            value={value}
            onChange={(e) => setValue(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Ask anything, or drop a file…"
            disabled={isStreaming}
            rows={2}
            className={cn(
              "max-h-[200px] min-h-[52px] flex-1 resize-none bg-transparent py-2 text-[15px] text-foreground outline-none placeholder:text-foreground-muted",
              "disabled:opacity-50"
            )}
          />

          {isStreaming ? (
            <Button
              variant="ghost"
              size="icon-sm"
              type="button"
              onClick={stopStreaming}
              className="mb-0.5 shrink-0"
              title="Stop generating"
              aria-label="Stop generating"
            >
              <Square className="h-3.5 w-3.5 fill-current" />
            </Button>
          ) : (
            <Button
              variant={canSend ? "default" : "ghost"}
              size="icon-sm"
              type="button"
              onClick={handleSubmit}
              disabled={!canSend}
              className="mb-0.5 shrink-0"
              title="Send"
              aria-label="Send"
            >
              {uploading ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <Send className="h-4 w-4" />
              )}
            </Button>
          )}
        </div>

        <div className="mt-1.5 flex items-center justify-between px-1 text-[10px] text-foreground-muted">
          <span className="flex items-center gap-1">
            <span className={isConnected ? "text-accent-success" : "text-accent-error"}>●</span>
            {isConnected ? "Connected" : "Disconnected"}
          </span>
          <span>
            {uploading
              ? "Uploading…"
              : isStreaming
                ? "Agent is typing…"
                : "Enter to send · Shift+Enter for a new line"}
          </span>
        </div>
      </div>
    </div>
  );
}

function AttachmentChip({
  attachment,
  onRemove,
}: {
  attachment: Attachment;
  onRemove: () => void;
}) {
  const isImage = attachment.type.startsWith("image/");
  const Icon = isImage ? ImageIcon : FileText;

  return (
    <div
      className={cn(
        "flex items-center gap-2 rounded-lg border px-2.5 py-1.5 text-xs",
        attachment.error
          ? "border-accent-error/40 bg-accent-error/5 text-accent-error"
          : "border-border bg-background-tertiary text-foreground-secondary"
      )}
    >
      {attachment.uploading ? (
        <Loader2 className="h-3.5 w-3.5 shrink-0 animate-spin" />
      ) : (
        <Icon className="h-3.5 w-3.5 shrink-0" />
      )}
      <span className="max-w-[180px] truncate">{attachment.name}</span>
      <span className="shrink-0 text-foreground-muted">
        {attachment.error ?? formatBytes(attachment.size)}
      </span>
      <button
        type="button"
        onClick={onRemove}
        aria-label={`Remove ${attachment.name}`}
        className="shrink-0 text-foreground-muted transition-colors hover:text-foreground"
      >
        <X className="h-3 w-3" />
      </button>
    </div>
  );
}
