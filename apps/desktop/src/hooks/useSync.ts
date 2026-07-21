import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke, isTauri } from "@/utils/tauri";
import type { SyncConfig, SyncConfigInput, SyncReport, SyncStatus, WebDavGcReport, WebDavStorageStatus } from "@/types";

export function useSync({
  beforeSync,
  onSynced,
}: {
  beforeSync: () => Promise<boolean>;
  onSynced: () => Promise<void>;
}) {
  const [config, setConfig] = useState<SyncConfig | null>(null);
  const [status, setStatus] = useState<SyncStatus>("disabled");
  const [error, setError] = useState<string | null>(null);
  const [lastReport, setLastReport] = useState<SyncReport | null>(null);
  const [pendingCount, setPendingCount] = useState(0);
  const [lastSuccessAt, setLastSuccessAt] = useState<number | null>(() => {
    const value = localStorage.getItem("quicknote-last-sync-success-v1");
    return value ? Number(value) : null;
  });
  const syncingRef = useRef<Promise<boolean> | null>(null);
  const retryTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const failureCountRef = useRef(0);
  const syncIfNeededRef = useRef<() => Promise<void>>(async () => {});
  const configRef = useRef<SyncConfig | null>(null);
  const beforeSyncRef = useRef(beforeSync);
  const onSyncedRef = useRef(onSynced);

  useEffect(() => {
    configRef.current = config;
  }, [config]);

  useEffect(() => {
    beforeSyncRef.current = beforeSync;
    onSyncedRef.current = onSynced;
  }, [beforeSync, onSynced]);

  const refreshPendingCount = useCallback(async () => {
    if (!isTauri()) return 0;
    try {
      const count = await invoke<number>("pending_sync_change_count");
      setPendingCount(count);
      return count;
    } catch {
      return 0;
    }
  }, []);

  const scheduleRetry = useCallback((reason: unknown) => {
    setError(getErrorMessage(reason));
    failureCountRef.current += 1;
    if (!navigator.onLine) {
      setStatus("waiting");
      return;
    }
    setStatus("retrying");
    const delays = [5_000, 15_000, 30_000, 60_000, 300_000];
    const base = delays[Math.min(failureCountRef.current - 1, delays.length - 1)];
    const jitter = Math.floor(Math.random() * Math.min(2_000, base / 4));
    if (retryTimerRef.current) clearTimeout(retryTimerRef.current);
    retryTimerRef.current = setTimeout(
      () => void syncIfNeededRef.current(),
      base + jitter,
    );
  }, []);

  const loadConfig = useCallback(async () => {
    if (!isTauri()) return null;
    try {
      const next = await invoke<SyncConfig>("get_sync_config");
      configRef.current = next;
      setConfig(next);
      setStatus(next.enabled || next.cloud_enabled ? "idle" : "disabled");
      return next;
    } catch (err) {
      setError(getErrorMessage(err));
      setStatus("error");
      return null;
    }
  }, []);

  const syncNow = useCallback(async () => {
    const cfg = configRef.current;
    if ((!cfg?.enabled && !cfg?.cloud_enabled) || syncingRef.current) {
      return syncingRef.current ?? false;
    }

    const task = (async () => {
      if (!(await beforeSyncRef.current())) return false;
      setStatus("syncing");
      setError(null);
      try {
        const report = await invoke<SyncReport>("sync_now");
        setLastReport(report);
        setStatus("synced");
        failureCountRef.current = 0;
        if (retryTimerRef.current) clearTimeout(retryTimerRef.current);
        retryTimerRef.current = null;
        const completedAt = Date.now();
        localStorage.setItem("quicknote-last-sync-success-v1", String(completedAt));
        setLastSuccessAt(completedAt);
        await refreshPendingCount();
        await onSyncedRef.current();
        return true;
      } catch (err) {
        await refreshPendingCount();
        // A timed-out round may already have committed remote generations locally.
        await onSyncedRef.current().catch(() => {});
        scheduleRetry(err);
        return false;
      }
    })();

    syncingRef.current = task;
    try {
      return await task;
    } finally {
      syncingRef.current = null;
    }
  }, [refreshPendingCount, scheduleRetry]);

  const syncIfNeeded = useCallback(async () => {
    const cfg = configRef.current;
    if ((!cfg?.enabled && !cfg?.cloud_enabled) || syncingRef.current) return;
    if (!navigator.onLine) {
      setStatus("waiting");
      await refreshPendingCount();
      return;
    }
    try {
      const hasChanges = await invoke<boolean>("has_sync_changes");
      if (hasChanges) await syncNow();
      else {
        failureCountRef.current = 0;
        if (retryTimerRef.current) clearTimeout(retryTimerRef.current);
        retryTimerRef.current = null;
        setStatus((current) => current === "error" || current === "retrying" ? "idle" : current);
      }
    } catch (err) {
      scheduleRetry(err);
    }
  }, [refreshPendingCount, scheduleRetry, syncNow]);

  useEffect(() => {
    syncIfNeededRef.current = syncIfNeeded;
  }, [syncIfNeeded]);

  const testWebdav = useCallback(async (endpoint: string, username: string, password: string) => {
    await invoke<void>("test_webdav_connection", { endpoint, username, password });
  }, []);

  const testCloud = useCallback(async (cloudUrl: string, cloudEmail: string, cloudPassword: string) => {
    await invoke<void>("test_cloud_connection", { cloudUrl, cloudEmail, cloudPassword });
  }, []);

  const getWebdavStorageStatus = useCallback(async () =>
    invoke<WebDavStorageStatus>("get_webdav_storage_status"), []);

  const runWebdavGc = useCallback(async () =>
    invoke<WebDavGcReport>("run_webdav_gc"), []);

  const saveConfig = useCallback(async (input: SyncConfigInput) => {
    try {
      const next = await invoke<SyncConfig>("set_sync_config", { config: input });
      configRef.current = next;
      setConfig(next);
      setStatus(next.enabled || next.cloud_enabled ? "idle" : "disabled");
      setError(null);
      return next;
    } catch (err) {
      setError(getErrorMessage(err));
      setStatus("error");
      throw err;
    }
  }, []);

  useEffect(() => {
    void loadConfig().then((next) => {
      if (next?.enabled || next?.cloud_enabled) void syncNow();
    });
  }, [loadConfig, syncNow]);

  useEffect(() => {
    let pollTimer: ReturnType<typeof setTimeout> | null = null;
    let stopped = false;
    const poll = async () => {
      await syncIfNeededRef.current();
      if (stopped) return;
      const multiplier = 2 ** Math.min(failureCountRef.current, 3);
      pollTimer = setTimeout(poll, 60_000 * multiplier);
    };
    pollTimer = setTimeout(poll, 60_000);
    const onFocus = () => void syncIfNeeded();
    const onOnline = () => void syncIfNeeded();
    let requestedSyncTimer: ReturnType<typeof setTimeout> | null = null;
    let firstRequestedAt: number | null = null;
    const onSyncNeeded = () => {
      const cfg = configRef.current;
      if (!cfg?.enabled && !cfg?.cloud_enabled) return;
      if (firstRequestedAt === null) firstRequestedAt = Date.now();
      if (requestedSyncTimer) clearTimeout(requestedSyncTimer);
      setStatus("waiting");
      void refreshPendingCount();
      const remainingMaxDelay = Math.max(0, 30_000 - (Date.now() - firstRequestedAt));
      requestedSyncTimer = setTimeout(() => {
        firstRequestedAt = null;
        void syncIfNeeded();
      }, Math.min(4_000, remainingMaxDelay));
    };
    window.addEventListener("focus", onFocus);
    window.addEventListener("online", onOnline);
    window.addEventListener("quicknote:sync-needed", onSyncNeeded);
    return () => {
      stopped = true;
      if (pollTimer) clearTimeout(pollTimer);
      if (retryTimerRef.current) clearTimeout(retryTimerRef.current);
      if (requestedSyncTimer) clearTimeout(requestedSyncTimer);
      window.removeEventListener("focus", onFocus);
      window.removeEventListener("online", onOnline);
      window.removeEventListener("quicknote:sync-needed", onSyncNeeded);
    };
  }, [refreshPendingCount, syncIfNeeded]);

  const statusLabel = useMemo(() => {
    if (status === "disabled") return "同步未启用";
    if (status === "syncing") return pendingCount > 0 ? `正在同步 ${pendingCount} 项` : "正在检查远端变更";
    if (status === "waiting") return pendingCount > 0 ? `等待同步（${pendingCount} 项）` : "等待网络恢复";
    if (status === "retrying") return pendingCount > 0 ? `网络不稳定，${pendingCount} 项将在稍后重试` : "网络不稳定，稍后自动重试";
    if (status === "error") return error ? `同步失败：${error}` : "同步失败";
    if (lastSuccessAt) return `上次成功同步：${formatRelativeTime(lastSuccessAt)}`;
    return status === "synced" ? "同步完成" : "立即同步";
  }, [error, lastSuccessAt, pendingCount, status]);

  return {
    config,
    status,
    error,
    lastReport,
    pendingCount,
    statusLabel,
    saveConfig,
    testWebdav,
    testCloud,
    getWebdavStorageStatus,
    runWebdavGc,
    syncNow,
  };
}

function formatRelativeTime(timestamp: number): string {
  const seconds = Math.max(0, Math.floor((Date.now() - timestamp) / 1_000));
  if (seconds < 60) return "刚刚";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes} 分钟前`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} 小时前`;
  return new Date(timestamp).toLocaleString();
}

function getErrorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}
