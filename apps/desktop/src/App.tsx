import React, { Suspense, useEffect, useState } from "react";
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
import {
  X,
  RotateCcw,
  Pin,
  PinOff,
  Trash2,
  Eraser,
  Cloud,
  Server,
  Keyboard,
  Save,
  RefreshCw,
} from "lucide-react";
import type {
  AppView,
  ClipboardItem,
  NoteSummary,
  NoteVersion,
  ShortcutConfig,
  ShortcutConfigInput,
  SyncConfig,
  SyncConfigInput,
  SyncStatus,
} from "@/types";

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
    reorderNotes,
    loadNotes,
    loadDeletedNotes,
    restoreNote,
    undoDelete,
    purgeNote,
    purgeAllNotes,
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
  const [focusedClipboardItemId, setFocusedClipboardItemId] = useState<string | null>(null);
  const [shortcutConfig, setShortcutConfig] = useState<ShortcutConfig | null>(null);
  const [shortcutError, setShortcutError] = useState<string | null>(null);
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
              onOpenHistory={handleOpenHistory}
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
          onSaveShortcuts={saveShortcuts}
        />
      )}
    </div>
  );
}

// ── Shortcut key capture input ──

const MODIFIER_KEYS = new Set(["Control", "Alt", "Shift", "Meta"]);

function formatKey(event: KeyboardEvent): string {
  const parts: string[] = [];
  if (event.ctrlKey) parts.push("Ctrl");
  if (event.altKey) parts.push("Alt");
  if (event.shiftKey) parts.push("Shift");
  if (event.metaKey) parts.push("Win");

  const key = event.key;
  if (MODIFIER_KEYS.has(key)) return parts.join("+");

  let displayKey = key;
  if (key === " ") displayKey = "Space";
  else if (key.length === 1) displayKey = key.toUpperCase();

  parts.push(displayKey);
  return parts.join("+");
}

function hasValidModifier(shortcut: string): boolean {
  const parts = shortcut.split("+");
  const modifiers = parts.slice(0, -1);
  if (modifiers.length === 0) return false;
  const hasFunctionalModifier = modifiers.some(
    (m) => m === "Ctrl" || m === "Alt" || m === "Shift",
  );
  return hasFunctionalModifier;
}

function ShortcutCaptureInput({
  value,
  onChange,
  placeholder,
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder: string;
}) {
  const [capturing, setCapturing] = useState(false);
  const [currentKeys, setCurrentKeys] = useState("");
  const [error, setError] = useState("");
  const inputRef = React.useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!capturing) return;

    const handleKeyDown = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();

      const key = event.key;
      if (key === "Escape") {
        setCapturing(false);
        setCurrentKeys("");
        setError("");
        return;
      }
      if (key === "Backspace" || key === "Delete") {
        onChange("");
        setCapturing(false);
        setCurrentKeys("");
        setError("");
        return;
      }

      const combo = formatKey(event);
      if (!combo || MODIFIER_KEYS.has(event.key)) {
        setCurrentKeys(combo);
        return;
      }

      if (!hasValidModifier(combo)) {
        setError("需要 Ctrl/Alt/Shift 功能键参与");
        setCurrentKeys(combo);
        return;
      }

      setError("");
      onChange(combo);
      setCapturing(false);
      setCurrentKeys("");
    };

    window.addEventListener("keydown", handleKeyDown, true);
    return () => window.removeEventListener("keydown", handleKeyDown, true);
  }, [capturing, onChange]);

  const displayValue = capturing
    ? currentKeys || "请按下快捷键组合…"
    : value || "未设置";

  return (
    <div>
      <button
        type="button"
        ref={inputRef as unknown as React.Ref<HTMLButtonElement>}
        onClick={() => {
          setCapturing((v) => !v);
          setError("");
          setCurrentKeys("");
        }}
        className={`w-full rounded-lg border px-3 py-2 font-mono text-sm text-left transition-colors ${
          capturing
            ? "border-emerald-400 bg-emerald-50 text-emerald-700 ring-2 ring-emerald-100"
            : value
              ? "border-gray-200 bg-gray-50 text-gray-800 hover:border-gray-300"
              : "border-gray-200 bg-white text-gray-400 hover:border-gray-300"
        }`}
        title={capturing ? "按 Esc 取消，按 Backspace 清除" : "点击后按下快捷键"}
      >
        <span className="flex items-center justify-between gap-2">
          <span className={capturing && !currentKeys ? "animate-pulse" : ""}>
            {displayValue}
          </span>
          {capturing && (
            <kbd className="rounded bg-white px-1.5 py-0.5 text-[10px] text-gray-400 border border-gray-200">
              Esc
            </kbd>
          )}
        </span>
      </button>
      {error && (
        <p className="mt-1 text-xs text-orange-600">{error}</p>
      )}
      {!value && !capturing && (
        <p className="mt-1 text-xs text-gray-400">{placeholder}</p>
      )}
    </div>
  );
}

