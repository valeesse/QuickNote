import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { List } from "react-window";
import type { AppView, NoteSummary, SyncStatus } from "@/types";

interface SidebarProps {
  viewMode: AppView;
  onViewModeChange: (mode: AppView) => void;
  clipboardCount: number;
  notes: NoteSummary[];
  activeNoteId: string | null;
  searchQuery: string;
  onSearchChange: (query: string) => void;
  onSelectNote: (id: string) => void;
  onCreateNote: () => void;
  onDeleteNote: (id: string) => void;
  onTogglePin: (id: string) => void;
  onOpenTrash: () => void;
  syncStatus: SyncStatus;
  onSync: () => void;
  onOpenSettings: () => void;
}

function formatRelativeTime(dateStr: string): string {
  const date = new Date(dateStr);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffMins = Math.floor(diffMs / 60000);
  const diffHours = Math.floor(diffMs / 3600000);
  const diffDays = Math.floor(diffMs / 86400000);

  if (diffMins < 1) return "刚刚";
  if (diffMins < 60) return `${diffMins}分钟前`;
  if (diffHours < 24) return `${diffHours}小时前`;
  if (diffDays < 7) return `${diffDays}天前`;
  return date.toLocaleDateString("zh-CN", { month: "short", day: "numeric" });
}

