import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { List } from "react-window";
import {
  Plus,
  Search,
  X,
  Trash2,
  RefreshCw,
  Settings,
  Star,
  Bookmark,
  FileText,
  Clipboard,
} from "lucide-react";
import type { AppView, NoteSummary, SyncStatus } from "@/types";
import { NoteCard, NoteSectionLabel } from "@ui/components/NoteCard";
import { stripMarkdown } from "@ui/utils/html";

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
      setContextMenu({ noteId, ...getMenuPosition(e.clientX, e.clientY) });
    },
    []
  );

  const closeContextMenu = useCallback(() => {
    setContextMenu(null);
  }, []);

  useEffect(() => {
    if (!contextMenu) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") closeContextMenu();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [closeContextMenu, contextMenu]);

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
        <NoteSectionLabel
          style={style}
          icon={<Star className="mr-1.5 h-3 w-3 text-gray-400" fill="currentColor" />}
        >
          已置顶
        </NoteSectionLabel>
      );
    }

    if (showOtherHeader) {
      return <NoteSectionLabel style={style}>全部便签</NoteSectionLabel>;
    }

    if (!note) return null;

    const isActive = note.id === activeNoteId;
    const preview = stripMarkdown(note.preview).slice(0, 80);

    return (
      <NoteCard
        style={style}
        id={note.id}
        title={note.title}
        preview={preview}
        updatedAt={note.updated_at}
        isPinned={note.is_pinned}
        isActive={isActive}
        onSelect={onSelectNote}
        onContextMenu={handleContextMenu}
      />
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
              type="button"
              onClick={onCreateNote}
              className="focus-ring flex h-8 w-8 items-center justify-center rounded-lg bg-blue-600 text-white shadow-sm transition-colors hover:bg-blue-700"
              title="新建便签 (Ctrl+N)"
              aria-label="新建便签"
            >
              <Plus className="w-4 h-4" />
            </button>
          )}
        </div>

        <div className="mb-3 grid grid-cols-2 rounded-xl bg-gray-100 p-1 text-xs font-medium">
          <button type="button" onClick={() => onViewModeChange("notes")} aria-pressed={viewMode === "notes"} className={`focus-ring rounded-lg px-3 py-2 transition ${viewMode === "notes" ? "bg-white text-gray-900 shadow-sm" : "text-gray-500 hover:text-gray-700"}`}>便签</button>
          <button type="button" onClick={() => onViewModeChange("clipboard")} aria-pressed={viewMode === "clipboard"} className={`focus-ring rounded-lg px-3 py-2 transition ${viewMode === "clipboard" ? "bg-white text-gray-900 shadow-sm" : "text-gray-500 hover:text-gray-700"}`}>剪贴板</button>
        </div>

        {/* Search */}
        {viewMode === "notes" && <div className="relative">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-400" />
          <input
            type="text"
            placeholder="搜索便签..."
            value={searchQuery}
            onChange={(e) => onSearchChange(e.target.value)}
            className="w-full pl-9 pr-3 py-2 text-sm bg-gray-50 border border-gray-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent placeholder-gray-400"
          />
          {searchQuery && (
            <button
              type="button"
              onClick={() => onSearchChange("")}
              className="focus-ring absolute right-2 top-1/2 flex h-5 w-5 -translate-y-1/2 items-center justify-center rounded-full text-gray-400 hover:bg-gray-200 hover:text-gray-600"
              aria-label="清空搜索"
            >
              <X className="w-3 h-3" />
            </button>
          )}
        </div>}
      </div>

      {/* Note List (Virtual Scrolling) */}
      {viewMode === "notes" ? <div ref={listWrapRef} className="flex-1 overflow-hidden">
        {notes.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full text-gray-400 p-6">
            <FileText className="w-12 h-12 mb-3 opacity-50" />
            <p className="text-sm text-center">
              {searchQuery ? "没有找到匹配的便签" : "还没有便签"}
            </p>
            {!searchQuery && (
              <button
                type="button"
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
          <div className="flex h-14 w-14 items-center justify-center rounded-2xl bg-violet-100 text-violet-700">
            <Clipboard className="h-7 w-7" />
          </div>
          <h3 className="mt-4 text-sm font-semibold text-gray-700">跨设备剪贴板</h3>
          <p className="mt-2 text-xs leading-5 text-gray-400">复制的文本、链接与代码片段会保存在本地，并复用 WebDAV 安全同步。</p>
        </div>
      )}

      {/* Footer */}
      <div className="px-4 py-2 border-t border-gray-100 flex items-center justify-between gap-2">
        {viewMode === "notes" && <button
          type="button"
          onClick={onOpenTrash}
          className="focus-ring flex h-8 w-8 items-center justify-center rounded text-gray-500 hover:bg-gray-100 hover:text-gray-700"
          title="回收站"
          aria-label="打开回收站"
        >
          <Trash2 className="w-4 h-4" />
        </button>}
        <p className="text-xs text-gray-400 text-center flex-1">
          {viewMode === "notes" ? `${notes.length} 条便签` : `${clipboardCount} 条记录`}
        </p>
        <button
          type="button"
          onClick={onSync}
          className={`focus-ring flex h-8 w-8 items-center justify-center rounded text-xs hover:bg-gray-100 ${
            syncStatus === "error" ? "text-red-500" : "text-gray-500"
          }`}
          title={formatSyncStatus(syncStatus)}
          aria-label={formatSyncStatus(syncStatus)}
        >
          <RefreshCw className={`w-4 h-4 ${syncStatus === "syncing" ? "animate-spin" : ""}`} />
        </button>
        <button
          type="button"
          onClick={onOpenSettings}
          className="focus-ring flex h-8 w-8 items-center justify-center rounded text-gray-500 hover:bg-gray-100"
          title="同步设置"
          aria-label="打开同步设置"
        >
          <Settings className="w-4 h-4" />
        </button>
      </div>

      {/* Context Menu */}
      {contextMenu && (
        <div
          role="menu"
          aria-label="便签操作"
          className="fixed z-50 min-w-[140px] animate-fade-in rounded-lg border border-gray-200 bg-white py-1 shadow-lg"
          style={{ left: contextMenu.x, top: contextMenu.y }}
          onClick={(event) => event.stopPropagation()}
        >
          <button
            type="button"
            role="menuitem"
            onClick={() => {
              onTogglePin(contextMenu.noteId);
              closeContextMenu();
            }}
            className="w-full px-3 py-2 text-left text-sm hover:bg-gray-50 flex items-center gap-2"
          >
            <Bookmark className="w-4 h-4" />
            置顶 / 取消置顶
          </button>
          <hr className="my-1 border-gray-100" />
          <button
            type="button"
            role="menuitem"
            onClick={() => {
              onDeleteNote(contextMenu.noteId);
              closeContextMenu();
            }}
            className="w-full px-3 py-2 text-left text-sm text-red-600 hover:bg-red-50 flex items-center gap-2"
          >
            <Trash2 className="w-4 h-4" />
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

function getMenuPosition(x: number, y: number): { x: number; y: number } {
  const margin = 8;
  const menuWidth = 160;
  const menuHeight = 104;
  return {
    x: Math.min(x, window.innerWidth - menuWidth - margin),
    y: Math.min(y, window.innerHeight - menuHeight - margin),
  };
}
