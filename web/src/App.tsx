import React, { Suspense, useState } from "react";
import { LoginPage } from "@/components/LoginPage";
import { Sidebar } from "@/components/Sidebar";
import { ClipboardPanel } from "@/components/ClipboardPanel";
import { useAuth } from "@/hooks/useAuth";
import { useNotes } from "@/hooks/useNotes";
import { useClipboard } from "@/hooks/useClipboard";
import { FileEdit, Plus } from "lucide-react";
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
    togglePin,
  } = useNotes();

  const clipboard = useClipboard();
  const [viewMode, setViewMode] = useState<AppView>("notes");

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
        onDeleteNote={deleteNote}
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
            <div className="flex items-center gap-2 border-b border-gray-200 bg-white px-3 py-2 md:hidden">
              <select
                value={activeNote?.id ?? ""}
                onChange={(event) =>
                  event.target.value && void selectNote(event.target.value)
                }
                className="min-w-0 flex-1 rounded-lg border border-gray-200 bg-gray-50 px-3 py-2 text-sm"
              >
                <option value="">选择便签</option>
                {notes.map((note) => (
                  <option key={note.id} value={note.id}>
                    {note.title}
                  </option>
                ))}
              </select>
              <button
                onClick={() => void createNote()}
                className="rounded-lg bg-blue-600 px-3 py-2 text-sm font-medium text-white"
              >
                新建
              </button>
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
          onClick={() => setViewMode("notes")}
          className={viewMode === "notes" ? "text-blue-600" : "text-gray-500"}
        >
          便签
        </button>
        <button
          onClick={() => setViewMode("clipboard")}
          className={viewMode === "clipboard" ? "text-violet-600" : "text-gray-500"}
        >
          剪贴板
        </button>
      </nav>

      {errorMessage && (
        <div className="fixed right-4 bottom-4 max-w-sm rounded border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700 shadow">
          {errorMessage}
        </div>
      )}
    </div>
  );
}

function EditorSkeleton() {
  return (
    <div className="flex h-full flex-col animate-pulse">
      <div className="flex items-center gap-2 border-b border-gray-100 px-8 py-2">
        {Array.from({ length: 12 }).map((_, i) => (
          <div key={i} className="h-7 w-7 rounded bg-gray-100" />
        ))}
      </div>
      <div className="flex-1 space-y-4 px-8 py-6">
        <div className="h-5 w-3/4 rounded bg-gray-100" />
        <div className="h-4 w-full rounded bg-gray-50" />
        <div className="h-4 w-5/6 rounded bg-gray-50" />
        <div className="h-4 w-2/3 rounded bg-gray-50" />
      </div>
    </div>
  );
}

function EmptyState({ onCreateNote }: { onCreateNote: () => void }) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center p-8 text-gray-400">
      <div className="mb-6 flex h-24 w-24 items-center justify-center rounded-2xl bg-gray-100">
        <FileEdit className="h-12 w-12 text-gray-300" />
      </div>
      <h2 className="mb-2 text-xl font-medium text-gray-500">选择或创建一个便签</h2>
      <p className="mb-6 max-w-xs text-center text-sm text-gray-400">
        从左侧列表选择一个便签开始编辑，或创建一个新的便签
      </p>
      <button
        onClick={onCreateNote}
        className="flex items-center gap-2 rounded-lg bg-blue-600 px-6 py-2.5 font-medium text-white shadow-sm transition-colors hover:bg-blue-700"
      >
        <Plus className="h-4 w-4" />
        新建便签
      </button>
    </div>
  );
}
