import { FileText, LogOut, Plus, RefreshCw, Search, Settings, StickyNote, Trash2, X } from "lucide-react";
import type { SidebarProps, SidebarSyncStatus } from "../Sidebar";
import type { useSidebarModel } from "./useSidebarModel";
import { NoteSidebarItem, NoteSidebarSection, NoteTagSection } from "./NoteSidebar";
import { ClipboardSidebar } from "./ClipboardSidebar";
import { stripMarkdown } from "../../utils/html";

export function SidebarView({ props, model }: { props: SidebarProps; model: ReturnType<typeof useSidebarModel> }) {
  const {
  viewMode, onViewModeChange, clipboardCount, notes,
  tags,
  selectedTag,
  activeNoteId,
  searchQuery,
  onSearchChange,
  onSelectTag,
  onCreateNote,
  onDeleteNote,
  onTogglePin,
  onLoadMoreNotes,
  hasMoreNotes,
  isLoadingMoreNotes,
  onOpenTrash,
  isTrashOpen,
  onSelectClipboardItem,
  onCreateNoteFromClipboard,
  syncStatus,
  syncLabel,
  syncPendingCount = 0,
  onSync,
  onOpenSettings,
  settingsLabel = "设置",
  userEmail,
  onLogout,
  } = props;
  const {
    clipboardContextMenu, closeMenus, dragPosition, dragReadyId, draggingId,
    dragTarget, finishPointerDrag, floatingNote, pinnedClipboardItems, pinnedNotes,
    recentClipboardItems, resetLongPress, selectNote, setClipboardContextMenu,
    startLongPress, unpinnedNotes, updateDragTarget,
  } = model;
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
        <div
          className="note-sidebar"
          onScroll={(event) => {
            const element = event.currentTarget;
            if (
              hasMoreNotes && !isLoadingMoreNotes &&
              element.scrollHeight - element.scrollTop - element.clientHeight < 160
            ) onLoadMoreNotes?.();
          }}
        >
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
              {isLoadingMoreNotes && (
                <p className="px-4 py-3 text-center text-xs text-gray-400">正在加载更多…</p>
              )}
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
            title={syncLabel ?? formatSyncStatus(syncStatus)}
            aria-label={syncLabel ?? formatSyncStatus(syncStatus)}
          >
            <span className="relative">
              <RefreshCw className={`h-4 w-4 ${syncStatus === "syncing" ? "animate-spin" : ""}`} />
              {syncPendingCount > 0 && syncStatus !== "syncing" && (
                <span className="absolute -right-2 -top-2 min-w-3 rounded-full bg-blue-600 px-0.5 text-[8px] leading-3 text-white">
                  {syncPendingCount > 99 ? "99+" : syncPendingCount}
                </span>
              )}
            </span>
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

function formatSyncStatus(status: SidebarSyncStatus): string {
  if (status === "syncing") return "正在同步";
  if (status === "waiting") return "等待同步";
  if (status === "retrying") return "网络不稳定，等待自动重试";
  if (status === "synced") return "同步完成";
  if (status === "error") return "同步失败";
  if (status === "disabled") return "同步未启用";
  return "立即同步";
}

function getMenuPosition(x: number, y: number): { x: number; y: number } {
  const width = 190;
  const height = 96;
  return { x: Math.max(8, Math.min(x, window.innerWidth - width - 8)), y: Math.max(8, Math.min(y, window.innerHeight - height - 8)) };
}