// ── Desktop-only panels ──

function SyncSettingsPanel({
  config,
  status,
  error,
  shortcutConfig,
  shortcutError,
  onClose,
  onSave,
  onSync,
  onSaveShortcuts,
}: {
  config: SyncConfig | null;
  status: SyncStatus;
  error: string | null;
  shortcutConfig: ShortcutConfig | null;
  shortcutError: string | null;
  onClose: () => void;
  onSave: (input: SyncConfigInput) => Promise<SyncConfig>;
  onSync: () => Promise<boolean>;
  onSaveShortcuts: (input: ShortcutConfigInput) => Promise<ShortcutConfig>;
}) {
  const [enabled, setEnabled] = useState(config?.enabled ?? false);
  const [endpoint, setEndpoint] = useState(config?.endpoint ?? "");
  const [username, setUsername] = useState(config?.username ?? "");
  const [password, setPassword] = useState("");
  const [cloudEnabled, setCloudEnabled] = useState(config?.cloud_enabled ?? false);
  const [cloudUrl, setCloudUrl] = useState(config?.cloud_url ?? "");
  const [cloudEmail, setCloudEmail] = useState(config?.cloud_email ?? "");
  const [cloudPassword, setCloudPassword] = useState("");
  const [quickNoteShortcut, setQuickNoteShortcut] = useState(shortcutConfig?.quick_note ?? "");
  const [clipboardShortcut, setClipboardShortcut] = useState(shortcutConfig?.clipboard_history ?? "");
  const [alternateShortcut, setAlternateShortcut] = useState(shortcutConfig?.quick_note_alternate ?? "");
  const [saving, setSaving] = useState(false);
  const [savingShortcuts, setSavingShortcuts] = useState(false);

  useEffect(() => {
    if (!config) return;
    setEnabled(config.enabled);
    setEndpoint(config.endpoint);
    setUsername(config.username);
    setCloudEnabled(config.cloud_enabled ?? false);
    setCloudUrl(config.cloud_url ?? "");
    setCloudEmail(config.cloud_email ?? "");
  }, [config]);

  useEffect(() => {
    if (!shortcutConfig) return;
    setQuickNoteShortcut(shortcutConfig.quick_note);
    setClipboardShortcut(shortcutConfig.clipboard_history);
    setAlternateShortcut(shortcutConfig.quick_note_alternate);
  }, [shortcutConfig]);

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

  const saveShortcuts = async () => {
    setSavingShortcuts(true);
    try {
      await onSaveShortcuts({
        quick_note: quickNoteShortcut,
        clipboard_history: clipboardShortcut,
        quick_note_alternate: alternateShortcut,
      });
    } finally {
      setSavingShortcuts(false);
    }
  };

  return (
    <div className="animate-fade-in fixed inset-0 z-50 flex justify-end bg-black/20" onMouseDown={onClose}>
      <div
        className="animate-drawer-in h-full w-full max-w-sm bg-gray-50 shadow-xl overflow-y-auto"
        onMouseDown={(event) => event.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between border-b border-gray-100 bg-white px-5 py-4 sticky top-0 z-10 shadow-sm">
          <h2 className="text-sm font-semibold text-gray-800">设置</h2>
          <button type="button" onClick={onClose} className="h-7 w-7 rounded hover:bg-gray-100 flex items-center justify-center" title="关闭" aria-label="关闭">
            <X className="h-4 w-4 text-gray-500" />
          </button>
        </div>

        <div className="flex flex-col gap-4 p-4">

          {/* WebDAV Section */}
          <section className="rounded-xl bg-white border border-gray-100 shadow-sm overflow-hidden">
            <div className="flex items-center gap-2.5 border-b border-gray-100 bg-blue-50/40 px-4 py-3">
              <div className="flex h-7 w-7 items-center justify-center rounded-lg bg-blue-100">
                <Server className="h-3.5 w-3.5 text-blue-600" />
              </div>
              <span className="text-xs font-semibold text-blue-700 uppercase tracking-wider">WebDAV 同步</span>
            </div>
            <div className="space-y-4 p-4">
              <label className="flex items-center justify-between text-sm text-gray-700">
                <span>启用 WebDAV</span>
                <input type="checkbox" checked={enabled} onChange={(event) => setEnabled(event.target.checked)} className="h-4 w-4 accent-blue-600 rounded" />
              </label>
              <label className="block text-sm text-gray-600">
                <span className="mb-1.5 block text-xs text-gray-500">服务器目录</span>
                <input value={endpoint} onChange={(event) => setEndpoint(event.target.value)} placeholder="https://dav.example.com/QuickNote" className="w-full rounded-lg border border-gray-200 bg-gray-50 px-3 py-2 text-sm outline-none transition focus:border-blue-400 focus:bg-white focus:ring-2 focus:ring-blue-50" />
              </label>
              <label className="block text-sm text-gray-600">
                <span className="mb-1.5 block text-xs text-gray-500">用户名</span>
                <input value={username} onChange={(event) => setUsername(event.target.value)} className="w-full rounded-lg border border-gray-200 bg-gray-50 px-3 py-2 text-sm outline-none transition focus:border-blue-400 focus:bg-white focus:ring-2 focus:ring-blue-50" />
              </label>
              <label className="block text-sm text-gray-600">
                <span className="mb-1.5 block text-xs text-gray-500">应用密码</span>
                <input type="password" value={password} onChange={(event) => setPassword(event.target.value)} placeholder={config?.enabled ? "留空则保持不变" : "WebDAV 应用密码"} className="w-full rounded-lg border border-gray-200 bg-gray-50 px-3 py-2 text-sm outline-none transition focus:border-blue-400 focus:bg-white focus:ring-2 focus:ring-blue-50" />
              </label>
            </div>
          </section>

          {/* Cloud Sync Section */}
          <section className="rounded-xl bg-white border border-gray-100 shadow-sm overflow-hidden">
            <div className="flex items-center gap-2.5 border-b border-gray-100 bg-violet-50/40 px-4 py-3">
              <div className="flex h-7 w-7 items-center justify-center rounded-lg bg-violet-100">
                <Cloud className="h-3.5 w-3.5 text-violet-600" />
              </div>
              <span className="text-xs font-semibold text-violet-700 uppercase tracking-wider">云同步</span>
            </div>
            <div className="space-y-4 p-4">
              <label className="flex items-center justify-between text-sm text-gray-700">
                <span>启用云同步</span>
                <input type="checkbox" checked={cloudEnabled} onChange={(event) => setCloudEnabled(event.target.checked)} className="h-4 w-4 accent-violet-600 rounded" />
              </label>
              <label className="block text-sm text-gray-600">
                <span className="mb-1.5 block text-xs text-gray-500">云服务地址</span>
                <input value={cloudUrl} onChange={(event) => setCloudUrl(event.target.value)} placeholder="https://cloud.quicknote.app" className="w-full rounded-lg border border-gray-200 bg-gray-50 px-3 py-2 text-sm outline-none transition focus:border-violet-400 focus:bg-white focus:ring-2 focus:ring-violet-50" />
              </label>
              <label className="block text-sm text-gray-600">
                <span className="mb-1.5 block text-xs text-gray-500">邮箱</span>
                <input value={cloudEmail} onChange={(event) => setCloudEmail(event.target.value)} placeholder="user@example.com" className="w-full rounded-lg border border-gray-200 bg-gray-50 px-3 py-2 text-sm outline-none transition focus:border-violet-400 focus:bg-white focus:ring-2 focus:ring-violet-50" />
              </label>
              <label className="block text-sm text-gray-600">
                <span className="mb-1.5 block text-xs text-gray-500">密码</span>
                <input type="password" value={cloudPassword} onChange={(event) => setCloudPassword(event.target.value)} placeholder={cloudEnabled ? "留空则保持不变" : "云服务密码"} className="w-full rounded-lg border border-gray-200 bg-gray-50 px-3 py-2 text-sm outline-none transition focus:border-violet-400 focus:bg-white focus:ring-2 focus:ring-violet-50" />
              </label>
            </div>
          </section>

          {/* Shortcuts Section */}
          <section className="rounded-xl bg-white border border-gray-100 shadow-sm overflow-hidden">
            <div className="flex items-center gap-2.5 border-b border-gray-100 bg-emerald-50/40 px-4 py-3">
              <div className="flex h-7 w-7 items-center justify-center rounded-lg bg-emerald-100">
                <Keyboard className="h-3.5 w-3.5 text-emerald-600" />
              </div>
              <span className="text-xs font-semibold text-emerald-700 uppercase tracking-wider">快捷键</span>
            </div>
            <div className="space-y-4 p-4">
              <div>
                <span className="mb-1.5 block text-xs text-gray-500">快速便签</span>
                <ShortcutCaptureInput
                  value={quickNoteShortcut}
                  onChange={setQuickNoteShortcut}
                  placeholder="点击后按下快捷键，如 Ctrl+Alt+N"
                />
              </div>
              <div>
                <span className="mb-1.5 block text-xs text-gray-500">剪贴板历史</span>
                <ShortcutCaptureInput
                  value={clipboardShortcut}
                  onChange={setClipboardShortcut}
                  placeholder="点击后按下快捷键，如 Ctrl+Alt+C"
                />
              </div>
              <div>
                <span className="mb-1.5 block text-xs text-gray-500">备用快速便签</span>
                <ShortcutCaptureInput
                  value={alternateShortcut}
                  onChange={setAlternateShortcut}
                  placeholder="点击后按下快捷键，如 Ctrl+Alt+Q"
                />
              </div>
              <p className="text-xs leading-5 text-gray-400">点击输入框后按下按键组合，需要 Ctrl / Alt / Shift 参与。留空可关闭对应快捷键。</p>
              {shortcutError && <p className="rounded-lg bg-red-50 px-3 py-2 text-xs text-red-700 border border-red-100">{shortcutError}</p>}
              <button
                type="button"
                onClick={() => void saveShortcuts()}
                disabled={savingShortcuts}
                className="flex items-center gap-2 rounded-lg bg-emerald-600 px-4 py-2 text-sm font-medium text-white shadow-sm hover:bg-emerald-700 transition disabled:opacity-50"
              >
                <Save className="h-3.5 w-3.5" />
                {savingShortcuts ? "保存中" : "保存快捷键"}
              </button>
            </div>
          </section>

          {/* Actions Section */}
          <section className="rounded-xl bg-white border border-gray-100 shadow-sm overflow-hidden">
            <div className="p-4">
              {error && <p className="mb-3 rounded-lg bg-red-50 px-3 py-2 text-xs text-red-700 border border-red-100">{error}</p>}
              <div className="flex items-center gap-2">
                <button
                  type="button"
                  onClick={() => void save()}
                  disabled={saving}
                  className="flex items-center gap-2 rounded-lg bg-blue-600 px-4 py-2 text-sm font-medium text-white shadow-sm hover:bg-blue-700 transition disabled:opacity-50"
                >
                  <Save className="h-3.5 w-3.5" />
                  {saving ? "保存中" : "保存配置"}
                </button>
                <button
                  type="button"
                  onClick={() => void onSync()}
                  disabled={status === "syncing"}
                  className="flex items-center gap-2 rounded-lg border border-gray-200 bg-white px-4 py-2 text-sm font-medium text-gray-700 shadow-sm hover:bg-gray-50 transition disabled:opacity-40"
                >
                  <RefreshCw className={`h-3.5 w-3.5 ${status === "syncing" ? "animate-spin" : ""}`} />
                  {status === "syncing" ? "同步中" : "立即同步"}
                </button>
              </div>
            </div>
          </section>

        </div>
      </div>
    </div>
  );
}
