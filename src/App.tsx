import React, { Suspense, useEffect, useState } from "react";
import { Sidebar } from "@/components/Sidebar";
import { useNotes } from "@/hooks/useNotes";
import { useSync } from "@/hooks/useSync";
import { useClipboard } from "@/hooks/useClipboard";
import { ClipboardPanel } from "@/components/ClipboardPanel";
import type { AppView, NoteSummary, NoteVersion, SyncConfig, SyncConfigInput, SyncStatus } from "@/types";

const NoteEditor = React.lazy(() =>
  import("@/components/NoteEditor").then((m) => ({ default: m.NoteEditor }))
);

export default function App() {
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
    togglePin,
    loadNotes,
    loadDeletedNotes,
    restoreNote,
    undoDelete,
    purgeNote,
    loadVersions,
    restoreVersion,
    toggleVersionPin,
    saveAttachment,
    resolveAttachment,
    flushAllDrafts,
    refreshAfterSync,
  } = useNotes();
  const clipboard = useClipboard();
  const [viewMode, setViewMode] = useState<AppView>("notes");
  const [showTrash, setShowTrash] = useState(false);
  const [showHistory, setShowHistory] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const sync = useSync({
    beforeSync: flushAllDrafts,
    onSynced: async () => {
      await Promise.all([refreshAfterSync(), clipboard.loadItems()]);
    },
  });

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Ctrl+N: New note
      if ((e.ctrlKey || e.metaKey) && e.key === "n") {
        e.preventDefault();
        setViewMode("notes");
        createNote();
      }
      // Ctrl+F: Focus search
      if ((e.ctrlKey || e.metaKey) && e.key === "f") {
        e.preventDefault();
        const searchInput = document.querySelector<HTMLInputElement>(
          viewMode === "notes"
            ? 'input[placeholder="搜索便签..."]'
            : 'input[placeholder="搜索剪贴板历史"]'
        );
        searchInput?.focus();
      }
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "z" && e.shiftKey) {
        e.preventDefault();
        undoDelete();
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
        onOpenTrash={async () => {
          await loadDeletedNotes();
          setShowTrash(true);
        }}
        syncStatus={sync.status}
        onSync={() => void sync.syncNow()}
        onOpenSettings={() => setShowSettings(true)}
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
            onCapture={() => void clipboard.capture()}
            onAutoCaptureChange={clipboard.setAutoCaptureEnabled}
            onCopy={(id) => void clipboard.copyItem(id)}
            onTogglePin={(id) => void clipboard.togglePin(id)}
            onDelete={(id) => void clipboard.deleteItem(id)}
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
            <button onClick={() => void createNote()} className="rounded-lg bg-blue-600 px-3 py-2 text-sm font-medium text-white">新建</button>
          </div>
          <Suspense fallback={<EditorSkeleton />}>
          {activeNote ? (
            <NoteEditor
              note={activeNote}
              onUpdate={updateNote}
              onSaveAttachment={saveAttachment}
              onResolveAttachment={resolveAttachment}
              onOpenHistory={async () => {
                await loadVersions(activeNote.id);
                setShowHistory(true);
              }}
              saveStatus={saveStatus}
              errorMessage={errorMessage}
              isSyncing={sync.status === "syncing"}
            />
          ) : (
            <EmptyState onCreateNote={createNote} />
          )}
          </Suspense>
        </>}
      </div>

      <nav className="fixed inset-x-0 bottom-0 z-30 grid h-14 grid-cols-4 border-t border-gray-200 bg-white/95 px-2 backdrop-blur md:hidden">
        <button onClick={() => setViewMode("notes")} className={viewMode === "notes" ? "text-blue-600" : "text-gray-500"}>便签</button>
        <button onClick={() => setViewMode("clipboard")} className={viewMode === "clipboard" ? "text-violet-600" : "text-gray-500"}>剪贴板</button>
        <button onClick={() => void sync.syncNow()} className="text-gray-500">同步</button>
        <button onClick={() => setShowSettings(true)} className="text-gray-500">设置</button>
      </nav>

      {errorMessage && (
        <div className="fixed right-4 bottom-4 max-w-sm rounded border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700 shadow">
          {errorMessage}
        </div>
      )}

      {showTrash && (
        <TrashPanel
          notes={deletedNotes}
          onClose={() => setShowTrash(false)}
          onRestore={restoreNote}
          onPurge={purgeNote}
        />
      )}

      {showHistory && activeNote && (
        <HistoryPanel
          versions={versions}
          onClose={() => setShowHistory(false)}
          onRestore={(versionId) => restoreVersion(activeNote.id, versionId)}
          onTogglePin={(versionId) => toggleVersionPin(activeNote.id, versionId)}
        />
      )}

      {showSettings && (
        <SyncSettingsPanel
          config={sync.config}
          status={sync.status}
          error={sync.error}
          onClose={() => setShowSettings(false)}
          onSave={sync.saveConfig}
          onSync={sync.syncNow}
        />
      )}
    </div>
  );
}

