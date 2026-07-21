import { useEffect, useState } from "react";
import { X, RotateCcw, Pin, PinOff, Trash2, Eraser, Cloud, Server, Keyboard, Save, RefreshCw } from "lucide-react";
import type { ShortcutConfig, ShortcutConfigInput, SyncConfig, SyncConfigInput, SyncStatus, WebDavGcReport, WebDavStorageStatus } from "@/types";
import { ShortcutCaptureInput } from "./ShortcutCaptureInput";

type SyncSettingsPanelProps = {
  config: SyncConfig | null; status: SyncStatus; error: string | null;
  shortcutConfig: ShortcutConfig | null; shortcutError: string | null;
  onClose: () => void; onSave: (input: SyncConfigInput) => Promise<SyncConfig>;
  onSync: () => Promise<boolean>;
  onTestWebdav: (endpoint: string, username: string, password: string) => Promise<void>;
  onTestCloud: (cloudUrl: string, cloudEmail: string, cloudPassword: string) => Promise<void>;
  onGetWebdavStorageStatus: () => Promise<WebDavStorageStatus>;
  onRunWebdavGc: () => Promise<WebDavGcReport>;
  onSaveShortcuts: (input: ShortcutConfigInput) => Promise<ShortcutConfig>;
};

export function SyncSettingsPanel({
  config, status, error, shortcutConfig, shortcutError, onClose, onSave,
  onSync, onTestWebdav, onTestCloud, onGetWebdavStorageStatus, onRunWebdavGc, onSaveShortcuts,
}: SyncSettingsPanelProps) {
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
  const [savingWebdav, setSavingWebdav] = useState(false);
  const [savingCloud, setSavingCloud] = useState(false);
  const [webdavMsg, setWebdavMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [cloudMsg, setCloudMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [savingShortcuts, setSavingShortcuts] = useState(false);
  const [shortcutsMsg, setShortcutsMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [storage, setStorage] = useState<WebDavStorageStatus | null>(null);
  const [runningGc, setRunningGc] = useState(false);

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

  useEffect(() => {
    if (!config?.enabled) {
      setStorage(null);
      return;
    }
    let cancelled = false;
    void onGetWebdavStorageStatus()
      .then((value) => { if (!cancelled) setStorage(value); })
      .catch(() => undefined);
    return () => { cancelled = true; };
  }, [config?.enabled, config?.endpoint, onGetWebdavStorageStatus]);

  const runGc = async () => {
    setRunningGc(true);
    try {
      const report = await onRunWebdavGc();
      setStorage(report.status);
      setWebdavMsg({
        ok: true,
        text: report.deleted_objects > 0
          ? `已安全回收 ${report.deleted_objects} 个远端对象`
          : "扫描完成；新孤儿会进入7天安全观察期",
      });
    } catch (err) {
      setWebdavMsg({ ok: false, text: `垃圾回收失败：${err instanceof Error ? err.message : String(err)}` });
    } finally {
      setRunningGc(false);
    }
  };

  const saveWebdav = async () => {
    setSavingWebdav(true);
    setWebdavMsg(null);
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
      if (enabled) {
        try {
          await onTestWebdav(
            endpoint.trim().replace(/\/+$/, ""),
            username.trim(),
            password || "",
          );
          setWebdavMsg({ ok: true, text: "连接成功，配置已保存" });
        } catch (connErr) {
          setWebdavMsg({ ok: false, text: `配置已保存，但连接测试失败：${connErr instanceof Error ? connErr.message : String(connErr)}` });
        }
      } else {
        setWebdavMsg({ ok: true, text: "配置已保存" });
      }
      setPassword("");
    } catch {
      // Error captured by useSync
    } finally {
      setSavingWebdav(false);
    }
  };

  const saveCloud = async () => {
    setSavingCloud(true);
    setCloudMsg(null);
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
      if (cloudEnabled && cloudPassword) {
        try {
          await onTestCloud(cloudUrl.trim(), cloudEmail.trim(), cloudPassword);
          setCloudMsg({ ok: true, text: "登录成功，配置已保存" });
        } catch (connErr) {
          setCloudMsg({ ok: false, text: `配置已保存，但登录验证失败：${connErr instanceof Error ? connErr.message : String(connErr)}` });
        }
      } else {
        setCloudMsg({ ok: true, text: "云同步配置已保存" });
      }
      setCloudPassword("");
    } catch {
      // Error captured by useSync
    } finally {
      setSavingCloud(false);
    }
  };

  const saveShortcuts = async () => {
    setSavingShortcuts(true);
    setShortcutsMsg(null);
    try {
      await onSaveShortcuts({
        quick_note: quickNoteShortcut,
        clipboard_history: clipboardShortcut,
        quick_note_alternate: alternateShortcut,
      });
      setShortcutsMsg({ ok: true, text: "快捷键已保存" });
    } catch {
      setShortcutsMsg({ ok: false, text: "保存失败，请重试" });
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
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => void onSync()}
              disabled={status === "syncing"}
              className="flex items-center gap-1.5 rounded-lg border border-gray-200 bg-white px-3 py-1.5 text-xs font-medium text-gray-600 shadow-sm hover:bg-gray-50 transition disabled:opacity-40"
            >
              <RefreshCw className={`h-3 w-3 ${status === "syncing" ? "animate-spin" : ""}`} />
              {status === "syncing" ? "同步中" : "立即同步"}
            </button>
            <button type="button" onClick={onClose} className="h-7 w-7 rounded hover:bg-gray-100 flex items-center justify-center" title="关闭" aria-label="关闭">
              <X className="h-4 w-4 text-gray-500" />
            </button>
          </div>
        </div>

        <div className="flex flex-col gap-4 p-4">
          {error && <p className="rounded-lg bg-red-50 px-3 py-2 text-xs text-red-700 border border-red-100">{error}</p>}

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
              {webdavMsg && <p className={`rounded-lg px-3 py-2 text-xs border ${webdavMsg.ok ? "bg-green-50 text-green-700 border-green-100" : "bg-red-50 text-red-700 border-red-100"}`}>{webdavMsg.text}</p>}
              {storage && (
                <div className="rounded-lg border border-gray-100 bg-gray-50 px-3 py-2 text-xs leading-5 text-gray-500">
                  <div className="flex justify-between"><span>远端协议</span><span>v{storage.protocol_version} / epoch {storage.epoch}</span></div>
                  <div className="flex justify-between"><span>设备</span><span>{storage.devices}</span></div>
                  <div className="flex justify-between"><span>对象</span><span>{storage.reachable_objects} 有效 / {storage.stored_objects} 总计</span></div>
                  <div className="flex justify-between"><span>待回收</span><span>{storage.pending_gc_objects}</span></div>
                  <div className="flex justify-between"><span>占用</span><span>{formatBytes(storage.stored_bytes)}</span></div>
                </div>
              )}
              {config?.enabled && (
                <button type="button" onClick={() => void runGc()} disabled={runningGc} className="flex items-center gap-2 rounded-lg border border-gray-200 bg-white px-4 py-2 text-sm font-medium text-gray-600 hover:bg-gray-50 disabled:opacity-50">
                  <Eraser className="h-3.5 w-3.5" />
                  {runningGc ? "扫描中" : "安全回收远端垃圾"}
                </button>
              )}
              <button
                type="button"
                onClick={() => void saveWebdav()}
                disabled={savingWebdav}
                className="flex items-center gap-2 rounded-lg bg-blue-600 px-4 py-2 text-sm font-medium text-white shadow-sm hover:bg-blue-700 transition disabled:opacity-50"
              >
                <Save className="h-3.5 w-3.5" />
                {savingWebdav ? "保存中" : "保存并验证"}
              </button>
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
              {cloudMsg && <p className={`rounded-lg px-3 py-2 text-xs border ${cloudMsg.ok ? "bg-green-50 text-green-700 border-green-100" : "bg-red-50 text-red-700 border-red-100"}`}>{cloudMsg.text}</p>}
              <button
                type="button"
                onClick={() => void saveCloud()}
                disabled={savingCloud}
                className="flex items-center gap-2 rounded-lg bg-violet-600 px-4 py-2 text-sm font-medium text-white shadow-sm hover:bg-violet-700 transition disabled:opacity-50"
              >
                <Save className="h-3.5 w-3.5" />
                {savingCloud ? "保存中" : "保存云同步"}
              </button>
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
              {shortcutsMsg && <p className={`rounded-lg px-3 py-2 text-xs border ${shortcutsMsg.ok ? "bg-green-50 text-green-700 border-green-100" : "bg-red-50 text-red-700 border-red-100"}`}>{shortcutsMsg.text}</p>}
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

        </div>
      </div>
    </div>
  );
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MiB`;
}
