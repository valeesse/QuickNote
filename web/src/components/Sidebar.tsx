import { useState, useMemo } from "react";
import {
  Plus,
  Search,
  X,
  Trash2,
  Star,
  FileText,
  Clipboard,
  LogOut,
} from "lucide-react";
import type { AppView, NoteSummary } from "@/types";

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
  userEmail: string;
  onLogout: () => void;
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

function stripHtml(text: string): string {
  return text.replace(/<[^>]+>/g, " ").replace(/\s+/g, " ").trim();
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
  userEmail,
  onLogout,
}: SidebarProps) {
  const [contextMenu, setContextMenu] = useState<{
    noteId: string;
    x: number;
    y: number;
  } | null>(null);

  const pinnedNotes = useMemo(() => notes.filter((n) => n.is_pinned), [notes]);
  const unpinnedNotes = useMemo(() => notes.filter((n) => !n.is_pinned), [notes]);

  const handleContextMenu = (e: React.MouseEvent, noteId: string) => {
    e.preventDefault();
    setContextMenu({ noteId, x: e.clientX, y: e.clientY });
  };

  const closeContextMenu = () => setContextMenu(null);

  return (
    <div
      className="flex h-full w-72 flex-col border-r border-gray-200 bg-white"
      onClick={closeContextMenu}
    >
      {/* Header */}
      <div className="border-b border-gray-100 p-4">
        <div className="mb-3 flex items-center justify-between">
          <h1 className="text-lg font-bold text-gray-800">QuickNote</h1>
          {viewMode === "notes" && (
            <button
              onClick={onCreateNote}
              className="flex h-8 w-8 items-center justify-center rounded-lg bg-blue-600 text-white shadow-sm transition-colors hover:bg-blue-700"
              title="新建便签 (Ctrl+N)"
            >
              <Plus className="h-4 w-4" />
            </button>
          )}
        </div>

        <div className="mb-3 grid grid-cols-2 rounded-xl bg-gray-100 p-1 text-xs font-medium">
          <button
            onClick={() => onViewModeChange("notes")}
            className={`rounded-lg px-3 py-2 transition ${viewMode === "notes" ? "bg-white text-gray-900 shadow-sm" : "text-gray-500 hover:text-gray-700"}`}
          >
            便签
          </button>
          <button
            onClick={() => onViewModeChange("clipboard")}
            className={`rounded-lg px-3 py-2 transition ${viewMode === "clipboard" ? "bg-white text-gray-900 shadow-sm" : "text-gray-500 hover:text-gray-700"}`}
          >
            剪贴板
          </button>
        </div>

        {viewMode === "notes" && (
          <div className="relative">
            <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-gray-400" />
            <input
              type="text"
              placeholder="搜索便签..."
              value={searchQuery}
              onChange={(e) => onSearchChange(e.target.value)}
              className="w-full rounded-lg border border-gray-200 bg-gray-50 py-2 pl-9 pr-3 text-sm placeholder-gray-400 outline-none focus:border-blue-500 focus:ring-2 focus:ring-blue-500/20"
            />
            {searchQuery && (
              <button
                onClick={() => onSearchChange("")}
                className="absolute right-2 top-1/2 flex h-5 w-5 -translate-y-1/2 items-center justify-center rounded-full text-gray-400 hover:bg-gray-200 hover:text-gray-600"
              >
                <X className="h-3 w-3" />
              </button>
            )}
          </div>
        )}
      </div>

      {/* Note List */}
      {viewMode === "notes" ? (
        <div className="flex-1 overflow-y-auto">
          {notes.length === 0 ? (
            <div className="flex h-full flex-col items-center justify-center p-6 text-gray-400">
              <FileText className="mb-3 h-12 w-12 opacity-50" />
              <p className="text-center text-sm">
                {searchQuery ? "没有找到匹配的便签" : "还没有便签"}
              </p>
              {!searchQuery && (
                <button
                  onClick={onCreateNote}
                  className="mt-3 text-sm font-medium text-blue-600 hover:text-blue-700"
                >
                  创建第一个便签
                </button>
              )}
            </div>
          ) : (
            <>
              {pinnedNotes.length > 0 && (
                <>
                  <div className="flex items-center px-4 pt-3 pb-1">
                    <Star className="mr-1.5 h-3 w-3 text-gray-400" fill="currentColor" />
                    <span className="text-xs font-medium uppercase tracking-wider text-gray-400">
                      已置顶
                    </span>
                  </div>
                  {pinnedNotes.map((note) => (
                    <NoteCard
                      key={note.id}
                      note={note}
                      isActive={note.id === activeNoteId}
                      onSelect={onSelectNote}
                      onContextMenu={handleContextMenu}
                    />
                  ))}
                </>
              )}
              {pinnedNotes.length > 0 && unpinnedNotes.length > 0 && (
                <div className="flex items-center px-4 pt-3 pb-1">
                  <span className="text-xs font-medium uppercase tracking-wider text-gray-400">
                    全部便签
                  </span>
                </div>
              )}
              {unpinnedNotes.map((note) => (
                <NoteCard
                  key={note.id}
                  note={note}
                  isActive={note.id === activeNoteId}
                  onSelect={onSelectNote}
                  onContextMenu={handleContextMenu}
                />
              ))}
            </>
          )}
        </div>
      ) : (
        <div className="flex flex-1 flex-col items-center justify-center px-7 text-center">
          <div className="flex h-14 w-14 items-center justify-center rounded-2xl bg-violet-100 text-violet-700">
            <Clipboard className="h-7 w-7" />
          </div>
          <h3 className="mt-4 text-sm font-semibold text-gray-700">跨设备剪贴板</h3>
          <p className="mt-2 text-xs leading-5 text-gray-400">
            复制的文本、链接与代码片段会通过云同步在各设备间共享。
          </p>
        </div>
      )}

      {/* Footer */}
      <div className="flex items-center justify-between border-t border-gray-100 px-4 py-2">
        <p className="flex-1 text-center text-xs text-gray-400">
          {viewMode === "notes" ? `${notes.length} 条便签` : `${clipboardCount} 条记录`}
        </p>
        <button
          onClick={onLogout}
          className="flex h-8 w-8 items-center justify-center rounded text-gray-500 hover:bg-gray-100"
          title={`退出登录 (${userEmail})`}
        >
          <LogOut className="h-4 w-4" />
        </button>
      </div>

      {/* Context Menu */}
      {contextMenu && (
        <div
          className="fixed z-50 min-w-[140px] rounded-lg border border-gray-200 bg-white py-1 shadow-lg"
          style={{ left: contextMenu.x, top: contextMenu.y }}
        >
          <button
            onClick={() => {
              onTogglePin(contextMenu.noteId);
              closeContextMenu();
            }}
            className="flex w-full items-center gap-2 px-3 py-2 text-left text-sm hover:bg-gray-50"
          >
            <Star className="h-4 w-4" />
            置顶 / 取消置顶
          </button>
          <hr className="my-1 border-gray-100" />
          <button
            onClick={() => {
              onDeleteNote(contextMenu.noteId);
              closeContextMenu();
            }}
            className="flex w-full items-center gap-2 px-3 py-2 text-left text-sm text-red-600 hover:bg-red-50"
          >
            <Trash2 className="h-4 w-4" />
            删除
          </button>
        </div>
      )}
    </div>
  );
}

