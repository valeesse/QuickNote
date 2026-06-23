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
      const payload = await readClipboardPayload();
      if (payload.content.trim()) {
        await clipboardApi.capture(payload.content, payload.kind);
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

async function readClipboardPayload(): Promise<{ content: string; kind?: string }> {
  if ("read" in navigator.clipboard) {
    try {
      const items = await navigator.clipboard.read();
      for (const item of items) {
        if (item.types.includes("text/html")) {
          const blob = await item.getType("text/html");
          const html = await blob.text();
          if (html.trim()) return { content: html, kind: "rich" };
        }
      }
      for (const item of items) {
        const imageType = item.types.find((type) => type.startsWith("image/"));
        if (imageType) {
          const blob = await item.getType(imageType);
          const dataUrl = await blobToDataUrl(blob);
          return {
            content: `<img src="${dataUrl}" alt="剪贴板图片" title="剪贴板图片">`,
            kind: "image",
          };
        }
      }
    } catch {
      // Browser support and permissions vary; fall back to readText below.
    }
  }

  const text = await navigator.clipboard.readText();
  return { content: text };
}

function blobToDataUrl(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result || ""));
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(blob);
  });
}
