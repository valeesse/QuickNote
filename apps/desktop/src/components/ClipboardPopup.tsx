import { useEffect, useCallback, useState } from "react";
import { useClipboard } from "@/hooks/useClipboard";
import { hideCurrentWindow } from "@/utils/window";
import { formatRelativeTime } from "@ui/utils/format";
import { Pin, PinOff, Copy, Clipboard, X } from "lucide-react";

export function ClipboardPopup() {
  const {
    items,
    query,
    setQuery,
    copiedId,
    capture,
    copyItem,
    togglePin,
    deleteItem,
    loadItems,
    error,
  } = useClipboard();
  const [capturing, setCapturing] = useState(false);
  const [captureMessage, setCaptureMessage] = useState<string | null>(null);

  // Hide window on blur
  useEffect(() => {
    let destroyed = false;
    let unlisten: (() => void) | undefined;
    (async () => {
      try {
        const { getCurrentWindow } = await import("@tauri-apps/api/window");
        const win = getCurrentWindow();
        unlisten = await win.onFocusChanged(({ payload: focused }) => {
          if (!focused && !destroyed) void win.hide();
        });
        if (destroyed) unlisten();
      } catch { /* not in tauri */ }
    })();
    return () => {
      destroyed = true;
      unlisten?.();
    };
  }, []);

  // ESC to close
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        void hideCurrentWindow();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // Refresh items when window becomes visible
  useEffect(() => {
    let destroyed = false;
    let unlisten: (() => void) | undefined;
    (async () => {
      try {
        const { getCurrentWindow } = await import("@tauri-apps/api/window");
        const win = getCurrentWindow();
        unlisten = await win.onFocusChanged(({ payload: focused }) => {
          if (focused) void loadItems();
        });
        if (destroyed) unlisten();
      } catch { /* noop */ }
    })();
    return () => {
      destroyed = true;
      unlisten?.();
    };
  }, [loadItems]);

  const handleCapture = useCallback(async () => {
    if (capturing) return;
    setCapturing(true);
    setCaptureMessage(null);
    try {
      const item = await capture(false);
      await loadItems();
      setCaptureMessage(item ? "已读取" : "没有新的剪贴板内容");
      window.setTimeout(() => setCaptureMessage(null), 1_500);
    } finally {
      setCapturing(false);
    }
  }, [capture, capturing, loadItems]);

  const handleCopy = useCallback(async (id: string) => {
    await copyItem(id);
    // Close popup after copying
    try {
      setTimeout(() => void hideCurrentWindow(), 300);
    } catch { /* noop */ }
  }, [copyItem]);

  return (
    <div className="animate-popup-in flex h-screen flex-col bg-[#f7f7f9]">
      {/* Header */}
      <div className="flex items-center gap-2 border-b border-gray-200 bg-white px-4 py-3">
        <Clipboard className="h-4 w-4 text-violet-600" />
        <h2 className="text-sm font-semibold text-gray-800">剪贴板历史</h2>
        <div className="flex-1" />
        {(captureMessage || error) && (
          <span className={`max-w-28 truncate text-[11px] ${error ? "text-red-500" : "text-emerald-600"}`}>
            {error ?? captureMessage}
          </span>
        )}
        <button
          type="button"
          onClick={() => void handleCapture()}
          disabled={capturing}
          className="rounded-lg bg-gray-900 px-3 py-1.5 text-xs font-medium text-white hover:bg-black disabled:cursor-not-allowed disabled:opacity-60"
        >
          {capturing ? "读取中..." : "读取剪贴板"}
        </button>
        <button
          type="button"
          onClick={() => void hideCurrentWindow()}
          className="rounded-lg p-1.5 text-gray-400 hover:bg-gray-100 hover:text-gray-600"
          title="关闭"
          aria-label="关闭"
        >
          <X className="h-4 w-4" />
        </button>
      </div>

      {/* Search */}
      <div className="px-4 py-2">
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="搜索..."
          className="w-full rounded-lg border border-gray-200 bg-gray-50 px-3 py-2 text-xs outline-none focus:border-gray-400 focus:bg-white"
        />
      </div>

      {/* Items */}
      <div className="flex-1 overflow-y-auto px-3 pb-3">
        {items.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center text-center">
            <Clipboard className="mb-2 h-8 w-8 text-gray-300" />
            <p className="text-xs text-gray-400">暂无剪贴板记录</p>
          </div>
        ) : (
          <div className="space-y-2">
            {items.map((item) => (
              <div
                key={item.id}
                className="group rounded-lg border border-gray-200/80 bg-white p-2.5 transition hover:border-gray-300 hover:shadow-sm"
              >
                <div className="mb-1.5 flex items-center gap-1.5">
                  <span
                    className={`rounded px-1.5 py-0.5 text-[10px] font-medium ${
                      item.kind === "link"
                        ? "bg-blue-50 text-blue-600"
                        : item.kind === "code"
                          ? "bg-emerald-50 text-emerald-600"
                          : "bg-violet-50 text-violet-600"
                    }`}
                  >
                    {item.kind === "link" ? "链接" : item.kind === "code" ? "代码" : "文本"}
                  </span>
                  <div className="flex-1" />
                  <button
                    type="button"
                    onClick={() => void togglePin(item.id)}
                    className={`rounded p-0.5 hover:bg-gray-100 ${item.is_pinned ? "text-amber-500" : "text-gray-300"}`}
                    title={item.is_pinned ? "取消固定" : "固定"}
                    aria-label={item.is_pinned ? "取消固定" : "固定"}
                  >
                    {item.is_pinned ? <PinOff className="h-3 w-3" /> : <Pin className="h-3 w-3" />}
                  </button>
                  <button
                    type="button"
                    onClick={() => void deleteItem(item.id)}
                    className="rounded p-0.5 text-gray-300 hover:bg-red-50 hover:text-red-500"
                    title="删除"
                    aria-label="删除剪贴板记录"
                  >
                    <X className="h-3 w-3" />
                  </button>
                </div>
                <button
                  type="button"
                  onClick={() => void handleCopy(item.id)}
                  className="block w-full text-left"
                  title="复制到剪贴板"
                >
                  <p
                    className={`line-clamp-3 whitespace-pre-wrap break-words text-xs leading-4.5 text-gray-700 ${
                      item.kind === "code" ? "font-mono" : ""
                    }`}
                  >
                    {item.content}
                  </p>
                </button>
                <div className="mt-1.5 flex items-center justify-between text-[10px] text-gray-400">
                  <span>{formatRelativeTime(item.last_copied_at)}</span>
                  <span className={copiedId === item.id ? "font-medium text-emerald-600" : ""}>
                    {copiedId === item.id ? "已复制" : <Copy className="inline h-3 w-3" />}
                  </span>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
