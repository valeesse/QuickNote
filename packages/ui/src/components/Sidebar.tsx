import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Clipboard,
  FileText,
  LogOut,
  Pin,
  PinOff,
  Plus,
  RefreshCw,
  Search,
  Settings,
  StickyNote,
  Tags,
  Trash2,
  X,
} from "lucide-react";
import type { AppView, ClipboardItem, NoteSummary, TagSummary } from "@contracts";
import { formatRelativeTime } from "../utils/format";
import { stripHtml, stripMarkdown } from "../utils/html";

export type SidebarSyncStatus = "disabled" | "idle" | "syncing" | "synced" | "error";

export interface SidebarProps {
  viewMode: AppView;
  onViewModeChange: (mode: AppView) => void;
  clipboardCount: number;
  clipboardItems: ClipboardItem[];
  notes: NoteSummary[];
  tags: TagSummary[];
  selectedTag: string | null;
  activeNoteId: string | null;
  searchQuery: string;
  onSearchChange: (query: string) => void;
  onSelectTag: (tag: string | null) => void;
  onSelectNote: (id: string) => void;
  onCreateNote: () => void;
  onDeleteNote: (id: string) => void;
  onTogglePin: (id: string) => void;
  onReorderNotes: (orderedIds: string[], isPinned: boolean) => void;
  onOpenTrash: () => void;
  isTrashOpen: boolean;
  onSelectClipboardItem: (id: string) => void;
  onCreateNoteFromClipboard: (id: string) => void;
  syncStatus?: SidebarSyncStatus;
  onSync?: () => void;
  onOpenSettings?: () => void;
  settingsLabel?: string;
  userEmail?: string;
  onLogout?: () => void;
}

type ClipboardContextMenu = {
  itemId: string;
  x: number;
  y: number;
};

type DropPlacement = "before" | "after";

