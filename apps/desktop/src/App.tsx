import React, { Suspense, useEffect, useState } from "react";
import { Sidebar } from "@/components/Sidebar";
import { useNotes } from "@/hooks/useNotes";
import { useSync } from "@/hooks/useSync";
import { useClipboard } from "@/hooks/useClipboard";
import { ClipboardPanel } from "@ui/components/ClipboardPanel";
import { EmptyState } from "@ui/components/EmptyState";
import { EditorSkeleton } from "@ui/components/EditorSkeleton";
import { stripHtml } from "@ui/utils/html";
import {
  X,
  RotateCcw,
  Pin,
  PinOff,
  Trash2,
  Eraser,
} from "lucide-react";
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
    deleteVersion,
    clearVersions,
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
  const [deletedToast, setDeletedToast] = useState<{
    title: string;
  } | null>(null);
  const sync = useSync({
    beforeSync: flushAllDrafts,
    onSynced: async () => {
      await Promise.all([refreshAfterSync(), clipboard.loadItems()]);
    },
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
        clipboardItems={clipboard.items}
        notes={notes}
        activeNoteId={activeNote?.id ?? null}
        searchQuery={searchQuery}
        onSearchChange={setSearchQuery}
        onSelectNote={selectNote}
        onCreateNote={createNote}
        onDeleteNote={(id) => void handleDeleteNote(id)}
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
            onCapture={clipboard.capture}
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
            <button type="button" onClick={() => void createNote()} className="rounded-lg bg-blue-600 px-3 py-2 text-sm font-medium text-white">新建</button>
          </div>
          <Suspense fallback={<EditorSkeleton showStatusBar />}>
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
        <button type="button" onClick={() => setViewMode("notes")} aria-pressed={viewMode === "notes"} className={`focus-ring rounded-lg text-sm font-medium ${viewMode === "notes" ? "text-blue-600" : "text-gray-500"}`}>便签</button>
        <button type="button" onClick={() => setViewMode("clipboard")} aria-pressed={viewMode === "clipboard"} className={`focus-ring rounded-lg text-sm font-medium ${viewMode === "clipboard" ? "text-violet-600" : "text-gray-500"}`}>剪贴板</button>
        <button type="button" onClick={() => void sync.syncNow()} className="focus-ring rounded-lg text-sm font-medium text-gray-500">同步</button>
        <button type="button" onClick={() => setShowSettings(true)} className="focus-ring rounded-lg text-sm font-medium text-gray-500">设置</button>
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
          onClose={() => setShowSettings(false)}
          onSave={sync.saveConfig}
          onSync={sync.syncNow}
        />
      )}
    </div>
  );
}

