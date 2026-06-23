import React, { useCallback, useEffect, useRef, useState } from "react";
import { Search, Pin, PinOff, Copy, Clipboard, Trash2 } from "lucide-react";
import type { ClipboardItem } from "@contracts";
import { formatRelativeTime } from "../utils/format";

interface ClipboardPanelProps {
  items: ClipboardItem[];
  query: string;
  copiedId: string | null;
  error: string | null;
  onQueryChange: (query: string) => void;
  onCapture: () => void | Promise<unknown>;
  onCopy: (id: string) => void;
  onDelete: (id: string) => void;
  /** Desktop-only: toggle pin on clipboard items. */
  onTogglePin?: (id: string) => void;
  /** Desktop-only: whether auto-capture is supported on this platform. */
  autoCaptureSupported?: boolean;
  /** Desktop-only: whether auto-capture is currently enabled. */
  autoCaptureEnabled?: boolean;
  /** Desktop-only: callback to toggle auto-capture. */
  onAutoCaptureChange?: (enabled: boolean) => void;
  /** Description text shown under the header title. */
  description?: string;
  /** Desktop-only: item that should be scrolled into view. */
  focusedItemId?: string | null;
  /** Desktop-only: create a note from this clipboard item. */
  onCreateNoteFromItem?: (id: string) => void;
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
  onTogglePin,
  autoCaptureSupported,
  autoCaptureEnabled,
  onAutoCaptureChange,
  description = "跨设备共享的剪贴板历史",
  focusedItemId,
  onCreateNoteFromItem,
}: ClipboardPanelProps) {
  const [capturing, setCapturing] = useState(false);
  const [captureMessage, setCaptureMessage] = useState<string | null>(null);
  const cardRefs = useRef(new Map<string, HTMLElement>());

  const handleCapture = useCallback(async () => {
    if (capturing) return;
    setCapturing(true);
    setCaptureMessage(null);
    try {
      const result = await onCapture();
      setCaptureMessage(result === null ? "没有新的剪贴板内容" : "已读取剪贴板");
      window.setTimeout(() => setCaptureMessage(null), 1_500);
    } finally {
      setCapturing(false);
    }
  }, [capturing, onCapture]);

  useEffect(() => {
    if (!focusedItemId) return;
    const element = cardRefs.current.get(focusedItemId);
    element?.scrollIntoView({ block: "center", behavior: "smooth" });
  }, [focusedItemId, items]);

  return (
    <div className="clipboard-panel">
      <header className="clipboard-panel__header">
        <div className="clipboard-panel__shell clipboard-panel__topbar">
          <div>
            <h2 className="clipboard-panel__title">剪贴板</h2>
            <p className="clipboard-panel__description">
              {autoCaptureSupported !== undefined
                ? autoCaptureSupported
                  ? autoCaptureEnabled
                    ? "窗口活跃时自动采集并跨设备同步"
                    : "自动采集已暂停"
                  : "移动端按系统隐私规则，仅在点击后读取"
                : description}
            </p>
          </div>
          <div className="clipboard-panel__actions">
            {autoCaptureSupported && onAutoCaptureChange && (
              <button
                type="button"
                onClick={() => onAutoCaptureChange(!autoCaptureEnabled)}
                className="focus-ring clipboard-panel__secondary-button"
              >
                {autoCaptureEnabled ? "暂停自动采集" : "开启自动采集"}
              </button>
            )}
            <button
              type="button"
              onClick={() => void handleCapture()}
              disabled={capturing}
              className="focus-ring clipboard-panel__primary-button"
            >
              {capturing ? "读取中..." : "读取当前剪贴板"}
            </button>
          </div>
        </div>
        <div className="clipboard-panel__shell clipboard-panel__search-row">
          <label className="clipboard-panel__search">
            <Search className="clipboard-panel__search-icon" />
            <input
              type="search"
              aria-label="搜索剪贴板历史"
              value={query}
              onChange={(event) => onQueryChange(event.target.value)}
              placeholder="搜索剪贴板历史"
              className="clipboard-panel__search-input"
            />
          </label>
          {(error || captureMessage) && (
            <p className={`mt-2 text-xs ${error ? "text-red-600" : "text-emerald-600"}`}>
              {error ?? captureMessage}
            </p>
          )}
        </div>
      </header>

      <main className="clipboard-panel__body">
        <div className="clipboard-panel__shell">
          {items.length === 0 ? (
            <div className="clipboard-panel__empty">
              <div className="clipboard-panel__empty-icon">
                <Clipboard className="h-7 w-7" />
              </div>
              <h3 className="clipboard-panel__empty-title">暂无剪贴板记录</h3>
              <p className="clipboard-panel__empty-text">
                复制文本、链接或代码片段后，它们会安全地保存。
              </p>
            </div>
          ) : (
            <div className="clipboard-panel__grid">
              {items.map((item) => (
                <ClipboardCard
                  key={item.id}
                  ref={(element) => {
                    if (element) cardRefs.current.set(item.id, element);
                    else cardRefs.current.delete(item.id);
                  }}
                  item={item}
                  focused={focusedItemId === item.id}
                  copied={copiedId === item.id}
                  onCopy={() => onCopy(item.id)}
                  onTogglePin={onTogglePin ? () => onTogglePin(item.id) : undefined}
                  onDelete={() => onDelete(item.id)}
                  onCreateNote={onCreateNoteFromItem ? () => onCreateNoteFromItem(item.id) : undefined}
                />
              ))}
            </div>
          )}
        </div>
      </main>
    </div>
  );
}

