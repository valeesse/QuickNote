import { useCallback, useEffect, useRef, useState } from "react";
import { invoke, isTauri } from "@/utils/tauri";
import type { SyncConfig, SyncConfigInput, SyncReport, SyncStatus } from "@/types";

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
  const syncingRef = useRef<Promise<boolean> | null>(null);
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

  const loadConfig = useCallback(async () => {
    if (!isTauri()) return null;
    try {
      const next = await invoke<SyncConfig>("get_sync_config");
      configRef.current = next;
      setConfig(next);
      setStatus(next.enabled ? "idle" : "disabled");
      return next;
    } catch (err) {
      setError(getErrorMessage(err));
      setStatus("error");
      return null;
    }
  }, []);

  const syncNow = useCallback(async () => {
    if (!configRef.current?.enabled || syncingRef.current) {
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
        await onSyncedRef.current();
        return true;
      } catch (err) {
        setError(getErrorMessage(err));
        setStatus("error");
        return false;
      }
    })();

    syncingRef.current = task;
    try {
      return await task;
    } finally {
      syncingRef.current = null;
    }
  }, []);

  const saveConfig = useCallback(async (input: SyncConfigInput) => {
    const next = await invoke<SyncConfig>("set_sync_config", { config: input });
    configRef.current = next;
    setConfig(next);
    setStatus(next.enabled ? "idle" : "disabled");
    setError(null);
    return next;
  }, []);

  useEffect(() => {
    void loadConfig().then((next) => {
      if (next?.enabled) void syncNow();
    });
  }, [loadConfig, syncNow]);

  useEffect(() => {
    const timer = setInterval(() => void syncNow(), 60_000);
    const onFocus = () => void syncNow();
    window.addEventListener("focus", onFocus);
    return () => {
      clearInterval(timer);
      window.removeEventListener("focus", onFocus);
    };
  }, [syncNow]);

  return {
    config,
    status,
    error,
    lastReport,
    saveConfig,
    syncNow,
  };
}

function getErrorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}
