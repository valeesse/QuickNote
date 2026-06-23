import { useEffect, useCallback } from "react";
import { useClipboard } from "@/hooks/useClipboard";
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
    loadItems,
  } = useClipboard();

  // Hide window on blur
  useEffect(() => {
    let destroyed = false;
    (async () => {
      try {
        const { getCurrentWindow } = await import("@tauri-apps/api/window");
        const win = getCurrentWindow();
        const unlisten = await win.onFocusChanged(({ payload: focused }) => {
          if (!focused && !destroyed) void win.hide();
        });
        if (destroyed) unlisten();
        else return () => { destroyed = true; unlisten(); };
      } catch { /* not in tauri */ }
    })();
    return () => { destroyed = true; };
  }, []);

  // ESC to close
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        import("@tauri-apps/api/window").then(({ getCurrentWindow }) => {
          void getCurrentWindow().hide();
        });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // Refresh items when window becomes visible
  useEffect(() => {
    (async () => {
      try {
        const { getCurrentWindow } = await import("@tauri-apps/api/window");
        const win = getCurrentWindow();
        return await win.onFocusChanged(({ payload: focused }) => {
          if (focused) void loadItems();
        });
      } catch { /* noop */ }
    })();
  }, [loadItems]);

  const handleCopy = useCallback(async (id: string) => {
    await copyItem(id);
    // Close popup after copying
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      setTimeout(() => void getCurrentWindow().hide(), 300);
    } catch { /* noop */ }
  }, [copyItem]);

  return (
    <div className="flex h-screen flex-col bg-[#f7f7f9]">
      {/* Header */}
      <div className="flex items-center gap-2 border-b border-gray-200 bg-white px-4 py-3">
        <Clipboard className="h-4 w-4 text-violet-600" />
        <h2 className="text-sm font-semibold text-gray-800">剪贴板历史</h2>
        <div className="flex-1" />
        <button
          onClick={() => void capture()}
          className="rounded-lg bg-gray-900 px-3 py-1.5 text-xs font-medium text-white hover:bg-black"
        >
          读取剪贴板
        </button>
        <button
          onClick={() => {
            import("@tauri-apps/api/window").then(({ getCurrentWindow }) => {
              void getCurrentWindow().hide();
            });
          }}
          className="rounded-lg p-1.5 text-gray-400 hover:bg-gray-100 hover:text-gray-600"
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
                    onClick={() => void togglePin(item.id)}
                    className={`rounded p-0.5 hover:bg-gray-100 ${item.is_pinned ? "text-amber-500" : "text-gray-300"}`}
                  >
                    {item.is_pinned ? <PinOff className="h-3 w-3" /> : <Pin className="h-3 w-3" />}
                  </button>
                </div>
                <button
                  onClick={() => void handleCopy(item.id)}
                  className="block w-full text-left"
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
