import React, { Suspense, useEffect, useState } from "react";
import { LoginPage } from "@/components/LoginPage";
import { Sidebar } from "@/components/Sidebar";
import { ClipboardPanel } from "@ui/components/ClipboardPanel";
import { EmptyState } from "@ui/components/EmptyState";
import { EditorSkeleton } from "@ui/components/EditorSkeleton";
import { useAuth } from "@/hooks/useAuth";
import { useNotes } from "@/hooks/useNotes";
import { useClipboard } from "@/hooks/useClipboard";
import { useCloudEvents } from "@/hooks/useCloudEvents";
import { Search, X } from "lucide-react";
import type { AppView } from "@/types";

const NoteEditor = React.lazy(() =>
  import("@/components/NoteEditor").then((m) => ({ default: m.NoteEditor })),
);

export default function App() {
  const auth = useAuth();

  if (!auth.user) {
    return (
      <LoginPage
        onLogin={auth.login}
        onRegister={auth.register}
        error={auth.error}
        loading={auth.loading}
      />
    );
  }

  return <MainApp userEmail={auth.user.email} onLogout={auth.logout} />;
}

function MainApp({ userEmail, onLogout }: { userEmail: string; onLogout: () => void }) {
  const {
    notes,
    activeNote,
    saveStatus,
    errorMessage,
    searchQuery,
    setSearchQuery,
    createNote,
    selectNote,
    updateNote,
    deleteNote,
    restoreNote,
    togglePin,
    loadNotes,
  } = useNotes();

  const clipboard = useClipboard();
  const [viewMode, setViewMode] = useState<AppView>("notes");
  const [deletedToast, setDeletedToast] = useState<{
    id: string;
    title: string;
  } | null>(null);
  useCloudEvents(() => {
    void loadNotes();
    void clipboard.loadItems();
  });

  useEffect(() => {
    if (!deletedToast) return;
    const timer = window.setTimeout(() => setDeletedToast(null), 6_000);
    return () => window.clearTimeout(timer);
  }, [deletedToast]);

  const handleDeleteNote = async (id: string) => {
    const note = notes.find((item) => item.id === id);
    const deleted = await deleteNote(id);
    if (deleted) setDeletedToast({ id, title: note?.title || "无标题" });
  };

  const handleUndoDelete = async () => {
    if (!deletedToast) return;
    const { id } = deletedToast;
    setDeletedToast(null);
    if (await restoreNote(id)) await selectNote(id);
  };

  return (
    <div className="flex h-screen overflow-hidden">
      {/* Sidebar */}
      <Sidebar
        viewMode={viewMode}
        onViewModeChange={setViewMode}
        clipboardCount={clipboard.items.length}
        notes={notes}
        activeNoteId={activeNote?.id ?? null}
        searchQuery={searchQuery}
        onSearchChange={setSearchQuery}
        onSelectNote={selectNote}
        onCreateNote={createNote}
        onDeleteNote={(id) => void handleDeleteNote(id)}
        onTogglePin={togglePin}
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
            onDelete={clipboard.deleteItem}
          />
        ) : (
          <>
            {/* Mobile header */}
            <div className="space-y-2 border-b border-gray-200 bg-white px-3 py-2 md:hidden">
              <div className="flex items-center gap-2">
                <select
                  value={activeNote?.id ?? ""}
                  onChange={(event) =>
                    event.target.value && void selectNote(event.target.value)
                  }
                  className="focus-ring min-w-0 flex-1 rounded-lg border border-gray-200 bg-gray-50 px-3 py-2 text-sm"
                  aria-label="选择便签"
                >
                  <option value="">选择便签</option>
                  {notes.map((note) => (
                    <option key={note.id} value={note.id}>
                      {note.title || "无标题"}
                    </option>
                  ))}
                </select>
                <button
                  type="button"
                  onClick={() => void createNote()}
                  className="focus-ring rounded-lg bg-blue-600 px-3 py-2 text-sm font-medium text-white"
                >
                  新建
                </button>
              </div>
              <label className="relative block">
                <span className="sr-only">搜索便签</span>
                <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-gray-400" />
                <input
                  type="search"
                  value={searchQuery}
                  onChange={(event) => setSearchQuery(event.target.value)}
                  placeholder="搜索便签..."
                  className="focus-ring w-full rounded-lg border border-gray-200 bg-gray-50 py-2 pl-9 pr-9 text-sm placeholder-gray-400"
                />
                {searchQuery && (
                  <button
                    type="button"
                    onClick={() => setSearchQuery("")}
                    className="focus-ring absolute right-2 top-1/2 flex h-6 w-6 -translate-y-1/2 items-center justify-center rounded-full text-gray-400 hover:bg-gray-200 hover:text-gray-600"
                    aria-label="清空搜索"
                  >
                    <X className="h-3.5 w-3.5" />
                  </button>
                )}
              </label>
            </div>

            <Suspense fallback={<EditorSkeleton />}>
              {activeNote ? (
                <NoteEditor
                  note={activeNote}
                  onUpdate={updateNote}
                  saveStatus={saveStatus}
                  errorMessage={errorMessage}
                />
              ) : (
                <EmptyState onCreateNote={createNote} />
              )}
            </Suspense>
          </>
        )}
      </div>

      {/* Mobile bottom nav */}
      <nav className="fixed inset-x-0 bottom-0 z-30 grid h-14 grid-cols-2 border-t border-gray-200 bg-white/95 px-2 backdrop-blur md:hidden">
        <button
          type="button"
          onClick={() => setViewMode("notes")}
          className={`focus-ring rounded-lg text-sm font-medium ${viewMode === "notes" ? "text-blue-600" : "text-gray-500"}`}
          aria-pressed={viewMode === "notes"}
        >
          便签
        </button>
        <button
          type="button"
          onClick={() => setViewMode("clipboard")}
          className={`focus-ring rounded-lg text-sm font-medium ${viewMode === "clipboard" ? "text-violet-600" : "text-gray-500"}`}
          aria-pressed={viewMode === "clipboard"}
        >
          剪贴板
        </button>
      </nav>

      {deletedToast && (
        <div
          role="status"
          className="fixed right-4 bottom-20 z-40 flex max-w-sm items-center gap-3 rounded-xl border border-gray-200 bg-white px-4 py-3 text-sm text-gray-700 shadow-lg md:bottom-4"
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
        <div className="fixed right-4 bottom-4 max-w-sm rounded border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700 shadow">
          {errorMessage}
        </div>
      )}
    </div>
  );
}