function SyncSettingsPanel({
  config,
  status,
  error,
  onClose,
  onSave,
  onSync,
}: {
  config: SyncConfig | null;
  status: SyncStatus;
  error: string | null;
  onClose: () => void;
  onSave: (input: SyncConfigInput) => Promise<SyncConfig>;
  onSync: () => Promise<boolean>;
}) {
  const [enabled, setEnabled] = useState(config?.enabled ?? false);
  const [endpoint, setEndpoint] = useState(config?.endpoint ?? "");
  const [username, setUsername] = useState(config?.username ?? "");
  const [password, setPassword] = useState("");
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!config) return;
    setEnabled(config.enabled);
    setEndpoint(config.endpoint);
    setUsername(config.username);
  }, [config]);

  const save = async () => {
    setSaving(true);
    try {
      await onSave({
        enabled,
        provider: "webdav",
        endpoint,
        username,
        password: password || undefined,
      });
      setPassword("");
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex justify-end bg-black/20" onMouseDown={onClose}>
      <div
        className="h-full w-full max-w-sm border-l border-gray-200 bg-white shadow-xl"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <div className="flex items-center justify-between border-b border-gray-100 px-5 py-4">
          <h2 className="text-sm font-semibold text-gray-800">数据同步</h2>
          <button onClick={onClose} className="h-7 w-7 rounded hover:bg-gray-100" title="关闭">×</button>
        </div>
        <div className="space-y-5 p-5">
          <label className="flex items-center justify-between text-sm text-gray-700">
            <span>启用 WebDAV 同步</span>
            <input
              type="checkbox"
              checked={enabled}
              onChange={(event) => setEnabled(event.target.checked)}
              className="h-4 w-4 accent-blue-600"
            />
          </label>
          <label className="block text-sm text-gray-600">
            <span className="mb-1.5 block">服务器目录</span>
            <input
              value={endpoint}
              onChange={(event) => setEndpoint(event.target.value)}
              placeholder="https://dav.example.com/QuickNote"
              className="w-full rounded border border-gray-200 px-3 py-2 outline-none focus:border-blue-500"
            />
          </label>
          <label className="block text-sm text-gray-600">
            <span className="mb-1.5 block">用户名</span>
            <input
              value={username}
              onChange={(event) => setUsername(event.target.value)}
              className="w-full rounded border border-gray-200 px-3 py-2 outline-none focus:border-blue-500"
            />
          </label>
          <label className="block text-sm text-gray-600">
            <span className="mb-1.5 block">应用密码</span>
            <input
              type="password"
              value={password}
              onChange={(event) => setPassword(event.target.value)}
              placeholder={config?.enabled ? "留空则保持不变" : "WebDAV 应用密码"}
              className="w-full rounded border border-gray-200 px-3 py-2 outline-none focus:border-blue-500"
            />
          </label>
          {error && <p className="rounded bg-red-50 px-3 py-2 text-xs text-red-700">{error}</p>}
          <div className="flex items-center gap-2 border-t border-gray-100 pt-4">
            <button
              onClick={() => void save()}
              disabled={saving}
              className="rounded bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50"
            >
              {saving ? "保存中" : "保存配置"}
            </button>
            <button
              onClick={() => void onSync()}
              disabled={!config?.enabled || status === "syncing"}
              className="rounded px-4 py-2 text-sm text-gray-700 hover:bg-gray-100 disabled:opacity-40"
            >
              {status === "syncing" ? "同步中" : "立即同步"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

function TrashPanel({
  notes,
  onClose,
  onRestore,
  onPurge,
}: {
  notes: NoteSummary[];
  onClose: () => void;
  onRestore: (id: string) => void;
  onPurge: (id: string) => void;
}) {
  return (
    <div className="fixed inset-y-0 right-0 z-40 w-80 border-l border-gray-200 bg-white shadow-xl">
      <div className="flex items-center justify-between border-b border-gray-100 px-4 py-3">
        <h2 className="text-sm font-semibold text-gray-800">回收站</h2>
        <button onClick={onClose} className="w-7 h-7 rounded hover:bg-gray-100" title="关闭">
          ×
        </button>
      </div>
      <div className="h-[calc(100%-49px)] overflow-y-auto">
        {notes.length === 0 ? (
          <p className="p-4 text-sm text-gray-400">回收站为空</p>
        ) : (
          notes.map((note) => (
            <div key={note.id} className="border-b border-gray-100 px-4 py-3">
              <h3 className="truncate text-sm font-medium text-gray-800">{note.title}</h3>
              <p className="mt-1 line-clamp-2 text-xs text-gray-500">{note.preview || "空便签"}</p>
              <div className="mt-3 flex gap-2">
                <button
                  onClick={() => onRestore(note.id)}
                  className="rounded bg-blue-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-blue-700"
                >
                  恢复
                </button>
                <button
                  onClick={() => onPurge(note.id)}
                  className="rounded px-3 py-1.5 text-xs font-medium text-red-600 hover:bg-red-50"
                >
                  永久删除
                </button>
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
}

function HistoryPanel({
  versions,
  onClose,
  onRestore,
  onTogglePin,
}: {
  versions: NoteVersion[];
  onClose: () => void;
  onRestore: (versionId: number) => void;
  onTogglePin: (versionId: number) => void;
}) {
  return (
    <div className="fixed inset-y-0 right-0 z-40 w-80 border-l border-gray-200 bg-white shadow-xl">
      <div className="flex items-center justify-between border-b border-gray-100 px-4 py-3">
        <div>
          <h2 className="text-sm font-semibold text-gray-800">历史版本</h2>
          <p className="mt-0.5 text-xs text-gray-400">每 5 分钟自动保存，未固定最多 10 个</p>
        </div>
        <button onClick={onClose} className="w-7 h-7 rounded hover:bg-gray-100" title="关闭">
          ×
        </button>
      </div>
      <div className="h-[calc(100%-65px)] overflow-y-auto">
        {versions.length === 0 ? (
          <p className="p-4 text-sm text-gray-400">暂无历史版本</p>
        ) : (
          versions.map((version) => (
            <div key={version.id} className="border-b border-gray-100 px-4 py-3">
              <div className="flex items-center justify-between gap-2">
                <h3 className="truncate text-sm font-medium text-gray-800">
                  {new Date(version.created_at).toLocaleString("zh-CN")}
                </h3>
                {version.is_pinned && (
                  <span className="flex h-5 w-5 flex-shrink-0 items-center justify-center text-blue-600" title="已固定">
                    <PinIcon />
                  </span>
                )}
              </div>
              <p className="mt-1 line-clamp-3 text-xs text-gray-500">{stripHtml(version.content)}</p>
              <div className="mt-3 flex gap-1">
                <button
                  onClick={() => onRestore(version.id)}
                  className="flex h-8 w-8 items-center justify-center rounded text-blue-600 hover:bg-blue-50"
                  title="恢复此版本"
                >
                  <RestoreIcon />
                </button>
                <button
                  onClick={() => onTogglePin(version.id)}
                  className={`flex h-8 w-8 items-center justify-center rounded hover:bg-gray-100 ${
                    version.is_pinned ? "text-blue-600" : "text-gray-600"
                  }`}
                  title={version.is_pinned ? "取消固定" : "固定此版本"}
                >
                  <PinIcon />
                </button>
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
}

function stripHtml(html: string): string {
  return html.replace(/<[^>]+>/g, " ").replace(/\s+/g, " ").trim() || "空便签";
}

function RestoreIcon() {
  return (
    <svg className="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 10h10a6 6 0 016 6 6 6 0 01-6 6H7m-4-12l4-4m-4 4l4 4" />
    </svg>
  );
}

function PinIcon() {
  return (
    <svg className="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M14 4l6 6-3 1-4 4v5l-2 2-2-7-7-2 2-2h5l4-4 1-3z" />
    </svg>
  );
}

function EditorSkeleton() {
  return (
    <div className="flex flex-col h-full animate-pulse">
      {/* Toolbar skeleton */}
      <div className="px-8 py-2 border-b border-gray-100 flex items-center gap-2">
        {Array.from({ length: 12 }).map((_, i) => (
          <div key={i} className="w-7 h-7 rounded bg-gray-100" />
        ))}
      </div>
      {/* Content skeleton */}
      <div className="flex-1 px-8 py-6 space-y-4">
        <div className="h-5 w-3/4 rounded bg-gray-100" />
        <div className="h-4 w-full rounded bg-gray-50" />
        <div className="h-4 w-5/6 rounded bg-gray-50" />
        <div className="h-4 w-2/3 rounded bg-gray-50" />
      </div>
      {/* Status bar skeleton */}
      <div className="px-8 py-2 border-t border-gray-100">
        <div className="h-3 w-20 rounded bg-gray-100" />
      </div>
    </div>
  );
}

function EmptyState({ onCreateNote }: { onCreateNote: () => void }) {
  return (
    <div className="flex-1 flex flex-col items-center justify-center text-gray-400 p-8">
      <div className="w-24 h-24 mb-6 rounded-2xl bg-gray-100 flex items-center justify-center">
        <svg
          className="w-12 h-12 text-gray-300"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={1.5}
            d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z"
          />
        </svg>
      </div>
      <h2 className="text-xl font-medium text-gray-500 mb-2">选择或创建一个便签</h2>
      <p className="text-sm text-gray-400 mb-6 text-center max-w-xs">
        从左侧列表选择一个便签开始编辑，或创建一个新的便签
      </p>
      <button
        onClick={onCreateNote}
        className="px-6 py-2.5 bg-blue-600 text-white rounded-lg hover:bg-blue-700 transition-colors font-medium shadow-sm flex items-center gap-2"
      >
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
        </svg>
        新建便签
      </button>
      <div className="mt-8 text-xs text-gray-300 space-y-1 text-center">
        <p>
          <kbd className="px-1.5 py-0.5 bg-gray-100 rounded text-gray-500 font-mono text-xs">
            Ctrl+N
          </kbd>{" "}
          新建便签
        </p>
        <p>
          <kbd className="px-1.5 py-0.5 bg-gray-100 rounded text-gray-500 font-mono text-xs">
            Ctrl+F
          </kbd>{" "}
          搜索便签
        </p>
        <p>
          <kbd className="px-1.5 py-0.5 bg-gray-100 rounded text-gray-500 font-mono text-xs">
            Ctrl+Alt+N
          </kbd>{" "}
          全局呼出
        </p>
      </div>
    </div>
  );
}
