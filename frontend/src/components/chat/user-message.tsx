"use client";

import { formatTime } from "@/lib/utils";
import type { Message } from "@/types";

interface UserMessageProps {
  message: Message;
}

export function UserMessage({ message }: UserMessageProps) {
  return (
    <div className="flex justify-end animate-fade-in">
      <div className="max-w-[85%] md:max-w-[70%]">
        <div className="bg-accent/10 rounded-2xl rounded-br-md px-4 py-3">
          <p className="text-[15px] text-foreground whitespace-pre-wrap break-words">
            {message.content}
          </p>
        </div>
        <div className="flex justify-end mt-1 px-1">
          <span className="text-[10px] text-foreground-muted">
            {formatTime(message.timestamp)}
          </span>
        </div>
      </div>
    </div>
  );
}
