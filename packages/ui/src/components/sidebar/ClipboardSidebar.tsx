import React from "react";
import { Clipboard } from "lucide-react";
import type { ClipboardItem } from "@contracts";
import { formatRelativeTime } from "../../utils/format";
import { stripHtml, stripMarkdown } from "../../utils/html";

export function ClipboardSidebar({
  items,
  pinnedItems,
  onSelect,
  onContextMenu,
}: {
  items: ClipboardItem[];
  pinnedItems: ClipboardItem[];
  onSelect: (id: string) => void;
  onContextMenu: (event: React.MouseEvent, itemId: string) => void;
}) {
  if (items.length === 0 && pinnedItems.length === 0) {
    return (
      <div className="clipboard-sidebar__empty">
        <div>
          <div className="mx-auto flex h-14 w-14 items-center justify-center rounded-2xl bg-violet-100 text-violet-700">
            <Clipboard className="h-7 w-7" />
          </div>
          <h3 className="mt-4 text-sm font-semibold text-gray-700">跨设备剪贴板</h3>
          <p className="mt-2 text-xs leading-5 text-gray-400">复制文本、链接、代码或图文内容后会出现在这里。</p>
        </div>
      </div>
    );
  }

  return (
    <div className="clipboard-sidebar">
      {pinnedItems.length > 0 && (
        <section className="clipboard-sidebar__section">
          <h3 className="clipboard-sidebar__title">固定</h3>
          {pinnedItems.map((item) => (
            <ClipboardSidebarItem key={item.id} item={item} onSelect={onSelect} onContextMenu={onContextMenu} />
          ))}
        </section>
      )}
      <section className="clipboard-sidebar__section">
        <h3 className="clipboard-sidebar__title">最近</h3>
        {items.map((item) => (
          <ClipboardSidebarItem key={item.id} item={item} onSelect={onSelect} onContextMenu={onContextMenu} />
        ))}
      </section>
    </div>
  );
}

export function ClipboardSidebarItem({
  item,
  onSelect,
  onContextMenu,
}: {
  item: ClipboardItem;
  onSelect: (id: string) => void;
  onContextMenu: (event: React.MouseEvent, itemId: string) => void;
}) {
  return (
    <button
      type="button"
      className="clipboard-sidebar__item"
      title={stripClipboardPreview(item)}
      onClick={() => onSelect(item.id)}
      onContextMenu={(event) => onContextMenu(event, item.id)}
    >
      <span className="clipboard-sidebar__item-title">
        <span>{clipboardKindLabel(item.kind)}</span>
        <span className="text-[10px] font-normal text-gray-400">{formatRelativeTime(item.last_copied_at)}</span>
      </span>
      <span className="clipboard-sidebar__item-text">{stripClipboardPreview(item)}</span>
    </button>
  );
}

function clipboardKindLabel(kind: ClipboardItem["kind"]): string {
  if (kind === "link") return "链接";
  if (kind === "code") return "代码";
  if (kind === "image") return "图片";
  if (kind === "rich") return "图文";
  return "文本";
}

function stripClipboardPreview(item: ClipboardItem): string {
  const preview = item.preview || item.content;
  return stripMarkdown(stripHtml(preview)).replace(/\s+/g, " ").trim() || "空剪贴板内容";
}
