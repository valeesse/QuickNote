import { useEffect, useMemo, useState } from "react";
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
import { NoteCard, NoteSectionLabel } from "@ui/components/NoteCard";
import { stripHtml } from "@ui/utils/html";

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
    setContextMenu({ noteId, ...getMenuPosition(e.clientX, e.clientY) });
  };

  const closeContextMenu = () => setContextMenu(null);

  useEffect(() => {
    if (!contextMenu) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") closeContextMenu();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [contextMenu]);

  return (
    <div
      className="hidden h-full w-72 flex-col border-r border-gray-200 bg-white md:flex"
      onClick={closeContextMenu}
    >
      {/* Header */}
      <div className="border-b border-gray-100 p-4">
        <div className="mb-3 flex items-center justify-between">
          <h1 className="text-lg font-bold text-gray-800">QuickNote</h1>
          {viewMode === "notes" && (
            <button
              type="button"
              onClick={onCreateNote}
              className="focus-ring flex h-8 w-8 items-center justify-center rounded-lg bg-blue-600 text-white shadow-sm transition-colors hover:bg-blue-700"
              title="新建便签 (Ctrl+N)"
              aria-label="新建便签"
            >
              <Plus className="h-4 w-4" />
            </button>
          )}
        </div>

        <div className="mb-3 grid grid-cols-2 rounded-xl bg-gray-100 p-1 text-xs font-medium">
          <button
            type="button"
            onClick={() => onViewModeChange("notes")}
            className={`focus-ring rounded-lg px-3 py-2 transition ${viewMode === "notes" ? "bg-white text-gray-900 shadow-sm" : "text-gray-500 hover:text-gray-700"}`}
            aria-pressed={viewMode === "notes"}
          >
            便签
          </button>
          <button
            type="button"
            onClick={() => onViewModeChange("clipboard")}
            className={`focus-ring rounded-lg px-3 py-2 transition ${viewMode === "clipboard" ? "bg-white text-gray-900 shadow-sm" : "text-gray-500 hover:text-gray-700"}`}
            aria-pressed={viewMode === "clipboard"}
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
                type="button"
                onClick={() => onSearchChange("")}
                className="focus-ring absolute right-2 top-1/2 flex h-5 w-5 -translate-y-1/2 items-center justify-center rounded-full text-gray-400 hover:bg-gray-200 hover:text-gray-600"
                aria-label="清空搜索"
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
                  type="button"
                  onClick={onCreateNote}
                  className="focus-ring mt-3 rounded px-2 py-1 text-sm font-medium text-blue-600 hover:text-blue-700"
                >
                  创建第一个便签
                </button>
              )}
            </div>
          ) : (
            <>
              {pinnedNotes.length > 0 && (
                <>
                  <NoteSectionLabel
                    icon={<Star className="mr-1.5 h-3 w-3 text-gray-400" fill="currentColor" />}
                  >
                    已置顶
                  </NoteSectionLabel>
                  {pinnedNotes.map((note) => (
                    <NoteCard
                      key={note.id}
                      id={note.id}
                      title={note.title}
                      preview={stripHtml(note.preview).slice(0, 80)}
                      updatedAt={note.updated_at}
                      isPinned={note.is_pinned}
                      isActive={note.id === activeNoteId}
                      onSelect={onSelectNote}
                      onContextMenu={handleContextMenu}
                    />
                  ))}
                </>
              )}
              {pinnedNotes.length > 0 && unpinnedNotes.length > 0 && (
                <NoteSectionLabel>全部便签</NoteSectionLabel>
              )}
              {unpinnedNotes.map((note) => (
                <NoteCard
                  key={note.id}
                  id={note.id}
                  title={note.title}
                  preview={stripHtml(note.preview).slice(0, 80)}
                  updatedAt={note.updated_at}
                  isPinned={note.is_pinned}
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
          type="button"
          onClick={onLogout}
          className="focus-ring flex h-8 w-8 items-center justify-center rounded text-gray-500 hover:bg-gray-100"
          title={`退出登录 (${userEmail})`}
          aria-label={`退出登录，当前用户 ${userEmail}`}
        >
          <LogOut className="h-4 w-4" />
        </button>
      </div>

      {/* Context Menu */}
      {contextMenu && (
        <div
          role="menu"
          aria-label="便签操作"
          className="fixed z-50 min-w-[140px] rounded-lg border border-gray-200 bg-white py-1 shadow-lg"
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
            className="flex w-full items-center gap-2 px-3 py-2 text-left text-sm hover:bg-gray-50"
          >
            <Star className="h-4 w-4" />
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

function getMenuPosition(x: number, y: number): { x: number; y: number } {
  const margin = 8;
  const menuWidth = 160;
  const menuHeight = 104;
  return {
    x: Math.min(x, window.innerWidth - menuWidth - margin),
    y: Math.min(y, window.innerHeight - menuHeight - margin),
  };
}