function NoteCard({
  note,
  isActive,
  onSelect,
  onContextMenu,
}: {
  note: NoteSummary;
  isActive: boolean;
  onSelect: (id: string) => void;
  onContextMenu: (e: React.MouseEvent, id: string) => void;
}) {
  const preview = stripHtml(note.preview).slice(0, 80);
  return (
    <div
      className={`cursor-pointer border-b border-gray-100 ${isActive ? "active bg-blue-50 border-l-[3px] border-l-blue-500" : ""}`}
      onClick={() => onSelect(note.id)}
      onContextMenu={(e) => onContextMenu(e, note.id)}
    >
      <div className="flex flex-col px-4 py-3">
        <div className="flex items-start justify-between gap-2">
          <h3 className="min-w-0 flex-1 truncate text-sm font-semibold text-gray-800">
            {note.title || "无标题"}
          </h3>
          {note.is_pinned && (
            <Star className="h-3.5 w-3.5 flex-shrink-0 text-blue-500" fill="currentColor" />
          )}
        </div>
        <p className="mt-1 line-clamp-2 text-xs leading-relaxed text-gray-500">
          {preview || "空便签"}
        </p>
        <p className="mt-1 truncate text-xs text-gray-400">
          {formatRelativeTime(note.updated_at)}
        </p>
      </div>
    </div>
  );
}
