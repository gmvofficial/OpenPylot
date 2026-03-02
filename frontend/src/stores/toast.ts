import { create } from "zustand";

export interface ToastItem {
  id: string;
  title: string;
  description?: string;
  variant?: "default" | "success" | "error" | "warning" | "info";
}

interface ToastState {
  toasts: ToastItem[];
  addToast: (toast: Omit<ToastItem, "id">) => void;
  dismiss: (id: string) => void;
}

let toastCounter = 0;

export const useToastStore = create<ToastState>((set) => ({
  toasts: [],

  addToast(toast) {
    const id = `toast_${++toastCounter}_${Date.now()}`;
    set((state) => ({
      toasts: [...state.toasts, { ...toast, id }].slice(-5), // Keep max 5 toasts
    }));
  },

  dismiss(id) {
    set((state) => ({
      toasts: state.toasts.filter((t) => t.id !== id),
    }));
  },
}));
