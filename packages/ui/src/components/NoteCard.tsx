import React from "react";
import { Star } from "lucide-react";
import { formatRelativeTime } from "../utils/format";

export interface NoteCardProps {
  id: string;
  title: string;
  preview: string;
  updatedAt: string;
  isPinned?: boolean;
  isActive?: boolean;
  style?: React.CSSProperties;
  onSelect: (id: string) => void;
  onContextMenu?: (event: React.MouseEvent, id: string) => void;
}

export function NoteCard({
  id,
  title,
  preview,
  updatedAt,
  isPinned = false,
  isActive = false,
  style,
  onSelect,
  onContextMenu,
}: NoteCardProps) {
  const displayTitle = title || "无标题";
  const displayPreview = preview || "空便签";

  return (
    <button
      type="button"
      style={style}
      aria-current={isActive ? "true" : undefined}
      className={`note-card flex w-full border-b border-gray-100 text-left ${
        isActive ? "active" : ""
      }`}
      onClick={() => onSelect(id)}
      onContextMenu={(event) => onContextMenu?.(event, id)}
    >
      <span className="flex h-full min-h-[88px] w-full flex-col px-4 py-3">
        <span className="flex items-start justify-between gap-2">
          <span className="min-w-0 flex-1 truncate text-sm font-semibold text-gray-800">
            {displayTitle}
          </span>
          {isPinned && (
            <Star
              className="h-3.5 w-3.5 flex-shrink-0 text-blue-500"
              fill="currentColor"
              aria-label="已置顶"
            />
          )}
        </span>
        <span className="mt-1 min-h-0 flex-1 overflow-hidden text-xs leading-relaxed text-gray-500">
          <span className="line-clamp-2">{displayPreview}</span>
        </span>
        <span className="mt-1 flex-shrink-0 truncate text-xs text-gray-400">
          {formatRelativeTime(updatedAt)}
        </span>
      </span>
    </button>
  );
}

export function NoteSectionLabel({
  children,
  icon,
  style,
}: {
  children: React.ReactNode;
  icon?: React.ReactNode;
  style?: React.CSSProperties;
}) {
  return (
    <div style={style} className="flex items-center px-4 pt-3 pb-1">
      {icon}
      <span className="text-xs font-medium uppercase tracking-wider text-gray-400">
        {children}
      </span>
    </div>
  );
}
