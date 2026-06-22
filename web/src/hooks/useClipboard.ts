import { useCallback, useEffect, useState } from "react";
import { clipboardApi } from "@/api/client";
import type { ClipboardItem } from "@/types";

export function useClipboard() {
  const [items, setItems] = useState<ClipboardItem[]>([]);
  const [query, setQuery] = useState("");
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const loadItems = useCallback(async () => {
    try {
      const result = await clipboardApi.list();
      const filtered = query
        ? result.filter(
            (item) =>
              item.content.toLowerCase().includes(query.toLowerCase()) ||
              item.preview.toLowerCase().includes(query.toLowerCase()),
          )
        : result;
      setItems(filtered);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }, [query]);

  const capture = useCallback(async () => {
    try {
      const text = await navigator.clipboard.readText();
      if (text.trim()) {
        await clipboardApi.capture(text);
        await loadItems();
      }
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }, [loadItems]);

  const copyItem = useCallback(
    async (id: string) => {
      const item = items.find((i) => i.id === id);
      if (!item) return;
      try {
        await navigator.clipboard.writeText(item.content);
        setCopiedId(id);
        setTimeout(
          () => setCopiedId((c) => (c === id ? null : c)),
          1200,
        );
      } catch {
        setError("Failed to copy to clipboard");
      }
    },
    [items],
  );

  const deleteItem = useCallback(
    async (id: string) => {
      try {
        await clipboardApi.delete(id);
        await loadItems();
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
      }
    },
    [loadItems],
  );

  useEffect(() => {
    const timer = setTimeout(() => void loadItems(), query ? 200 : 0);
    return () => clearTimeout(timer);
  }, [loadItems, query]);

  return {
    items,
    query,
    setQuery,
    copiedId,
    error,
    capture,
    copyItem,
    deleteItem,
    loadItems,
  };
}