// ── Desktop-only panels ──

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
  const [cloudEnabled, setCloudEnabled] = useState(config?.cloud_enabled ?? false);
  const [cloudUrl, setCloudUrl] = useState(config?.cloud_url ?? "");
  const [cloudEmail, setCloudEmail] = useState(config?.cloud_email ?? "");
  const [cloudPassword, setCloudPassword] = useState("");
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!config) return;
    setEnabled(config.enabled);
    setEndpoint(config.endpoint);
    setUsername(config.username);
    setCloudEnabled(config.cloud_enabled ?? false);
    setCloudUrl(config.cloud_url ?? "");
    setCloudEmail(config.cloud_email ?? "");
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
        cloud_enabled: cloudEnabled,
        cloud_url: cloudUrl,
        cloud_email: cloudEmail,
        cloud_password: cloudPassword || undefined,
      });
      setPassword("");
      setCloudPassword("");
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="animate-fade-in fixed inset-0 z-50 flex justify-end bg-black/20" onMouseDown={onClose}>
      <div
        className="animate-drawer-in h-full w-full max-w-sm border-l border-gray-200 bg-white shadow-xl overflow-y-auto"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <div className="flex items-center justify-between border-b border-gray-100 px-5 py-4 sticky top-0 bg-white z-10">
          <h2 className="text-sm font-semibold text-gray-800">数据同步</h2>
          <button type="button" onClick={onClose} className="h-7 w-7 rounded hover:bg-gray-100 flex items-center justify-center" title="关闭" aria-label="关闭">
            <X className="h-4 w-4 text-gray-500" />
          </button>
        </div>

        {/* WebDAV Section */}
        <div className="space-y-5 p-5">
          <div className="flex items-center gap-2 border-b border-gray-100 pb-2">
            <span className="text-xs font-medium text-gray-400 uppercase tracking-wider">WebDAV</span>
          </div>
          <label className="flex items-center justify-between text-sm text-gray-700">
            <span>启用 WebDAV 同步</span>
            <input type="checkbox" checked={enabled} onChange={(event) => setEnabled(event.target.checked)} className="h-4 w-4 accent-blue-600" />
          </label>
          <label className="block text-sm text-gray-600">
            <span className="mb-1.5 block">服务器目录</span>
            <input value={endpoint} onChange={(event) => setEndpoint(event.target.value)} placeholder="https://dav.example.com/QuickNote" className="w-full rounded border border-gray-200 px-3 py-2 outline-none focus:border-blue-500" />
          </label>
          <label className="block text-sm text-gray-600">
            <span className="mb-1.5 block">用户名</span>
            <input value={username} onChange={(event) => setUsername(event.target.value)} className="w-full rounded border border-gray-200 px-3 py-2 outline-none focus:border-blue-500" />
          </label>
          <label className="block text-sm text-gray-600">
            <span className="mb-1.5 block">应用密码</span>
            <input type="password" value={password} onChange={(event) => setPassword(event.target.value)} placeholder={config?.enabled ? "留空则保持不变" : "WebDAV 应用密码"} className="w-full rounded border border-gray-200 px-3 py-2 outline-none focus:border-blue-500" />
          </label>
        </div>

        {/* Cloud Sync Section */}
        <div className="space-y-5 border-t border-gray-200 p-5">
          <div className="flex items-center gap-2 border-b border-gray-100 pb-2">
            <span className="text-xs font-medium text-gray-400 uppercase tracking-wider">云同步</span>
          </div>
          <label className="flex items-center justify-between text-sm text-gray-700">
            <span>启用云同步</span>
            <input type="checkbox" checked={cloudEnabled} onChange={(event) => setCloudEnabled(event.target.checked)} className="h-4 w-4 accent-violet-600" />
          </label>
          <label className="block text-sm text-gray-600">
            <span className="mb-1.5 block">云服务地址</span>
            <input value={cloudUrl} onChange={(event) => setCloudUrl(event.target.value)} placeholder="https://cloud.quicknote.app" className="w-full rounded border border-gray-200 px-3 py-2 outline-none focus:border-violet-500" />
          </label>
          <label className="block text-sm text-gray-600">
            <span className="mb-1.5 block">邮箱</span>
            <input value={cloudEmail} onChange={(event) => setCloudEmail(event.target.value)} placeholder="user@example.com" className="w-full rounded border border-gray-200 px-3 py-2 outline-none focus:border-violet-500" />
          </label>
          <label className="block text-sm text-gray-600">
            <span className="mb-1.5 block">密码</span>
            <input type="password" value={cloudPassword} onChange={(event) => setCloudPassword(event.target.value)} placeholder={cloudEnabled ? "留空则保持不变" : "云服务密码"} className="w-full rounded border border-gray-200 px-3 py-2 outline-none focus:border-violet-500" />
          </label>
        </div>

        {/* Actions */}
        <div className="p-5 border-t border-gray-200 sticky bottom-0 bg-white">
          {error && <p className="mb-3 rounded bg-red-50 px-3 py-2 text-xs text-red-700">{error}</p>}
          <div className="flex items-center gap-2">
            <button type="button" onClick={() => void save()} disabled={saving} className="rounded bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50">
              {saving ? "保存中" : "保存配置"}
            </button>
            <button type="button" onClick={() => void onSync()} disabled={status === "syncing"} className="rounded px-4 py-2 text-sm text-gray-700 hover:bg-gray-100 disabled:opacity-40">
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
    <div className="animate-drawer-in fixed inset-y-0 right-0 z-40 w-80 border-l border-gray-200 bg-white shadow-xl">
      <div className="flex items-center justify-between border-b border-gray-100 px-4 py-3">
        <h2 className="text-sm font-semibold text-gray-800">回收站</h2>
        <button type="button" onClick={onClose} className="w-7 h-7 rounded hover:bg-gray-100 flex items-center justify-center" title="关闭" aria-label="关闭">
          <X className="h-4 w-4 text-gray-500" />
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
                <button type="button" onClick={() => onRestore(note.id)} className="rounded bg-blue-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-blue-700">恢复</button>
                <button type="button" onClick={() => onPurge(note.id)} className="rounded px-3 py-1.5 text-xs font-medium text-red-600 hover:bg-red-50">永久删除</button>
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
  onDelete,
  onClear,
}: {
  versions: NoteVersion[];
  onClose: () => void;
  onRestore: (versionId: number) => void;
  onTogglePin: (versionId: number) => void;
  onDelete: (versionId: number) => void;
  onClear: () => void;
}) {
  const unpinnedCount = versions.filter((v) => !v.is_pinned).length;

  return (
    <div className="animate-drawer-in fixed inset-y-0 right-0 z-40 w-80 border-l border-gray-200 bg-white shadow-xl">
      <div className="flex items-center justify-between border-b border-gray-100 px-4 py-3">
        <div>
          <h2 className="text-sm font-semibold text-gray-800">历史版本</h2>
          <p className="mt-0.5 text-xs text-gray-400">每 5 分钟自动保存，未固定最多 10 个</p>
        </div>
        <div className="flex items-center gap-1">
          {unpinnedCount > 0 && (
            <button type="button" onClick={onClear} className="flex h-7 items-center gap-1 rounded px-2 text-xs text-red-500 hover:bg-red-50" title="清空所有未固定版本" aria-label="清空所有未固定版本">
              <Eraser className="h-3.5 w-3.5" />
            </button>
          )}
          <button type="button" onClick={onClose} className="h-7 w-7 rounded hover:bg-gray-100 flex items-center justify-center" title="关闭" aria-label="关闭">
            <X className="h-4 w-4 text-gray-500" />
          </button>
        </div>
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
                    <Pin className="h-3.5 w-3.5" />
                  </span>
                )}
              </div>
              <p className="mt-1 line-clamp-3 text-xs text-gray-500">{stripHtml(version.content)}</p>
              <div className="mt-3 flex gap-1">
                <button type="button" onClick={() => onRestore(version.id)} className="flex h-8 w-8 items-center justify-center rounded text-blue-600 hover:bg-blue-50" title="恢复此版本" aria-label="恢复此版本">
                  <RotateCcw className="h-4 w-4" />
                </button>
                <button
                  type="button"
                  onClick={() => onTogglePin(version.id)}
                  className={`flex h-8 w-8 items-center justify-center rounded hover:bg-gray-100 ${version.is_pinned ? "text-blue-600" : "text-gray-600"}`}
                  title={version.is_pinned ? "取消固定" : "固定此版本"}
                  aria-label={version.is_pinned ? "取消固定" : "固定此版本"}
                >
                  {version.is_pinned ? <PinOff className="h-4 w-4" /> : <Pin className="h-4 w-4" />}
                </button>
                <button type="button" onClick={() => onDelete(version.id)} className="flex h-8 w-8 items-center justify-center rounded text-gray-400 hover:bg-red-50 hover:text-red-500" title="删除此版本" aria-label="删除此版本">
                  <Trash2 className="h-4 w-4" />
                </button>
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
