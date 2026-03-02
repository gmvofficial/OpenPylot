"use client";

import { MessageList } from "./message-list";
import { ChatInput } from "./chat-input";
import { SidePanel } from "./side-panel";

export function ChatPage() {
  return (
    <div className="flex h-full overflow-hidden">
      {/* Main chat area */}
      <div className="flex flex-col flex-1 min-w-0 h-full">
        <MessageList />
        <ChatInput />
      </div>

      {/* Side panel for tool results */}
      <SidePanel />
    </div>
  );
}