const ClipboardCard = React.forwardRef<HTMLElement, {
  item: ClipboardItem;
  focused: boolean;
  copied: boolean;
  onCopy: () => void;
  onTogglePin?: () => void;
  onDelete: () => void;
  onCreateNote?: () => void;
}>(function ClipboardCard({
  item,
  focused,
  copied,
  onCopy,
  onTogglePin,
  onDelete,
  onCreateNote,
}, ref) {
  const palette =
    item.kind === "link"
      ? "bg-blue-50 text-blue-700"
      : item.kind === "code"
        ? "bg-emerald-50 text-emerald-700"
        : item.kind === "image" || item.kind === "rich"
          ? "bg-rose-50 text-rose-700"
          : "bg-violet-50 text-violet-700";
  const label =
    item.kind === "link"
      ? "链接"
      : item.kind === "code"
        ? "代码"
        : item.kind === "image"
          ? "图片"
          : item.kind === "rich"
            ? "图文"
            : "文本";
  const isRich = item.kind === "rich" || item.kind === "image";

  return (
    <article
      ref={ref}
      className={`clipboard-card group ${focused ? "clipboard-card--focused" : ""}`}
      onContextMenu={(event) => {
        if (!onCreateNote) return;
        event.preventDefault();
        onCreateNote();
      }}
    >
      <div className="clipboard-card__header">
        <span className={`rounded-md px-2 py-0.5 text-[11px] font-medium ${palette}`}>{label}</span>
        <div className="clipboard-card__actions">
          {onTogglePin && (
            <button
              type="button"
              onClick={onTogglePin}
              className={`focus-ring rounded p-1 hover:bg-gray-100 ${item.is_pinned ? "text-amber-500" : "text-gray-400"}`}
              title={item.is_pinned ? "取消固定" : "固定"}
              aria-label={item.is_pinned ? "取消固定" : "固定"}
            >
              {item.is_pinned ? <Pin className="h-3.5 w-3.5" /> : <PinOff className="h-3.5 w-3.5" />}
            </button>
          )}
          {!onTogglePin && item.is_pinned && (
            <Pin className="h-3.5 w-3.5 text-amber-500" />
          )}
          <button
            type="button"
            onClick={onDelete}
            className="focus-ring rounded p-1 text-gray-400 hover:bg-red-50 hover:text-red-500"
            title="删除"
            aria-label="删除剪贴板记录"
          >
            <Trash2 className="h-3.5 w-3.5" />
          </button>
        </div>
      </div>
      <button type="button" onClick={onCopy} className="focus-ring clipboard-card__copy" title="复制到剪贴板">
        {isRich ? (
          <div
            className="clipboard-card__rich line-clamp-4"
            dangerouslySetInnerHTML={{ __html: sanitizeClipboardHtml(item.content) }}
          />
        ) : (
          <p
            className={`line-clamp-4 whitespace-pre-wrap break-words text-[13px] leading-5 text-gray-700 ${item.kind === "code" ? "font-mono" : ""}`}
          >
            {item.content}
          </p>
        )}
      </button>
      <div className="clipboard-card__meta">
        <span>
          {formatRelativeTime(item.last_copied_at)} · {shortDevice(item.source_device)}
        </span>
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
});

function shortDevice(device: string): string {
  return device ? `设备 ${device.slice(0, 6)}` : "本机";
}

function sanitizeClipboardHtml(html: string): string {
  if (typeof document === "undefined") return "";
  const doc = new DOMParser().parseFromString(html, "text/html");
  const allowedTags = new Set([
    "A",
    "B",
    "BR",
    "CODE",
    "DIV",
    "EM",
    "I",
    "IMG",
    "LI",
    "OL",
    "P",
    "PRE",
    "SPAN",
    "STRONG",
    "UL",
  ]);
  const allowedAttrs = new Set(["alt", "href", "src", "title"]);

  for (const element of Array.from(doc.body.querySelectorAll("*"))) {
    if (!allowedTags.has(element.tagName)) {
      element.replaceWith(...Array.from(element.childNodes));
      continue;
    }
    for (const attr of Array.from(element.attributes)) {
      const name = attr.name.toLowerCase();
      const value = attr.value.trim();
      if (!allowedAttrs.has(name)) {
        element.removeAttribute(attr.name);
        continue;
      }
      if ((name === "src" || name === "href") && !isSafeClipboardUrl(value)) {
        element.removeAttribute(attr.name);
      }
    }
    if (element.tagName === "A") {
      element.setAttribute("target", "_blank");
      element.setAttribute("rel", "noreferrer");
    }
  }

  return doc.body.innerHTML;
}

function isSafeClipboardUrl(value: string): boolean {
  return (
    value.startsWith("data:image/") ||
    value.startsWith("https://") ||
    value.startsWith("http://") ||
    value.startsWith("blob:")
  );
}
