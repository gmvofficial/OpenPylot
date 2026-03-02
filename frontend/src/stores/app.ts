import { create } from "zustand";
import type { AgentStatus } from "@/types";
import { api } from "@/lib/api";

interface AppState {
  status: AgentStatus | null;
  sidebarCollapsed: boolean;
  currentPage: string;

  fetchStatus: () => Promise<void>;
  toggleSidebar: () => void;
  setCurrentPage: (page: string) => void;
}

export const useAppStore = create<AppState>((set) => ({
  status: null,
  sidebarCollapsed: false,
  currentPage: "chat",

  async fetchStatus() {
    try {
      const status = await api.getStatus();
      set({ status });
    } catch {
      set({
        status: {
          status: "offline",
          online: false,
          uptime: 0,
          model: "unknown",
          provider: "unknown",
          active_integrations: 0,
          integrationCount: 0,
          toolCount: 0,
          version: "0.2.0",
        },
      });
    }
  },

  toggleSidebar() {
    set((state) => ({ sidebarCollapsed: !state.sidebarCollapsed }));
  },

  setCurrentPage(page: string) {
    set({ currentPage: page });
  },
}));
