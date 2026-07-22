import { useEffect, useCallback, useState } from "react";
import { useClipboard } from "@/hooks/useClipboard";
import { convertFileSrc, invoke } from "@/utils/tauri";
import { hideCurrentWindow } from "@/utils/window";
import { ClipboardCard } from "@ui/components/ClipboardPanel";
import { Clipboard, Trash2, X, AlertTriangle } from "lucide-react";

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
    clearClipboard,
    loadItems,
    loadMore,
    hasMore,
    loadingMore,
    error,
  } = useClipboard();
  const [capturing, setCapturing] = useState(false);
  const [clearing, setClearing] = useState(false);
  const [showClearConfirm, setShowClearConfirm] = useState(false);
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

  const handleClear = useCallback(async () => {
    setShowClearConfirm(false);
    setClearing(true);
    setCaptureMessage(null);
    try {
      const count = await clearClipboard();
      setCaptureMessage(count > 0 ? `已清空 ${count} 条记录` : "已清空");
      window.setTimeout(() => setCaptureMessage(null), 1_500);
    } finally {
      setClearing(false);
    }
  }, [clearClipboard]);

  const handleCopy = useCallback(async (id: string) => {
    await copyItem(id);
    // Close popup after copying
    try {
      setTimeout(() => void hideCurrentWindow(), 300);
    } catch { /* noop */ }
  }, [copyItem]);

  return (
    <div className="animate-popup-in relative flex h-screen flex-col bg-[#f7f7f9]">
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
          onClick={() => setShowClearConfirm(true)}
          disabled={clearing}
          className="rounded-lg p-1.5 text-gray-400 hover:bg-red-50 hover:text-red-500 disabled:cursor-not-allowed disabled:opacity-60"
          title="清空剪贴板"
          aria-label="清空剪贴板"
        >
          <Trash2 className="h-4 w-4" />
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
              <ClipboardCard
                key={item.id}
                item={item}
                focused={false}
                copied={copiedId === item.id}
                compact
                onCopy={() => void handleCopy(item.id)}
                onTogglePin={() => void togglePin(item.id)}
                onDelete={() => void deleteItem(item.id)}
                resolveAttachmentSrc={resolveClipboardAttachment}
              />
            ))}
            {hasMore && (
              <button
                type="button"
                disabled={loadingMore}
                onClick={() => void loadMore()}
                className="w-full rounded-lg border border-gray-200 px-3 py-2 text-xs text-gray-500 hover:bg-gray-50 disabled:opacity-50"
              >
                {loadingMore ? "加载中..." : "加载更多"}
              </button>
            )}
          </div>
        )}
      </div>

      {/* Clear confirmation overlay */}
      {showClearConfirm && (
        <div className="absolute inset-0 z-50 flex items-center justify-center bg-black/30 backdrop-blur-sm">
          <div className="mx-6 w-full max-w-xs animate-popup-in rounded-2xl bg-white p-5 shadow-2xl">
            <div className="mb-4 flex items-center gap-3">
              <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-red-50 text-red-500">
                <AlertTriangle className="h-5 w-5" />
              </div>
              <div>
                <h3 className="text-sm font-semibold text-gray-900">清空剪贴板历史</h3>
                <p className="mt-0.5 text-xs text-gray-500">固定项将被保留，其余记录将被删除且无法恢复。</p>
              </div>
            </div>
            <div className="flex gap-2">
              <button
                type="button"
                onClick={() => setShowClearConfirm(false)}
                className="flex-1 rounded-xl border border-gray-200 px-3 py-2 text-xs font-medium text-gray-600 transition hover:bg-gray-50"
              >
                取消
              </button>
              <button
                type="button"
                onClick={() => void handleClear()}
                className="flex-1 rounded-xl bg-red-500 px-3 py-2 text-xs font-medium text-white transition hover:bg-red-600"
              >
                确认清空
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

async function resolveClipboardAttachment(id: string): Promise<string> {
  const attachment = await invoke<{ id: string; path: string }>("get_attachment_preview", { id });
  return convertFileSrc(attachment.path);
}
