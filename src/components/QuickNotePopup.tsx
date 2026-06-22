import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@/utils/tauri";
import { FileEdit, X } from "lucide-react";

export function QuickNotePopup() {
  const [content, setContent] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Auto-focus textarea
  useEffect(() => {
    setTimeout(() => textareaRef.current?.focus(), 100);
  }, []);

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

  const saveAndClose = useCallback(async () => {
    const text = content.trim();
    if (!text || saving) return;
    setSaving(true);
    setError(null);
    try {
      // Wrap plain text in a paragraph for the HTML content
      const html = text
        .split("\n")
        .map((line) => (line.trim() ? `<p>${escapeHtml(line)}</p>` : "<p></p>"))
        .join("");
      await invoke("create_note", { content: html });
      setContent("");
      try {
        const { getCurrentWindow } = await import("@tauri-apps/api/window");
        void getCurrentWindow().hide();
      } catch { /* noop */ }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  }, [content, saving]);

  // Keyboard shortcuts
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        import("@tauri-apps/api/window").then(({ getCurrentWindow }) => {
          void getCurrentWindow().hide();
        });
      } else if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        void saveAndClose();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [saveAndClose]);

  return (
    <div className="flex h-screen flex-col bg-[#f7f7f9]">
      {/* Header */}
      <div className="flex items-center gap-2 border-b border-gray-200 bg-white px-4 py-3">
        <FileEdit className="h-4 w-4 text-violet-600" />
        <h2 className="text-sm font-semibold text-gray-800">快速便签</h2>
        <div className="flex-1" />
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

      {/* Editor */}
      <div className="flex-1 p-4">
        <textarea
          ref={textareaRef}
          value={content}
          onChange={(e) => setContent(e.target.value)}
          placeholder="输入便签内容... (Ctrl+Enter 保存)"
          className="h-full w-full resize-none rounded-xl border border-gray-200 bg-white px-4 py-3 text-sm leading-relaxed text-gray-800 outline-none transition focus:border-violet-300 focus:ring-2 focus:ring-violet-100"
        />
      </div>

      {/* Footer */}
      <div className="flex items-center justify-between border-t border-gray-200 bg-white px-4 py-2.5">
        <div className="text-[11px] text-gray-400">
          {error ? (
            <span className="text-red-500">{error}</span>
          ) : (
            <>Ctrl+Enter 保存并关闭 · Esc 关闭</>
          )}
        </div>
        <button
          onClick={() => void saveAndClose()}
          disabled={!content.trim() || saving}
          className="rounded-lg bg-violet-600 px-4 py-1.5 text-xs font-medium text-white transition hover:bg-violet-700 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {saving ? "保存中..." : "保存"}
        </button>
      </div>
    </div>
  );
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}