export function Sidebar({
  viewMode,
  onViewModeChange,
  clipboardCount,
  clipboardItems,
  notes,
  tags,
  selectedTag,
  activeNoteId,
  searchQuery,
  onSearchChange,
  onSelectTag,
  onSelectNote,
  onCreateNote,
  onDeleteNote,
  onTogglePin,
  onReorderNotes,
  onOpenTrash,
  isTrashOpen,
  onSelectClipboardItem,
  onCreateNoteFromClipboard,
  syncStatus,
  onSync,
  onOpenSettings,
  settingsLabel = "设置",
  userEmail,
  onLogout,
}: SidebarProps) {
  const [dragReadyId, setDragReadyId] = useState<string | null>(null);
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const [dragTarget, setDragTarget] = useState<{ id: string; placement: DropPlacement } | null>(null);
  const [dragPosition, setDragPosition] = useState<{ x: number; y: number } | null>(null);
  const [clipboardContextMenu, setClipboardContextMenu] = useState<ClipboardContextMenu | null>(null);
  const longPressTimerRef = useRef<number | null>(null);
  const pointerDragRef = useRef<{ noteId: string; pointerId: number } | null>(null);
  const dragTargetRef = useRef<{ id: string; placement: DropPlacement } | null>(null);
  const suppressClickRef = useRef(false);

  const closeMenus = useCallback(() => setClipboardContextMenu(null), []);

  useEffect(() => {
    if (!clipboardContextMenu) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") closeMenus();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [clipboardContextMenu, closeMenus]);

  useEffect(() => {
    return () => clearLongPressTimer(longPressTimerRef);
  }, []);

  const pinnedNotes = useMemo(() => notes.filter((note) => note.is_pinned), [notes]);
  const unpinnedNotes = useMemo(() => notes.filter((note) => !note.is_pinned), [notes]);
  const pinnedClipboardItems = useMemo(
    () => clipboardItems.filter((item) => item.is_pinned).slice(0, 4),
    [clipboardItems],
  );
  const recentClipboardItems = useMemo(
    () => clipboardItems.filter((item) => !item.is_pinned).slice(0, 8),
    [clipboardItems],
  );

  const startLongPress = (noteId: string, pointerId: number) => {
    clearLongPressTimer(longPressTimerRef);
    pointerDragRef.current = { noteId, pointerId };
    longPressTimerRef.current = window.setTimeout(() => {
      setDragReadyId(noteId);
      setDraggingId(noteId);
      suppressClickRef.current = true;
    }, 260);
  };

  const resetLongPress = () => {
    clearLongPressTimer(longPressTimerRef);
    pointerDragRef.current = null;
    dragTargetRef.current = null;
    setDragTarget(null);
    setDragPosition(null);
    if (!draggingId) setDragReadyId(null);
  };

  const handleDropNote = (
    sourceId: string,
    targetId: string,
    placement: DropPlacement,
    targetPinnedOverride: boolean | null = null,
  ) => {
    if (sourceId === targetId) return;
    const targetNote = notes.find((note) => note.id === targetId);
    const sourceNote = notes.find((note) => note.id === sourceId);
    if (!targetNote || !sourceNote) return;
    const targetPinned = targetPinnedOverride ?? targetNote.is_pinned;
    const group = targetPinned ? pinnedNotes : unpinnedNotes;
    const dragged = { ...sourceNote, is_pinned: targetPinned };

    const next = group.filter((note) => note.id !== sourceId);
    const targetIndex = next.findIndex((note) => note.id === targetId);
    const insertIndex = targetIndex + (placement === "after" ? 1 : 0);
    next.splice(Math.max(insertIndex, 0), 0, dragged);
    onReorderNotes(next.map((note) => note.id), targetPinned);
  };

  const finishPointerDrag = (targetId: string | null) => {
    const sourceId = pointerDragRef.current?.noteId;
    const targetGroup = dragPosition
      ? document.elementFromPoint(dragPosition.x, dragPosition.y)?.closest<HTMLElement>("[data-note-group]")
      : null;
    const groupPinned =
      targetGroup?.dataset.noteGroup === "pinned"
        ? true
        : targetGroup?.dataset.noteGroup === "all"
          ? false
          : null;
    const activeDragTarget = dragTargetRef.current;
    const fallbackTargetId =
      groupPinned === true
        ? pinnedNotes[pinnedNotes.length - 1]?.id ?? null
        : groupPinned === false
          ? unpinnedNotes[unpinnedNotes.length - 1]?.id ?? null
          : null;
    const effectiveTargetId = targetId ?? activeDragTarget?.id ?? fallbackTargetId;
    const placement = activeDragTarget?.id === effectiveTargetId ? activeDragTarget.placement : "before";
    pointerDragRef.current = null;
    clearLongPressTimer(longPressTimerRef);
    setDraggingId(null);
    setDragReadyId(null);
    dragTargetRef.current = null;
    setDragTarget(null);
    setDragPosition(null);
    if (sourceId && dragReadyId) {
      suppressClickRef.current = true;
      window.setTimeout(() => {
        suppressClickRef.current = false;
      }, 0);
    }
    if (sourceId && !effectiveTargetId && groupPinned !== null) {
      onReorderNotes([sourceId], groupPinned);
      return;
    }
    if (!sourceId || !effectiveTargetId) return;
    handleDropNote(sourceId, effectiveTargetId, placement, groupPinned);
  };

  const updateDragTarget = (clientX: number, clientY: number) => {
    const sourceId = pointerDragRef.current?.noteId;
    if (!sourceId || !dragReadyId) return;
    setDragPosition({ x: clientX, y: clientY });
    const target = document
      .elementFromPoint(clientX, clientY)
      ?.closest<HTMLElement>("[data-note-id]");
    const targetId = target?.dataset.noteId;
    if (!targetId || targetId === sourceId) {
      setDragTarget(null);
      dragTargetRef.current = null;
      return;
    }
    const sourceNote = notes.find((note) => note.id === sourceId);
    const targetNote = notes.find((note) => note.id === targetId);
    if (!sourceNote || !targetNote) {
      setDragTarget(null);
      dragTargetRef.current = null;
      return;
    }
    const rect = target.getBoundingClientRect();
    const nextTarget: { id: string; placement: DropPlacement } = {
      id: targetId,
      placement: clientY < rect.top + rect.height / 2 ? "before" : "after",
    };
    if (
      dragTargetRef.current?.id === nextTarget.id &&
      dragTargetRef.current?.placement === nextTarget.placement
    ) {
      return;
    }
    dragTargetRef.current = nextTarget;
    setDragTarget(nextTarget);
  };

  const selectNote = (id: string) => {
    if (suppressClickRef.current) return;
    onSelectNote(id);
  };

  const floatingNote = draggingId ? notes.find((note) => note.id === draggingId) : null;

  return (
    <div
      className="hidden h-full w-72 flex-col border-r border-gray-200 bg-white md:flex"
      onClick={closeMenus}
    >
      <div className="border-b border-gray-100 p-4">
        <div className="mb-3 flex items-center justify-between">
          <h1 className="text-lg font-bold text-gray-800">QuickNote</h1>
          {viewMode === "notes" && (
            <button
              type="button"
              onClick={() => onCreateNote()}
              className="focus-ring flex h-8 w-8 items-center justify-center rounded-lg bg-blue-600 text-white shadow-sm transition-colors hover:bg-blue-700"
              title="新建便签 (Ctrl+N)"
              aria-label="新建便签"
            >
              <Plus className="h-4 w-4" />
            </button>
          )}
        </div>

        <div className="mb-3 grid grid-cols-2 rounded-xl bg-gray-100 p-1 text-xs font-medium">
          <button type="button" onClick={() => onViewModeChange("notes")} aria-pressed={viewMode === "notes"} className={`focus-ring rounded-lg px-3 py-2 transition ${viewMode === "notes" ? "bg-white text-gray-900 shadow-sm" : "text-gray-500 hover:text-gray-700"}`}>便签</button>
          <button type="button" onClick={() => onViewModeChange("clipboard")} aria-pressed={viewMode === "clipboard"} className={`focus-ring rounded-lg px-3 py-2 transition ${viewMode === "clipboard" ? "bg-white text-gray-900 shadow-sm" : "text-gray-500 hover:text-gray-700"}`}>剪贴板</button>
        </div>

        {viewMode === "notes" && (
          <div className="relative">
            <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-gray-400" />
            <input
              type="text"
              placeholder="搜索便签..."
              value={searchQuery}
              onChange={(event) => onSearchChange(event.target.value)}
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

      {viewMode === "notes" ? (
        <div className="note-sidebar">
          {notes.length === 0 ? (
            <>
              {tags.length > 0 && (
                <NoteTagSection tags={tags} selectedTag={selectedTag} onSelectTag={onSelectTag} />
              )}
              <div className="clipboard-sidebar__empty">
                <div>
                  <div className="mx-auto flex h-14 w-14 items-center justify-center rounded-2xl bg-blue-100 text-blue-700">
                    <FileText className="h-7 w-7" />
                  </div>
                  <h3 className="mt-4 text-sm font-semibold text-gray-700">
                    {searchQuery || selectedTag ? "没有找到匹配的便签" : "还没有便签"}
                  </h3>
                  {!searchQuery && !selectedTag && (
                    <button type="button" onClick={() => onCreateNote()} className="mt-3 text-sm font-medium text-blue-600 hover:text-blue-700">
                      创建第一个便签
                    </button>
                  )}
                </div>
              </div>
            </>
          ) : (
            <>
              {(pinnedNotes.length > 0 || draggingId) && (
                <>
                {tags.length > 0 && (
                  <NoteTagSection
                    tags={tags}
                    selectedTag={selectedTag}
                    onSelectTag={onSelectTag}
                  />
                )}
                <NoteSidebarSection title="固定" group="pinned">
                  {pinnedNotes.map((note) => (
                    <NoteSidebarItem
                      key={note.id}
                      note={note}
                      active={note.id === activeNoteId}
                      dragging={note.id === draggingId}
                      dragReady={note.id === dragReadyId}
                      dropPlacement={dragTarget?.id === note.id ? dragTarget.placement : null}
                      onSelect={selectNote}
                      onDelete={onDeleteNote}
                      onTogglePin={onTogglePin}
                      onSelectTag={onSelectTag}
                      onPointerDown={startLongPress}
                      onPointerUp={finishPointerDrag}
                      onPointerLeave={resetLongPress}
                      onPointerMove={updateDragTarget}
                    />
                  ))}
                </NoteSidebarSection>
                </>
              )}
              {pinnedNotes.length === 0 && !draggingId && tags.length > 0 && (
                <NoteTagSection tags={tags} selectedTag={selectedTag} onSelectTag={onSelectTag} />
              )}
              <NoteSidebarSection title="全部" group="all">
                {unpinnedNotes.map((note) => (
                  <NoteSidebarItem
                    key={note.id}
                    note={note}
                    active={note.id === activeNoteId}
                    dragging={note.id === draggingId}
                    dragReady={note.id === dragReadyId}
                    dropPlacement={dragTarget?.id === note.id ? dragTarget.placement : null}
                    onSelect={selectNote}
                    onDelete={onDeleteNote}
                    onTogglePin={onTogglePin}
                    onSelectTag={onSelectTag}
                    onPointerDown={startLongPress}
                    onPointerUp={finishPointerDrag}
                    onPointerLeave={resetLongPress}
                    onPointerMove={updateDragTarget}
                  />
                ))}
              </NoteSidebarSection>
            </>
          )}
        </div>
      ) : (
        <ClipboardSidebar
          items={recentClipboardItems}
          pinnedItems={pinnedClipboardItems}
          onSelect={onSelectClipboardItem}
          onContextMenu={(event, itemId) => {
            event.preventDefault();
            setClipboardContextMenu({ itemId, ...getMenuPosition(event.clientX, event.clientY) });
          }}
        />
      )}

      <div className="flex items-center justify-between gap-2 border-t border-gray-100 px-4 py-2">
        {viewMode === "notes" && (
          <button
            type="button"
            onClick={onOpenTrash}
            className={`focus-ring flex h-8 w-8 items-center justify-center rounded hover:bg-gray-100 ${isTrashOpen ? "text-blue-600" : "text-gray-500 hover:text-gray-700"}`}
            title={isTrashOpen ? "收起回收站" : "回收站"}
            aria-label={isTrashOpen ? "收起回收站" : "打开回收站"}
          >
            <Trash2 className="h-4 w-4" />
          </button>
        )}
        <p className="flex-1 text-center text-xs text-gray-400">
          {viewMode === "notes" ? `${notes.length} 条便签` : `${clipboardCount} 条记录`}
        </p>
        {onSync && syncStatus && (
          <button
            type="button"
            onClick={onSync}
            className={`focus-ring flex h-8 w-8 items-center justify-center rounded text-xs hover:bg-gray-100 ${
              syncStatus === "error" ? "text-red-500" : "text-gray-500"
            }`}
            title={formatSyncStatus(syncStatus)}
            aria-label={formatSyncStatus(syncStatus)}
          >
            <RefreshCw className={`h-4 w-4 ${syncStatus === "syncing" ? "animate-spin" : ""}`} />
          </button>
        )}
        {onOpenSettings && (
          <button
            type="button"
            onClick={onOpenSettings}
            className="focus-ring flex h-8 w-8 items-center justify-center rounded text-gray-500 hover:bg-gray-100"
            title={settingsLabel}
            aria-label={settingsLabel}
          >
            <Settings className="h-4 w-4" />
          </button>
        )}
        {onLogout && (
          <button
            type="button"
            onClick={onLogout}
            className="focus-ring flex h-8 w-8 items-center justify-center rounded text-gray-500 hover:bg-gray-100"
            title={userEmail ? `退出登录 (${userEmail})` : "退出登录"}
            aria-label={userEmail ? `退出登录，当前用户 ${userEmail}` : "退出登录"}
          >
            <LogOut className="h-4 w-4" />
          </button>
        )}
      </div>

      {clipboardContextMenu && (
        <div
          role="menu"
          aria-label="剪贴板操作"
          className="animate-menu-in fixed z-50 min-w-[170px] rounded-lg border border-gray-200 bg-white py-1 shadow-lg"
          style={{ left: clipboardContextMenu.x, top: clipboardContextMenu.y }}
          onClick={(event) => event.stopPropagation()}
        >
          <button
            type="button"
            role="menuitem"
            onClick={() => {
              onCreateNoteFromClipboard(clipboardContextMenu.itemId);
              closeMenus();
            }}
            className="flex w-full items-center gap-2 px-3 py-2 text-left text-sm hover:bg-gray-50"
          >
            <StickyNote className="h-4 w-4" />
            从剪贴板创建便签
          </button>
        </div>
      )}

      {floatingNote && dragPosition && (
        <div
          className="note-sidebar__drag-preview"
          style={{ left: dragPosition.x, top: dragPosition.y }}
        >
          <span className="note-sidebar__item-title">
            <span className="min-w-0 truncate">{floatingNote.title || "无标题"}</span>
          </span>
          <span className="note-sidebar__item-text">{stripMarkdown(floatingNote.preview) || "空便签"}</span>
        </div>
      )}
    </div>
  );
}

function NoteTagSection({
  tags,
  selectedTag,
  onSelectTag,
}: {
  tags: TagSummary[];
  selectedTag: string | null;
  onSelectTag: (tag: string | null) => void;
}) {
  return (
    <section className="clipboard-sidebar__section">
      <h3 className="clipboard-sidebar__title flex items-center gap-1">
        <Tags className="h-3 w-3" />
        标签
      </h3>
      <button
        type="button"
        onClick={() => onSelectTag(null)}
        className={`clipboard-sidebar__item ${!selectedTag ? "note-sidebar__item--active" : ""}`}
      >
        <span className="clipboard-sidebar__item-title">
          <span>全部</span>
        </span>
      </button>
      {tags.slice(0, 12).map((tag) => (
        <button
          key={tag.id}
          type="button"
          onClick={() => onSelectTag(selectedTag === tag.normalized_name ? null : tag.normalized_name)}
          className={`clipboard-sidebar__item ${selectedTag === tag.normalized_name ? "note-sidebar__item--active" : ""}`}
          title={`#${tag.name}`}
        >
          <span className="clipboard-sidebar__item-title">
            <span className="truncate">#{tag.name}</span>
            <span className="text-[10px] font-normal text-gray-400">{tag.note_count}</span>
          </span>
        </button>
      ))}
    </section>
  );
}

function NoteSidebarSection({
  title,
  group,
  children,
}: {
  title: string;
  group: "pinned" | "all";
  children: React.ReactNode;
}) {
  return (
    <section className="clipboard-sidebar__section" data-note-group={group}>
      <h3 className="clipboard-sidebar__title">{title}</h3>
      {children}
    </section>
  );
}

function NoteSidebarItem({
  note,
  active,
  dragging,
  dragReady,
  dropPlacement,
  onSelect,
  onDelete,
  onTogglePin,
  onSelectTag,
  onPointerDown,
  onPointerUp,
  onPointerLeave,
  onPointerMove,
}: {
  note: NoteSummary;
  active: boolean;
  dragging: boolean;
  dragReady: boolean;
  dropPlacement: DropPlacement | null;
  onSelect: (id: string) => void;
  onDelete: (id: string) => void;
  onTogglePin: (id: string) => void;
  onSelectTag: (tag: string | null) => void;
  onPointerDown: (id: string, pointerId: number) => void;
  onPointerUp: (targetId: string | null) => void;
  onPointerLeave: () => void;
  onPointerMove: (clientX: number, clientY: number) => void;
}) {
  return (
    <button
      type="button"
      onClick={() => {
        if (!dragReady && !dragging) onSelect(note.id);
      }}
      data-note-id={note.id}
      onPointerDown={(event) => {
        if (event.button !== 0) return;
        event.currentTarget.setPointerCapture(event.pointerId);
        onPointerDown(note.id, event.pointerId);
      }}
      onPointerUp={(event) => {
        const target = document
          .elementFromPoint(event.clientX, event.clientY)
          ?.closest<HTMLElement>("[data-note-id]");
        onPointerUp(target?.dataset.noteId ?? null);
      }}
      onPointerLeave={() => {
        if (!dragReady) onPointerLeave();
      }}
      onPointerMove={(event) => {
        onPointerMove(event.clientX, event.clientY);
      }}
      className={`note-sidebar__item ${active ? "note-sidebar__item--active" : ""} ${dragging ? "note-sidebar__item--dragging" : ""} ${dropPlacement ? `note-sidebar__item--drop-${dropPlacement}` : ""} ${dragReady ? "select-none" : ""}`}
      title={`${note.title || "无标题"}\n${stripMarkdown(note.preview) || "空便签"}`}
    >
      <span className="note-sidebar__item-title">
        <span className="min-w-0 truncate">{note.title || "无标题"}</span>
        <span className="note-sidebar__item-actions">
          <span
            role="button"
            tabIndex={0}
            className={`note-sidebar__icon-button ${note.is_pinned ? "text-amber-500" : "text-gray-400"}`}
            title={note.is_pinned ? "取消固定" : "固定"}
            aria-label={note.is_pinned ? "取消固定" : "固定"}
            onClick={(event) => {
              event.stopPropagation();
              onTogglePin(note.id);
            }}
            onPointerDown={(event) => event.stopPropagation()}
            onKeyDown={(event) => {
              if (event.key !== "Enter" && event.key !== " ") return;
              event.preventDefault();
              event.stopPropagation();
              onTogglePin(note.id);
            }}
          >
            {note.is_pinned ? <Pin className="h-3.5 w-3.5" /> : <PinOff className="h-3.5 w-3.5" />}
          </span>
          <span
            role="button"
            tabIndex={0}
            className="note-sidebar__icon-button text-gray-400 hover:bg-red-50 hover:text-red-500"
            title="删除"
            aria-label="删除便签"
            onClick={(event) => {
              event.stopPropagation();
              onDelete(note.id);
            }}
            onPointerDown={(event) => event.stopPropagation()}
            onKeyDown={(event) => {
              if (event.key !== "Enter" && event.key !== " ") return;
              event.preventDefault();
              event.stopPropagation();
              onDelete(note.id);
            }}
          >
            <Trash2 className="h-3.5 w-3.5" />
          </span>
        </span>
      </span>
      <span className="note-sidebar__item-text">{stripMarkdown(note.preview) || "空便签"}</span>
      {note.tags.length > 0 && (
        <span className="mt-2 flex flex-wrap gap-1">
          {note.tags.slice(0, 3).map((tag) => (
            <span
              key={tag}
              role="button"
              tabIndex={0}
              className="rounded bg-blue-50 px-1.5 py-0.5 text-[10px] font-medium text-blue-700"
              onClick={(event) => {
                event.stopPropagation();
                onSelectTag(tag.toLowerCase());
              }}
              onPointerDown={(event) => event.stopPropagation()}
              onKeyDown={(event) => {
                if (event.key !== "Enter" && event.key !== " ") return;
                event.preventDefault();
                event.stopPropagation();
                onSelectTag(tag.toLowerCase());
              }}
            >
              #{tag}
            </span>
          ))}
        </span>
      )}
      <span className="mt-2 block truncate text-[10px] text-gray-400">{formatRelativeTime(note.updated_at)}</span>
    </button>
  );
}

function ClipboardSidebar({
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

function ClipboardSidebarItem({
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

function formatSyncStatus(status: SidebarSyncStatus): string {
  if (status === "disabled") return "同步未启用";
  if (status === "syncing") return "正在同步";
  if (status === "synced") return "同步完成";
  if (status === "error") return "同步失败";
  return "立即同步";
}

function getMenuPosition(x: number, y: number): { x: number; y: number } {
  const margin = 8;
  const menuWidth = 190;
  const menuHeight = 56;
  return {
    x: Math.min(x, window.innerWidth - menuWidth - margin),
    y: Math.min(y, window.innerHeight - menuHeight - margin),
  };
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

function clearLongPressTimer(ref: React.MutableRefObject<number | null>): void {
  if (ref.current) {
    window.clearTimeout(ref.current);
    ref.current = null;
  }
}
