import { useCallback, useEffect, useRef, useState } from "react";
import { invoke, isTauri } from "@/utils/tauri";
import type { ClipboardItem } from "@/types";

export function useClipboard() {
  const [items, setItems] = useState<ClipboardItem[]>([]);
  const [query, setQuery] = useState("");
  const [autoCaptureSupported, setAutoCaptureSupported] = useState(false);
  const [autoCaptureEnabled, setAutoCaptureEnabledState] = useState(
    () => typeof window === "undefined" || window.localStorage.getItem("quicknote-clipboard-auto-capture") !== "false"
  );
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const capturingRef = useRef(false);

  const loadItems = useCallback(async () => {
    if (!isTauri()) return;
    try {
      const result = await invoke<ClipboardItem[]>("list_clipboard_items", { query });
      setItems(result);
    } catch (err) {
      setError(getErrorMessage(err));
    }
  }, [query]);

  const capture = useCallback(async (silent = false) => {
    if (!isTauri() || capturingRef.current) return null;
    capturingRef.current = true;
    try {
      const item = await invoke<ClipboardItem | null>("capture_clipboard");
      if (item) await loadItems();
      if (!silent) setError(null);
      return item;
    } catch (err) {
      if (!silent) setError(getErrorMessage(err));
      return null;
    } finally {
      capturingRef.current = false;
    }
  }, [loadItems]);

  const copyItem = useCallback(async (id: string) => {
    const copied = await invoke<boolean>("copy_clipboard_item", { id });
    if (copied) {
      setCopiedId(id);
      setTimeout(() => setCopiedId((current) => current === id ? null : current), 1_200);
    }
  }, []);

  const togglePin = useCallback(async (id: string) => {
    await invoke("toggle_clipboard_pin", { id });
    await loadItems();
  }, [loadItems]);

  const deleteItem = useCallback(async (id: string) => {
    await invoke("delete_clipboard_item", { id });
    await loadItems();
  }, [loadItems]);

  const clearClipboard = useCallback(async () => {
    await invoke<number>("clear_clipboard");
    await invoke<boolean>("prime_clipboard_capture");
    await loadItems();
  }, [loadItems]);

  useEffect(() => {
    const timer = setTimeout(() => void loadItems(), query ? 180 : 0);
    return () => clearTimeout(timer);
  }, [loadItems, query]);

  useEffect(() => {
    if (!isTauri()) return;
    void invoke<boolean>("clipboard_auto_capture_supported").then((supported) => {
      setAutoCaptureSupported(supported);
      if (supported) void invoke<boolean>("prime_clipboard_capture");
    });
  }, []);

  useEffect(() => {
    if (!autoCaptureSupported || !autoCaptureEnabled) return;
    const timer = setInterval(() => {
      if (document.visibilityState === "visible" && document.hasFocus()) void capture(true);
    }, 1_500);
    const onFocus = () => void capture(true);
    window.addEventListener("focus", onFocus);
    return () => {
      clearInterval(timer);
      window.removeEventListener("focus", onFocus);
    };
  }, [autoCaptureEnabled, autoCaptureSupported, capture]);

  const setAutoCaptureEnabled = useCallback((enabled: boolean) => {
    setAutoCaptureEnabledState(enabled);
    window.localStorage.setItem("quicknote-clipboard-auto-capture", String(enabled));
    if (enabled) void capture(true);
  }, [capture]);

  return {
    items,
    query,
    setQuery,
    autoCaptureSupported,
    autoCaptureEnabled,
    setAutoCaptureEnabled,
    copiedId,
    error,
    capture,
    copyItem,
    togglePin,
    deleteItem,
    clearClipboard,
    loadItems,
  };
}

function getErrorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
