import React, { Suspense, useEffect, useRef, useState } from "react";
import { billingApi } from "@/api/client";
import { AccountPanel } from "@/components/AccountPanel";
import { LoginPage } from "@/components/LoginPage";
import { Sidebar } from "@/components/Sidebar";
import { ClipboardPanel } from "@ui/components/ClipboardPanel";
import { EmptyState } from "@ui/components/EmptyState";
import { EditorSkeleton } from "@ui/components/EditorSkeleton";
import { TrashPanel } from "@ui/components/TrashPanel";
import { HistoryPanel } from "@ui/components/HistoryPanel";
import { clipboardItemToNoteContent } from "@ui/utils/clipboard";
import { useAuth } from "@/hooks/useAuth";
import { useNotes } from "@/hooks/useNotes";
import { useClipboard } from "@/hooks/useClipboard";
import { useCloudEvents } from "@/hooks/useCloudEvents";
import { attachmentsApi } from "@/api/client";
import { LogOut, RefreshCw, Search, Settings, Trash2, X } from "lucide-react";
import type { AccountSummary, AppView } from "@/types";

const NoteEditor = React.lazy(() =>
  import("@/components/NoteEditor").then((m) => ({ default: m.NoteEditor })),
);

export default function App() {
  const auth = useAuth();

  if (auth.initializing) {
    return (
      <div className="flex min-h-screen items-center justify-center bg-gray-50 text-sm text-gray-500">
        正在恢复登录状态...
      </div>
    );
  }

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
    deletedNotes,
    versions,
    saveStatus,
    errorMessage,
    searchQuery,
    setSearchQuery,
    createNote,
    selectNote,
    updateNote,
    deleteNote,
    restoreNote,
    undoDelete,
    purgeNote,
    purgeAllNotes,
    togglePin,
    reorderNotes,
    loadNotes,
    loadDeletedNotes,
    loadVersions,
    restoreVersion,
    toggleVersionPin,
    deleteVersion,
    clearVersions,
  } = useNotes();

  const clipboard = useClipboard();
  const [viewMode, setViewMode] = useState<AppView>("notes");
  const [showTrash, setShowTrash] = useState(false);
  const [showHistory, setShowHistory] = useState(false);
  const [focusedClipboardItemId, setFocusedClipboardItemId] = useState<string | null>(null);
  const [deletedToast, setDeletedToast] = useState<{
    title: string;
  } | null>(null);
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [showAccount, setShowAccount] = useState(false);
  const [accountSummary, setAccountSummary] = useState<AccountSummary | null>(null);
  const [accountLoading, setAccountLoading] = useState(false);
  const [accountError, setAccountError] = useState<string | null>(null);
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

  const loadAccountSummary = React.useCallback(async () => {
    try {
      setAccountLoading(true);
      setAccountError(null);
      const summary = await billingApi.summary();
      setAccountSummary(summary);
      return summary;
    } catch (error) {
      setAccountError(error instanceof Error ? error.message : String(error));
      return null;
    } finally {
      setAccountLoading(false);
    }
  }, []);

  const pollAccountSummaryAfterCheckout = React.useCallback(async () => {
    for (let attempt = 0; attempt < 10; attempt += 1) {
      const summary = await loadAccountSummary();
      if (summary?.subscription?.plan_id === "pro") return;
      await new Promise((resolve) => window.setTimeout(resolve, 2_000));
    }
  }, [loadAccountSummary]);

  useCloudEvents(() => {
    void refreshCloudData({ showIndicator: false });
  });

  useEffect(() => {
    void loadAccountSummary();
  }, [loadAccountSummary]);

  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    if (params.get("checkout") !== "success") return;
    params.delete("checkout");
    const next = params.toString();
    window.history.replaceState({}, "", `${window.location.pathname}${next ? `?${next}` : ""}`);
    setShowAccount(true);
    void pollAccountSummaryAfterCheckout();
  }, [pollAccountSummaryAfterCheckout]);

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
        activeNoteId={activeNote?.id ?? null}
        searchQuery={searchQuery}
        onSearchChange={setSearchQuery}
        onSelectNote={selectNote}
        onCreateNote={createNote}
        onDeleteNote={(id) => void handleDeleteNote(id)}
        onTogglePin={togglePin}
        onReorderNotes={(ids, isPinned) => void reorderNotes(ids, isPinned)}
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
        onOpenSettings={() => {
          setShowAccount(true);
          if (!accountSummary) void loadAccountSummary();
        }}
        settingsLabel="账户与订阅"
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

            <Suspense fallback={<EditorSkeleton showStatusBar />}>
              {activeNote ? (
                <NoteEditor
                  note={activeNote}
                  onUpdate={updateNote}
                  saveStatus={saveStatus}
                  errorMessage={errorMessage}
                  onOpenHistory={handleOpenHistory}
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

      {/* Mobile bottom nav */}
      <nav className="fixed inset-x-0 bottom-0 z-30 grid h-14 grid-cols-6 border-t border-gray-200 bg-white/95 px-2 backdrop-blur md:hidden">
        <button
          type="button"
          onClick={() => changeViewMode("notes")}
          className={`focus-ring rounded-lg text-sm font-medium ${viewMode === "notes" ? "text-blue-600" : "text-gray-500"}`}
          aria-pressed={viewMode === "notes"}
        >
          便签
        </button>
        <button
          type="button"
          onClick={() => changeViewMode("clipboard")}
          className={`focus-ring rounded-lg text-sm font-medium ${viewMode === "clipboard" ? "text-violet-600" : "text-gray-500"}`}
          aria-pressed={viewMode === "clipboard"}
        >
          剪贴板
        </button>
        <button
          type="button"
          onClick={() => void refreshCloudData({ showIndicator: true })}
          disabled={isRefreshing}
          className={`focus-ring flex items-center justify-center rounded-lg text-sm font-medium ${isRefreshing ? "text-blue-600" : "text-gray-500"}`}
          aria-label={isRefreshing ? "正在刷新云端数据" : "刷新云端数据"}
        >
          <RefreshCw className={`h-4 w-4 ${isRefreshing ? "animate-spin" : ""}`} />
        </button>
        <button
          type="button"
          onClick={async () => {
            if (showTrash) {
              setShowTrash(false);
              return;
            }
            await loadDeletedNotes();
            openOnlyPanel("trash");
          }}
          className={`focus-ring flex items-center justify-center rounded-lg text-sm font-medium ${showTrash ? "text-blue-600" : "text-gray-500"}`}
          aria-label={showTrash ? "收起回收站" : "打开回收站"}
        >
          <Trash2 className="h-4 w-4" />
        </button>
        <button
          type="button"
          onClick={() => {
            setShowAccount(true);
            if (!accountSummary) void loadAccountSummary();
          }}
          className={`focus-ring flex items-center justify-center rounded-lg text-sm font-medium ${showAccount ? "text-emerald-600" : "text-gray-500"}`}
          aria-label="打开账户与订阅"
        >
          <Settings className="h-4 w-4" />
        </button>
        <button
          type="button"
          onClick={onLogout}
          className="focus-ring flex items-center justify-center rounded-lg text-sm font-medium text-gray-500"
          aria-label={`退出登录，当前用户 ${userEmail}`}
        >
          <LogOut className="h-4 w-4" />
        </button>
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
          onRestore={(versionId) => void restoreVersion(activeNote.id, versionId)}
          onTogglePin={(versionId) => void toggleVersionPin(activeNote.id, versionId)}
          onDelete={(versionId) => void deleteVersion(activeNote.id, versionId)}
          onClear={() => void clearVersions(activeNote.id)}
        />
      )}

      {showAccount && (
        <AccountPanel
          summary={accountSummary}
          loading={accountLoading}
          error={accountError}
          onClose={() => setShowAccount(false)}
          onRefresh={async () => {
            await loadAccountSummary();
          }}
          onCheckout={async (priceId) => {
            const session = await billingApi.createCheckout({ price_id: priceId });
            window.location.href = session.checkout_url;
          }}
          onManageBilling={async () => {
            const portal = await billingApi.portal();
            window.open(portal.management_url, "_blank", "noopener,noreferrer");
          }}
        />
      )}
    </div>
  );
}
