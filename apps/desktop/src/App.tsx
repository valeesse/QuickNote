import React, { Suspense, useCallback, useEffect, useState } from "react";
import { Sidebar } from "@/components/Sidebar";
import { useNotes } from "@/hooks/useNotes";
import { useSync } from "@/hooks/useSync";
import { useClipboard } from "@/hooks/useClipboard";
import { invoke } from "@/utils/tauri";
import { ClipboardPanel } from "@ui/components/ClipboardPanel";
import { EmptyState } from "@ui/components/EmptyState";
import { EditorSkeleton } from "@ui/components/EditorSkeleton";
import { TrashPanel } from "@ui/components/TrashPanel";
import { HistoryPanel } from "@ui/components/HistoryPanel";
import { stripHtml } from "@ui/utils/html";
import { clipboardItemToNoteContent } from "@ui/utils/clipboard";
import { SyncSettingsPanel } from "@/components/settings/SyncSettingsPanel";
import { useAppKeyboardShortcuts } from "@/hooks/useAppKeyboardShortcuts";
import type { AppView, ShortcutConfig, ShortcutConfigInput } from "@/types";

const NoteEditor = React.lazy(() => import("@/components/NoteEditor").then((m) => ({ default: m.NoteEditor })));

export default function App() {
  const {
    notes, tags, selectedTag, setSelectedTag, activeNote, deletedNotes, versions,
    saveStatus, errorMessage, searchQuery, setSearchQuery, createNote, selectNote,
    updateNote, deleteNote, togglePin, updateNoteTags, reorderNotes, loadNotes,
    loadDeletedNotes, restoreNote, undoDelete, purgeNote, purgeAllNotes, loadVersions,
    restoreVersion, toggleVersionPin, deleteVersion, clearVersions, saveAttachment,
    resolveAttachment, flushAllDrafts, refreshAfterSync,
  } = useNotes();
  const clipboard = useClipboard();
  const resolveClipboardAttachment = useCallback(async (id: string) =>
    (await invoke<{ id: string; data_url: string }>("get_attachment_data_url", { id })).data_url, []);
  const [viewMode, setViewMode] = useState<AppView>("notes");
  const [showTrash, setShowTrash] = useState(false);
  const [showHistory, setShowHistory] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [focusedClipboardItemId, setFocusedClipboardItemId] = useState<string | null>(null);
  const [shortcutConfig, setShortcutConfig] = useState<ShortcutConfig | null>(null);
  const [shortcutError, setShortcutError] = useState<string | null>(null);
  const [deletedToast, setDeletedToast] = useState<{ title: string } | null>(null);
  const sync = useSync({ beforeSync: flushAllDrafts, onSynced: async () => {
    await Promise.all([refreshAfterSync(), clipboard.loadItems()]);
  } });

  useEffect(() => {
    if (!deletedToast) return;
    const timer = window.setTimeout(() => setDeletedToast(null), 6_000);
    return () => window.clearTimeout(timer);
  }, [deletedToast]);

  useEffect(() => {
    let cancelled = false;
    invoke<ShortcutConfig>("get_shortcut_config")
      .then((config) => {
        if (!cancelled) {
          setShortcutConfig(config);
          setShortcutError(null);
        }
      })
      .catch((error) => {
        if (!cancelled) setShortcutError(String(error));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const saveShortcuts = async (input: ShortcutConfigInput) => {
    setShortcutError(null);
    try {
      const next = await invoke<ShortcutConfig>("set_shortcut_config", { config: input });
      setShortcutConfig(next);
      return next;
    } catch (error) {
      const message = String(error);
      setShortcutError(message);
      throw new Error(message);
    }
  };

  const handleDeleteNote = async (id: string) => {
    const note = notes.find((item) => item.id === id);
    const deleted = await deleteNote(id);
    if (deleted) setDeletedToast({ title: note?.title || "无标题" });
  };

  const handleUndoDelete = async () => {
    setDeletedToast(null);
    await undoDelete();
  };

  const openOnlyPanel = (panel: "trash" | "history" | "settings") => {
    setShowTrash(panel === "trash");
    setShowHistory(panel === "history");
    setShowSettings(panel === "settings");
  };

  const changeViewMode = (mode: AppView) => {
    setViewMode(mode);
    setShowTrash(false);
    setShowHistory(false);
    setShowSettings(false);
  };

  const handleOpenHistory = async () => {
    if (!activeNote) return;
    if (showHistory) {
      setShowHistory(false);
      return;
    }
    await loadVersions(activeNote.id);
    openOnlyPanel("history");
  };

  const handleCreateNoteFromClipboard = async (id: string) => {
    const item = clipboard.items.find((entry) => entry.id === id);
    if (!item) return;
    setViewMode("notes");
    setShowTrash(false);
    setShowHistory(false);
    setShowSettings(false);
    await createNote(clipboardItemToNoteContent(item));
  };

  useEffect(() => {
    if (!activeNote?.id || !showHistory) return;
    void loadVersions(activeNote.id);
  }, [activeNote?.id, loadVersions, showHistory]);

  useAppKeyboardShortcuts({ viewMode, onCreate: createNote, onUndoDelete: undoDelete, onShowNotes: () => setViewMode("notes") });

  return (
    <div className="flex h-screen overflow-hidden">
      {/* Sidebar */}
      <Sidebar
        viewMode={viewMode}
        onViewModeChange={changeViewMode}
        clipboardCount={clipboard.items.length}
        clipboardItems={clipboard.items}
        notes={notes}
        tags={tags}
        selectedTag={selectedTag}
        activeNoteId={activeNote?.id ?? null}
        searchQuery={searchQuery}
        onSearchChange={setSearchQuery}
        onSelectTag={setSelectedTag}
        onSelectNote={selectNote}
        onCreateNote={createNote}
        onDeleteNote={(id) => void handleDeleteNote(id)}
        onTogglePin={togglePin}
        onReorderNotes={async (ids, isPinned) => { await reorderNotes(ids, isPinned); void sync.syncNow(); }}
        onOpenTrash={async () => {
          if (showTrash) {
            setShowTrash(false);
            return;
          }
          await loadDeletedNotes();
          openOnlyPanel("trash");
        }}
        isTrashOpen={showTrash}
        syncStatus={sync.status}
        onSync={() => void sync.syncNow()}
        onOpenSettings={() => openOnlyPanel("settings")}
        onSelectClipboardItem={(id) => {
          setFocusedClipboardItemId(id);
          setViewMode("clipboard");
        }}
        onCreateNoteFromClipboard={(id) => void handleCreateNoteFromClipboard(id)}
      />

      {/* Main Content */}
      <div className="flex min-w-0 flex-1 flex-col bg-white pb-14 md:pb-0">
        {viewMode === "clipboard" ? (
          <ClipboardPanel
            items={clipboard.items}
            query={clipboard.query}
            autoCaptureSupported={clipboard.autoCaptureSupported}
            autoCaptureEnabled={clipboard.autoCaptureEnabled}
            copiedId={clipboard.copiedId}
            error={clipboard.error}
            onQueryChange={clipboard.setQuery}
            onCapture={clipboard.capture}
            onAutoCaptureChange={clipboard.setAutoCaptureEnabled}
            onCopy={(id) => void clipboard.copyItem(id)}
            onTogglePin={(id) => void clipboard.togglePin(id)}
            onDelete={(id) => void clipboard.deleteItem(id)}
            focusedItemId={focusedClipboardItemId}
            onCreateNoteFromItem={(id) => void handleCreateNoteFromClipboard(id)}
            resolveAttachmentSrc={resolveClipboardAttachment}
          />
        ) : <>
          <div className="flex items-center gap-2 border-b border-gray-200 bg-white px-3 py-2 md:hidden">
            <select
              value={activeNote?.id ?? ""}
              onChange={(event) => event.target.value && void selectNote(event.target.value)}
              className="min-w-0 flex-1 rounded-lg border border-gray-200 bg-gray-50 px-3 py-2 text-sm"
            >
              <option value="">选择便签</option>
              {notes.map((note) => <option key={note.id} value={note.id}>{note.title}</option>)}
            </select>
            <button type="button" onClick={() => void createNote()} className="rounded-lg bg-blue-600 px-3 py-2 text-sm font-medium text-white">新建</button>
          </div>
          <Suspense fallback={<EditorSkeleton showStatusBar />}>
          {activeNote ? (
                <NoteEditor
                  key={activeNote.id}
                  note={activeNote}
              onUpdate={updateNote}
              onSaveAttachment={saveAttachment}
              onResolveAttachment={resolveAttachment}
              onOpenHistory={handleOpenHistory}
              onUpdateTags={updateNoteTags}
              tags={tags}
              saveStatus={saveStatus}
              errorMessage={errorMessage}
              isSyncing={sync.status === "syncing"}
            />
          ) : (
            <EmptyState
              onCreateNote={createNote}
              hints={
                <>
                  <p><kbd className="px-1.5 py-0.5 bg-gray-100 rounded text-gray-500 font-mono text-xs">Ctrl+N</kbd> 新建便签</p>
                  <p><kbd className="px-1.5 py-0.5 bg-gray-100 rounded text-gray-500 font-mono text-xs">Ctrl+F</kbd> 搜索便签</p>
                  <p><kbd className="px-1.5 py-0.5 bg-gray-100 rounded text-gray-500 font-mono text-xs">Ctrl+Alt+N</kbd> 全局呼出</p>
                </>
              }
            />
          )}
          </Suspense>
        </>}
      </div>

      <nav className="fixed inset-x-0 bottom-0 z-30 grid h-14 grid-cols-4 border-t border-gray-200 bg-white/95 px-2 backdrop-blur md:hidden">
        <button type="button" onClick={() => changeViewMode("notes")} aria-pressed={viewMode === "notes"} className={`focus-ring rounded-lg text-sm font-medium ${viewMode === "notes" ? "text-blue-600" : "text-gray-500"}`}>便签</button>
        <button type="button" onClick={() => changeViewMode("clipboard")} aria-pressed={viewMode === "clipboard"} className={`focus-ring rounded-lg text-sm font-medium ${viewMode === "clipboard" ? "text-violet-600" : "text-gray-500"}`}>剪贴板</button>
        <button type="button" onClick={() => void sync.syncNow()} className="focus-ring rounded-lg text-sm font-medium text-gray-500">同步</button>
        <button type="button" onClick={() => openOnlyPanel("settings")} className="focus-ring rounded-lg text-sm font-medium text-gray-500">设置</button>
      </nav>

      {deletedToast && (
        <div
          role="status"
          className="animate-toast-in fixed right-4 bottom-20 z-40 flex max-w-sm items-center gap-3 rounded-xl border border-gray-200 bg-white px-4 py-3 text-sm text-gray-700 shadow-lg md:bottom-4"
        >
          <span className="min-w-0 flex-1 truncate">已删除「{deletedToast.title}」</span>
          <button
            type="button"
            onClick={() => void handleUndoDelete()}
            className="focus-ring rounded px-2 py-1 font-medium text-blue-600 hover:bg-blue-50"
          >
            撤销
          </button>
        </div>
      )}

      {errorMessage && (
        <div className="animate-toast-in fixed right-4 bottom-4 max-w-sm rounded border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700 shadow">
          {errorMessage}
        </div>
      )}

      {showTrash && (
        <TrashPanel
          notes={deletedNotes}
          onClose={() => setShowTrash(false)}
          onRestore={restoreNote}
          onPurge={purgeNote}
          onPurgeAll={purgeAllNotes}
        />
      )}

      {showHistory && activeNote && (
        <HistoryPanel
          versions={versions}
          onClose={() => setShowHistory(false)}
          onRestore={(versionId) => restoreVersion(activeNote.id, versionId)}
          onTogglePin={(versionId) => toggleVersionPin(activeNote.id, versionId)}
          onDelete={(versionId) => deleteVersion(activeNote.id, versionId)}
          onClear={() => clearVersions(activeNote.id)}
        />
      )}

      {showSettings && (
        <SyncSettingsPanel
          config={sync.config}
          status={sync.status}
          error={sync.error}
          shortcutConfig={shortcutConfig}
          shortcutError={shortcutError}
          onClose={() => setShowSettings(false)}
          onSave={sync.saveConfig}
          onSync={sync.syncNow}
          onTestWebdav={sync.testWebdav}
          onTestCloud={sync.testCloud}
          onSaveShortcuts={saveShortcuts}
        />
      )}
    </div>
  );
}
