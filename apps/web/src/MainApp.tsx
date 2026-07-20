import React, { Suspense, useEffect, useRef, useState } from "react";
import { MobileBottomNav, MobileNoteHeader } from "@/components/MobileNavigation";
import { AppToasts } from "@/components/AppToasts";
import { Sidebar } from "@/components/Sidebar";
import { ClipboardPanel } from "@ui/components/ClipboardPanel";
import { EmptyState } from "@ui/components/EmptyState";
import { EditorSkeleton } from "@ui/components/EditorSkeleton";
import { TrashPanel } from "@ui/components/TrashPanel";
import { HistoryPanel } from "@ui/components/HistoryPanel";
import { clipboardItemToNoteContent } from "@ui/utils/clipboard";
import { useNotes } from "@/hooks/useNotes";
import { useClipboard } from "@/hooks/useClipboard";
import { useCloudEvents } from "@/hooks/useCloudEvents";
import { attachmentsApi } from "@/api/client";
import type { AppView } from "@/types";

const NoteEditor = React.lazy(() => import("@/components/NoteEditor").then((m) => ({ default: m.NoteEditor })));

export function MainApp({ userEmail, onLogout }: { userEmail: string; onLogout: () => void }) {
  const {
    notes, tags, selectedTag, setSelectedTag, activeNote, deletedNotes, versions,
    saveStatus, errorMessage, searchQuery, setSearchQuery, createNote, selectNote,
    updateNote, deleteNote, restoreNote, undoDelete, purgeNote, purgeAllNotes,
    togglePin, updateNoteTags, reorderNotes, loadNotes, loadDeletedNotes, loadVersions,
    restoreVersion, toggleVersionPin, deleteVersion, clearVersions, loadMoreNotes,
    hasMoreNotes, isLoadingMoreNotes,
  } = useNotes();

  const clipboard = useClipboard();
  const [viewMode, setViewMode] = useState<AppView>("notes");
  const [showTrash, setShowTrash] = useState(false);
  const [showHistory, setShowHistory] = useState(false);
  const [focusedClipboardItemId, setFocusedClipboardItemId] = useState<string | null>(null);
  const [deletedToast, setDeletedToast] = useState<{ title: string } | null>(null);
  const [isRefreshing, setIsRefreshing] = useState(false);
  const refreshTaskRef = useRef<Promise<void> | null>(null);

  const resolveClipboardAttachment = React.useCallback(async (id: string) => {
    const blob = await attachmentsApi.download(id);
    return URL.createObjectURL(blob);
  }, []);

  const refreshCloudData = React.useCallback(async ({ showIndicator = false }: { showIndicator?: boolean } = {}) => {
    if (refreshTaskRef.current) return refreshTaskRef.current;
    if (showIndicator) setIsRefreshing(true);

    const task = Promise.all([loadNotes(), clipboard.loadItems()])
      .then(() => undefined)
      .finally(() => {
        refreshTaskRef.current = null;
        if (showIndicator) setIsRefreshing(false);
      });

    refreshTaskRef.current = task;
    return task;
  }, [clipboard, loadNotes]);

  useCloudEvents(() => {
    void refreshCloudData({ showIndicator: false });
  });

  useEffect(() => {
    if (!deletedToast) return;
    const timer = window.setTimeout(() => setDeletedToast(null), 6_000);
    return () => window.clearTimeout(timer);
  }, [deletedToast]);

  const handleDeleteNote = async (id: string) => {
    const note = notes.find((item) => item.id === id);
    const deleted = await deleteNote(id);
    if (deleted) setDeletedToast({ title: note?.title || "无标题" });
  };

  const handleUndoDelete = async () => {
    setDeletedToast(null);
    await undoDelete();
  };

  const openOnlyPanel = (panel: "trash" | "history") => {
    setShowTrash(panel === "trash");
    setShowHistory(panel === "history");
  };

  const changeViewMode = (mode: AppView) => {
    setViewMode(mode);
    setShowTrash(false);
    setShowHistory(false);
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
    await createNote(clipboardItemToNoteContent(item));
  };

  useEffect(() => {
    if (!activeNote?.id || !showHistory) return;
    void loadVersions(activeNote.id);
  }, [activeNote?.id, loadVersions, showHistory]);

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Ctrl+N: New note
      if ((e.ctrlKey || e.metaKey) && e.key === "n") {
        e.preventDefault();
        setViewMode("notes");
        void createNote();
      }
      // Ctrl+F: Focus search
      if ((e.ctrlKey || e.metaKey) && e.key === "f") {
        e.preventDefault();
        const searchInput = document.querySelector<HTMLInputElement>(
          viewMode === "notes"
            ? 'input[placeholder="搜索便签..."]'
            : 'input[placeholder="搜索剪贴板历史"]',
        );
        searchInput?.focus();
      }
      // Ctrl+Shift+Z: Undo delete
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "z" && e.shiftKey) {
        e.preventDefault();
        void undoDelete();
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [createNote, undoDelete, viewMode]);

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
        onReorderNotes={(ids, isPinned) => void reorderNotes(ids, isPinned)}
        onLoadMoreNotes={() => void loadMoreNotes()}
        hasMoreNotes={hasMoreNotes}
        isLoadingMoreNotes={isLoadingMoreNotes}
        onOpenTrash={async () => {
          if (showTrash) {
            setShowTrash(false);
            return;
          }
          await loadDeletedNotes();
          openOnlyPanel("trash");
        }}
        isTrashOpen={showTrash}
        onSelectClipboardItem={(id) => {
          setFocusedClipboardItemId(id);
          setViewMode("clipboard");
        }}
        onCreateNoteFromClipboard={(id) => void handleCreateNoteFromClipboard(id)}
        userEmail={userEmail}
        onLogout={onLogout}
      />

      {/* Main Content */}
      <div className="flex min-w-0 flex-1 flex-col bg-white pb-14 md:pb-0">
        {viewMode === "clipboard" ? (
          <ClipboardPanel
            items={clipboard.items}
            query={clipboard.query}
            copiedId={clipboard.copiedId}
            error={clipboard.error}
            onQueryChange={clipboard.setQuery}
            onCapture={clipboard.capture}
            onCopy={clipboard.copyItem}
            onTogglePin={(id) => void clipboard.togglePin(id)}
            onDelete={clipboard.deleteItem}
            focusedItemId={focusedClipboardItemId}
            onCreateNoteFromItem={(id) => void handleCreateNoteFromClipboard(id)}
            resolveAttachmentSrc={resolveClipboardAttachment}
          />
        ) : (
          <>
            <MobileNoteHeader
              activeNote={activeNote} notes={notes} searchQuery={searchQuery}
              onSelect={(id) => void selectNote(id)} onCreate={() => void createNote()}
              onSearch={setSearchQuery}
            />

            <Suspense fallback={<EditorSkeleton showStatusBar />}>
              {activeNote ? (
                <NoteEditor
                  key={activeNote.id}
                  note={activeNote}
                  onUpdate={updateNote}
                  saveStatus={saveStatus}
                  errorMessage={errorMessage}
                  onOpenHistory={handleOpenHistory}
                  onUpdateTags={updateNoteTags}
                  tags={tags}
                />
              ) : (
                <EmptyState
                  onCreateNote={createNote}
                  hints={
                    <>
                      <p><kbd className="px-1.5 py-0.5 bg-gray-100 rounded text-gray-500 font-mono text-xs">Ctrl+N</kbd> 新建便签</p>
                      <p><kbd className="px-1.5 py-0.5 bg-gray-100 rounded text-gray-500 font-mono text-xs">Ctrl+F</kbd> 搜索便签</p>
                    </>
                  }
                />
              )}
            </Suspense>
          </>
        )}
      </div>

      <MobileBottomNav
        viewMode={viewMode} refreshing={isRefreshing} trashOpen={showTrash} userEmail={userEmail}
        onChangeView={changeViewMode} onRefresh={() => void refreshCloudData({ showIndicator: true })}
        onTrash={() => {
          if (showTrash) return setShowTrash(false);
          void loadDeletedNotes().then(() => openOnlyPanel("trash"));
        }}
        onLogout={onLogout}
      />

      <AppToasts
        deletedTitle={deletedToast?.title ?? null} error={errorMessage}
        onUndo={() => void handleUndoDelete()}
      />

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
          onRestore={(versionId) => void restoreVersion(activeNote.id, versionId)}
          onTogglePin={(versionId) => void toggleVersionPin(activeNote.id, versionId)}
          onDelete={(versionId) => void deleteVersion(activeNote.id, versionId)}
          onClear={() => void clearVersions(activeNote.id)}
        />
      )}

    </div>
  );
}