function stripMarkdown(text: string): string {
  const withoutImages = text.replace(/<img\b[^>]*>/gi, " [图片] ");
  const plainText = withoutImages
    .replace(/<style\b[^>]*>[\s\S]*?<\/style>/gi, " ")
    .replace(/<script\b[^>]*>[\s\S]*?<\/script>/gi, " ")
    .replace(/<[^>]+>/g, " ")
    .replace(/&nbsp;/g, " ")
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'");

  return plainText
    .replace(/#{1,6}\s/g, "")
    .replace(/[*_~`]/g, "")
    .replace(/\[([^\]]+)\]\([^)]+\)/g, "$1")
    .replace(/!\[([^\]]*)\]\([^)]+\)/g, "[图片]")
    .replace(/\s+/g, " ")
    .trim();
}

export function Sidebar({
  viewMode,
  onViewModeChange,
  clipboardCount,
  notes,
  activeNoteId,
  searchQuery,
  onSearchChange,
  onSelectNote,
  onCreateNote,
  onDeleteNote,
  onTogglePin,
  onOpenTrash,
  syncStatus,
  onSync,
  onOpenSettings,
}: SidebarProps) {
  const [contextMenu, setContextMenu] = useState<{
    noteId: string;
    x: number;
    y: number;
  } | null>(null);
  const listWrapRef = useRef<HTMLDivElement | null>(null);
  const [listHeight, setListHeight] = useState(0);

  useEffect(() => {
    const element = listWrapRef.current;
    if (!element) return;

    const updateHeight = () => {
      setListHeight(element.getBoundingClientRect().height);
    };

    updateHeight();
    const observer = new ResizeObserver(updateHeight);
    observer.observe(element);
    window.addEventListener("resize", updateHeight);

    return () => {
      observer.disconnect();
      window.removeEventListener("resize", updateHeight);
    };
  }, [viewMode]);

  const handleContextMenu = useCallback(
    (e: React.MouseEvent, noteId: string) => {
      e.preventDefault();
      setContextMenu({ noteId, x: e.clientX, y: e.clientY });
    },
    []
  );

  const closeContextMenu = useCallback(() => {
    setContextMenu(null);
  }, []);

  // Separate pinned and unpinned notes
  const pinnedNotes = useMemo(() => notes.filter((n) => n.is_pinned), [notes]);
  const unpinnedNotes = useMemo(() => notes.filter((n) => !n.is_pinned), [notes]);
  const hasPinnedNotes = pinnedNotes.length > 0;
  const hasUnpinnedNotes = unpinnedNotes.length > 0;

  const NoteItem = ({ index, style }: { index: number; style: React.CSSProperties }) => {
    // Calculate which note to show at this index
    let note: NoteSummary | undefined;
    let isPinnedSection = false;
    let showPinnedHeader = false;
    let showOtherHeader = false;

    let adjustedIndex = index;

    // Row 0: "Pinned" header if there are pinned notes
    if (hasPinnedNotes && adjustedIndex === 0) {
      showPinnedHeader = true;
      adjustedIndex = 1;
    }

    if (hasPinnedNotes && !showPinnedHeader && adjustedIndex <= pinnedNotes.length) {
      note = pinnedNotes[adjustedIndex - 1];
      isPinnedSection = true;
    } else {
      const otherStart = hasPinnedNotes ? pinnedNotes.length + 1 : 0;
      if (adjustedIndex === otherStart && hasPinnedNotes && hasUnpinnedNotes) {
        showOtherHeader = true;
      } else {
        const noteIndex =
          adjustedIndex - (hasPinnedNotes && hasUnpinnedNotes ? pinnedNotes.length + 2 : 0);
        note = unpinnedNotes[noteIndex];
      }
    }

    if (showPinnedHeader) {
      return (
        <div style={style} className="flex items-center px-4 pt-3 pb-1">
          <svg className="w-3 h-3 text-gray-400 mr-1.5" fill="currentColor" viewBox="0 0 20 20">
            <path d="M10 2L13.09 8.26L20 9.27L15 14.14L16.18 21.02L10 17.77L3.82 21.02L5 14.14L0 9.27L6.91 8.26L10 2Z" />
          </svg>
          <span className="text-xs font-medium text-gray-400 uppercase tracking-wider">
            已置顶
          </span>
        </div>
      );
    }

    if (showOtherHeader) {
      return (
        <div style={style} className="flex items-center px-4 pt-3 pb-1">
          <span className="text-xs font-medium text-gray-400 uppercase tracking-wider">
            全部便签
          </span>
        </div>
      );
    }

    if (!note) return null;

    const isActive = note.id === activeNoteId;
    const preview = stripMarkdown(note.preview).slice(0, 80);

    return (
      <div
        style={style}
        className={`note-card cursor-pointer border-b border-gray-100 ${
          isActive ? "active" : ""
        }`}
        onClick={() => onSelectNote(note.id)}
        onContextMenu={(e) => handleContextMenu(e, note.id)}
      >
        <div className="flex h-full flex-col px-4 py-3">
          <div className="flex items-start justify-between gap-2">
            <h3 className="min-w-0 flex-1 truncate text-sm font-semibold text-gray-800">
              {note.title || "无标题"}
            </h3>
            {note.is_pinned && (
              <svg className="w-3.5 h-3.5 text-blue-500 flex-shrink-0" fill="currentColor" viewBox="0 0 20 20">
                <path d="M10 2L13.09 8.26L20 9.27L15 14.14L16.18 21.02L10 17.77L3.82 21.02L5 14.14L0 9.27L6.91 8.26L10 2Z" />
              </svg>
            )}
          </div>
          <p className="mt-1 min-h-0 flex-1 overflow-hidden text-xs leading-relaxed text-gray-500">
            <span className="line-clamp-2">{preview || "空便签"}</span>
          </p>
          <p className="mt-1 flex-shrink-0 truncate text-xs text-gray-400">
            {formatRelativeTime(note.updated_at)}
          </p>
        </div>
      </div>
    );
  };

  const totalRows =
    (hasPinnedNotes ? pinnedNotes.length + 1 : 0) +
    (hasPinnedNotes && hasUnpinnedNotes ? 1 : 0) +
    unpinnedNotes.length;

  return (
    <div
      className="hidden h-full w-72 flex-col border-r border-gray-200 bg-white md:flex"
      onClick={closeContextMenu}
    >
      {/* Header */}
      <div className="p-4 border-b border-gray-100">
        <div className="flex items-center justify-between mb-3">
          <h1 className="text-lg font-bold text-gray-800">QuickNote</h1>
          {viewMode === "notes" && (
            <button
              onClick={onCreateNote}
              className="w-8 h-8 flex items-center justify-center rounded-lg bg-blue-600 text-white hover:bg-blue-700 transition-colors shadow-sm"
              title="新建便签 (Ctrl+N)"
            >
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
              </svg>
            </button>
          )}
        </div>

        <div className="mb-3 grid grid-cols-2 rounded-xl bg-gray-100 p-1 text-xs font-medium">
          <button onClick={() => onViewModeChange("notes")} className={`rounded-lg px-3 py-2 transition ${viewMode === "notes" ? "bg-white text-gray-900 shadow-sm" : "text-gray-500 hover:text-gray-700"}`}>便签</button>
          <button onClick={() => onViewModeChange("clipboard")} className={`rounded-lg px-3 py-2 transition ${viewMode === "clipboard" ? "bg-white text-gray-900 shadow-sm" : "text-gray-500 hover:text-gray-700"}`}>剪贴板</button>
        </div>

        {/* Search */}
        {viewMode === "notes" && <div className="relative">
          <svg
            className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-400"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
            />
          </svg>
          <input
            type="text"
            placeholder="搜索便签..."
            value={searchQuery}
            onChange={(e) => onSearchChange(e.target.value)}
            className="w-full pl-9 pr-3 py-2 text-sm bg-gray-50 border border-gray-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent placeholder-gray-400"
          />
          {searchQuery && (
            <button
              onClick={() => onSearchChange("")}
              className="absolute right-2 top-1/2 -translate-y-1/2 w-5 h-5 flex items-center justify-center rounded-full text-gray-400 hover:text-gray-600 hover:bg-gray-200"
            >
              <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          )}
        </div>}
      </div>

      {/* Note List (Virtual Scrolling) */}
      {viewMode === "notes" ? <div ref={listWrapRef} className="flex-1 overflow-hidden">
        {notes.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full text-gray-400 p-6">
            <svg className="w-12 h-12 mb-3 opacity-50" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
            </svg>
            <p className="text-sm text-center">
              {searchQuery ? "没有找到匹配的便签" : "还没有便签"}
            </p>
            {!searchQuery && (
              <button
                onClick={onCreateNote}
                className="mt-3 text-sm text-blue-600 hover:text-blue-700 font-medium"
              >
                创建第一个便签
              </button>
            )}
          </div>
        ) : (
          <List<Record<string, never>>
            rowComponent={NoteItem}
            rowCount={totalRows}
            rowHeight={96}
            rowProps={{}}
            style={{ height: Math.max(listHeight, 1), width: "100%" }}
          />
        )}
      </div> : (
        <div className="flex flex-1 flex-col items-center justify-center px-7 text-center">
          <div className="flex h-14 w-14 items-center justify-center rounded-2xl bg-violet-100 text-2xl text-violet-700">▣</div>
          <h3 className="mt-4 text-sm font-semibold text-gray-700">跨设备剪贴板</h3>
          <p className="mt-2 text-xs leading-5 text-gray-400">复制的文本、链接与代码片段会保存在本地，并复用 WebDAV 安全同步。</p>
        </div>
      )}

      {/* Footer */}
      <div className="px-4 py-2 border-t border-gray-100 flex items-center justify-between gap-2">
        {viewMode === "notes" && <button
          onClick={onOpenTrash}
          className="w-8 h-8 flex items-center justify-center rounded text-gray-500 hover:bg-gray-100 hover:text-gray-700"
          title="回收站"
        >
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.9 12.1A2 2 0 0116.1 21H7.9a2 2 0 01-2-1.9L5 7m5 4v6m4-6v6M4 7h16m-3 0V5a2 2 0 00-2-2h-6a2 2 0 00-2 2v2" />
          </svg>
        </button>}
        <p className="text-xs text-gray-400 text-center flex-1">
          {viewMode === "notes" ? `${notes.length} 条便签` : `${clipboardCount} 条记录`}
        </p>
        <button
          onClick={onSync}
          className={`h-8 w-8 rounded text-xs hover:bg-gray-100 ${
            syncStatus === "error" ? "text-red-500" : "text-gray-500"
          }`}
          title={formatSyncStatus(syncStatus)}
        >
          {syncStatus === "syncing" ? "···" : "↻"}
        </button>
        <button
          onClick={onOpenSettings}
          className="h-8 w-8 rounded text-gray-500 hover:bg-gray-100"
          title="同步设置"
        >
          ⚙
        </button>
      </div>

      {/* Context Menu */}
      {contextMenu && (
        <div
          className="fixed z-50 bg-white rounded-lg shadow-lg border border-gray-200 py-1 min-w-[140px] animate-fade-in"
          style={{ left: contextMenu.x, top: contextMenu.y }}
        >
          <button
            onClick={() => {
              onTogglePin(contextMenu.noteId);
              closeContextMenu();
            }}
            className="w-full px-3 py-2 text-left text-sm hover:bg-gray-50 flex items-center gap-2"
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 5a2 2 0 012-2h10a2 2 0 012 2v16l-7-3.5L5 21V5z" />
            </svg>
            置顶 / 取消置顶
          </button>
          <hr className="my-1 border-gray-100" />
          <button
            onClick={() => {
              onDeleteNote(contextMenu.noteId);
              closeContextMenu();
            }}
            className="w-full px-3 py-2 text-left text-sm text-red-600 hover:bg-red-50 flex items-center gap-2"
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
            </svg>
            删除
          </button>
        </div>
      )}
    </div>
  );
}

function formatSyncStatus(status: SyncStatus): string {
  if (status === "disabled") return "同步未启用";
  if (status === "syncing") return "正在同步";
  if (status === "synced") return "同步完成";
  if (status === "error") return "同步失败";
  return "立即同步";
}
