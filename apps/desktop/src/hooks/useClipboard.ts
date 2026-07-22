import { useCallback, useEffect, useRef, useState } from "react";
import { invoke, isTauri } from "@/utils/tauri";
import type { ClipboardItem } from "@/types";

const CLIPBOARD_PAGE_SIZE = 50;

export function useClipboard() {
  const [items, setItems] = useState<ClipboardItem[]>([]);
  const [query, setQuery] = useState("");
  const [autoCaptureSupported, setAutoCaptureSupported] = useState(false);
  const [autoCaptureEnabled, setAutoCaptureEnabledState] = useState(
    () => typeof window === "undefined" || window.localStorage.getItem("quicknote-clipboard-auto-capture") !== "false"
  );
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [hasMore, setHasMore] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const capturingRef = useRef(false);
  const initialAutoCaptureEnabledRef = useRef(autoCaptureEnabled);

  const loadItems = useCallback(async () => {
    if (!isTauri()) return;
    try {
      const result = await invoke<ClipboardItem[]>("list_clipboard_items", {
        query,
        limit: CLIPBOARD_PAGE_SIZE,
        offset: 0,
      });
      setItems(result);
      setHasMore(result.length === CLIPBOARD_PAGE_SIZE);
    } catch (err) {
      setError(getErrorMessage(err));
    }
  }, [query]);

  const loadMore = useCallback(async () => {
    if (!isTauri() || loadingMore || !hasMore) return;
    setLoadingMore(true);
    try {
      const result = await invoke<ClipboardItem[]>("list_clipboard_items", {
        query,
        limit: CLIPBOARD_PAGE_SIZE,
        offset: items.length,
      });
      setItems((current) => {
        const known = new Set(current.map((item) => item.id));
        return [...current, ...result.filter((item) => !known.has(item.id))];
      });
      setHasMore(result.length === CLIPBOARD_PAGE_SIZE);
    } catch (err) {
      setError(getErrorMessage(err));
    } finally {
      setLoadingMore(false);
    }
  }, [hasMore, items.length, loadingMore, query]);

  const capture = useCallback(async (silent = false) => {
    if (!isTauri() || capturingRef.current) return null;
    capturingRef.current = true;
    try {
      const item = await invoke<ClipboardItem | null>("capture_clipboard");
      if (item) {
        await loadItems();
        requestSync();
      }
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
    requestSync();
  }, [loadItems]);

  const deleteItem = useCallback(async (id: string) => {
    await invoke("delete_clipboard_item", { id });
    await loadItems();
    requestSync();
  }, [loadItems]);

  const clearClipboard = useCallback(async () => {
    const count = await invoke<number>("clear_clipboard");
    await invoke<boolean>("prime_clipboard_capture");
    await loadItems();
    if (count > 0) requestSync();
    return count;
  }, [loadItems]);

  useEffect(() => {
    const timer = setTimeout(() => void loadItems(), query ? 180 : 0);
    return () => clearTimeout(timer);
  }, [loadItems, query]);

  useEffect(() => {
    if (!isTauri()) return;
    void invoke<boolean>("clipboard_auto_capture_supported")
      .then(async (supported) => {
        setAutoCaptureSupported(supported);
        if (!supported) return;
        await invoke<boolean>("set_clipboard_auto_capture_enabled", {
          enabled: initialAutoCaptureEnabledRef.current,
        });
      })
      .catch((err) => {
        setError(getErrorMessage(err));
      });
  }, []);

  useEffect(() => {
    if (!autoCaptureSupported || !autoCaptureEnabled) return;
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void import("@tauri-apps/api/event")
      .then(async ({ listen }) => {
        const stop = await listen<ClipboardItem>("clipboard-captured", ({ payload }) => {
          if (query) {
            void loadItems();
          } else {
            setItems((current) => upsertClipboardItem(current, payload));
          }
          requestSync();
        });
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch(() => {
        // Browser-based tests and non-Tauri previews do not expose Tauri's event bridge.
      });
    const onFocus = () => void loadItems();
    window.addEventListener("focus", onFocus);
    return () => {
      disposed = true;
      unlisten?.();
      window.removeEventListener("focus", onFocus);
    };
  }, [autoCaptureEnabled, autoCaptureSupported, loadItems, query]);

  const setAutoCaptureEnabled = useCallback((enabled: boolean) => {
    setAutoCaptureEnabledState(enabled);
    window.localStorage.setItem("quicknote-clipboard-auto-capture", String(enabled));
    void invoke<boolean>("set_clipboard_auto_capture_enabled", { enabled }).catch((err) => {
      setError(getErrorMessage(err));
    });
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
    loadMore,
    hasMore,
    loadingMore,
  };
}

function upsertClipboardItem(items: ClipboardItem[], next: ClipboardItem): ClipboardItem[] {
  const merged = [next, ...items.filter((item) => item.id !== next.id)];
  merged.sort((left, right) => {
    if (left.is_pinned !== right.is_pinned) return left.is_pinned ? -1 : 1;
    return right.last_copied_at.localeCompare(left.last_copied_at);
  });
  return merged.slice(0, Math.max(CLIPBOARD_PAGE_SIZE, items.length));
}

function getErrorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function requestSync() {
  window.dispatchEvent(new Event("quicknote:sync-needed"));
}
