import { Search, Pin, X, Copy, Clipboard } from "lucide-react";
import type { ClipboardItem } from "@/types";

interface ClipboardPanelProps {
  items: ClipboardItem[];
  query: string;
  copiedId: string | null;
  error: string | null;
  onQueryChange: (query: string) => void;
  onCapture: () => void;
  onCopy: (id: string) => void;
  onDelete: (id: string) => void;
}

export function ClipboardPanel({
  items,
  query,
  copiedId,
  error,
  onQueryChange,
  onCapture,
  onCopy,
  onDelete,
}: ClipboardPanelProps) {
  return (
    <div className="flex h-full flex-col bg-[#f7f7f9]">
      <header className="border-b border-gray-200/80 bg-white/90 px-6 py-5 backdrop-blur">
        <div className="mx-auto flex max-w-6xl flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <h2 className="text-xl font-semibold tracking-tight text-gray-900">剪贴板</h2>
            <p className="mt-1 text-xs text-gray-500">跨设备共享的剪贴板历史</p>
          </div>
          <button
            onClick={onCapture}
            className="rounded-xl bg-gray-900 px-4 py-2.5 text-sm font-medium text-white shadow-sm transition hover:bg-black"
          >
            读取当前剪贴板
          </button>
        </div>
        <div className="mx-auto mt-4 max-w-6xl">
          <div className="relative max-w-xl">
            <Search className="pointer-events-none absolute inset-y-0 left-3 my-auto h-4 w-4 text-gray-400" />
            <input
              value={query}
              onChange={(event) => onQueryChange(event.target.value)}
              placeholder="搜索剪贴板历史"
              className="w-full rounded-xl border border-gray-200 bg-gray-50 py-2.5 pl-9 pr-4 text-sm outline-none transition focus:border-gray-400 focus:bg-white"
            />
          </div>
          {error && <p className="mt-2 text-xs text-red-600">{error}</p>}
        </div>
      </header>

      <main className="flex-1 overflow-y-auto px-6 py-6">
        <div className="mx-auto max-w-6xl">
          {items.length === 0 ? (
            <div className="flex min-h-72 flex-col items-center justify-center rounded-3xl border border-dashed border-gray-300 bg-white text-center">
              <div className="mb-4 flex h-14 w-14 items-center justify-center rounded-2xl bg-violet-100 text-violet-700">
                <Clipboard className="h-7 w-7" />
              </div>
              <h3 className="text-sm font-semibold text-gray-700">暂无剪贴板记录</h3>
              <p className="mt-1 max-w-sm text-xs leading-5 text-gray-400">
                复制文本、链接或代码片段后，它们会安全地保存到云端。
              </p>
            </div>
          ) : (
            <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
              {items.map((item) => (
                <ClipboardCard
                  key={item.id}
                  item={item}
                  copied={copiedId === item.id}
                  onCopy={() => onCopy(item.id)}
                  onDelete={() => onDelete(item.id)}
                />
              ))}
            </div>
          )}
        </div>
      </main>
    </div>
  );
}

function ClipboardCard({
  item,
  copied,
  onCopy,
  onDelete,
}: {
  item: ClipboardItem;
  copied: boolean;
  onCopy: () => void;
  onDelete: () => void;
}) {
  const palette =
    item.kind === "link"
      ? "bg-blue-50 text-blue-700"
      : item.kind === "code"
        ? "bg-emerald-50 text-emerald-700"
        : "bg-violet-50 text-violet-700";
  const label = item.kind === "link" ? "链接" : item.kind === "code" ? "代码" : "文本";

  return (
    <article className="group flex min-h-36 flex-col rounded-xl border border-gray-200/80 bg-white p-3 shadow-sm transition hover:-translate-y-0.5 hover:shadow-md">
      <div className="flex items-center justify-between gap-2">
        <span className={`rounded-md px-2 py-0.5 text-[11px] font-medium ${palette}`}>{label}</span>
        <div className="flex items-center gap-0.5 opacity-60 transition group-hover:opacity-100">
          {item.is_pinned && (
            <Pin className="h-3.5 w-3.5 text-amber-500" />
          )}
          <button
            onClick={onDelete}
            className="rounded p-1 text-gray-400 hover:bg-red-50 hover:text-red-500"
            title="删除"
          >
            <X className="h-3.5 w-3.5" />
          </button>
        </div>
      </div>
      <button onClick={onCopy} className="mt-2 min-h-16 flex-1 text-left" title="复制到剪贴板">
        <p
          className={`line-clamp-4 whitespace-pre-wrap break-words text-[13px] leading-5 text-gray-700 ${item.kind === "code" ? "font-mono" : ""}`}
        >
          {item.content}
        </p>
      </button>
      <div className="mt-2 flex items-center justify-between border-t border-gray-100 pt-2 text-[11px] text-gray-400">
        <span>{formatRelativeTime(item.last_copied_at)} · {shortDevice(item.source_device)}</span>
        <span className={`flex items-center gap-1 ${copied ? "font-medium text-emerald-600" : ""}`}>
          {copied ? (
            "已复制"
          ) : (
            <>
              <Copy className="h-3 w-3" />
              {item.capture_count > 1 ? `${item.capture_count} 次` : "点击复制"}
            </>
          )}
        </span>
      </div>
    </article>
  );
}

function shortDevice(device: string): string {
  return device ? `设备 ${device.slice(0, 6)}` : "本机";
}

function formatRelativeTime(value: string): string {
  const diff = Date.now() - new Date(value).getTime();
  if (diff < 60_000) return "刚刚";
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)} 分钟前`;
  if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)} 小时前`;
  return new Date(value).toLocaleDateString("zh-CN", { month: "short", day: "numeric" });
}
