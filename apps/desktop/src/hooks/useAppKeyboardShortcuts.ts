import { useEffect } from "react";
import type { AppView } from "@/types";

type Options = { viewMode: AppView; onCreate: () => unknown; onUndoDelete: () => unknown; onShowNotes: () => void };

export function useAppKeyboardShortcuts({ viewMode, onCreate, onUndoDelete, onShowNotes }: Options) {
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key === "n") {
        event.preventDefault(); onShowNotes(); onCreate();
      }
      if ((event.ctrlKey || event.metaKey) && event.key === "f") {
        event.preventDefault();
        const placeholder = viewMode === "notes" ? "搜索便签..." : "搜索剪贴板历史";
        document.querySelector<HTMLInputElement>(`input[placeholder="${placeholder}"]`)?.focus();
      }
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "z" && event.shiftKey) {
        event.preventDefault(); onUndoDelete();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [onCreate, onShowNotes, onUndoDelete, viewMode]);
}
