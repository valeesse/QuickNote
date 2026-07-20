import { useCallback, useEffect } from "react";
import { invoke } from "@/utils/tauri";
import type { Note, NoteSummary, SaveStatus } from "@/types";
import type { Dispatch, MutableRefObject, SetStateAction } from "react";
import { createSummaryFromContent, getErrorMessage, persistDraftJournal, type DraftState } from "./draftJournal";

type Context = {
  activeNoteIdRef: MutableRefObject<string | null>;
  draftRecoveryRef: MutableRefObject<Promise<void> | null>;
  draftsRef: MutableRefObject<Map<string, DraftState>>;
  flushAllDraftsRef: MutableRefObject<() => Promise<boolean>>;
  setActiveNote: Dispatch<SetStateAction<Note | null>>;
  setErrorMessage: Dispatch<SetStateAction<string | null>>;
  setNotes: Dispatch<SetStateAction<NoteSummary[]>>;
  setSaveStatus: Dispatch<SetStateAction<SaveStatus>>;
};

export function useDraftManager(context: Context, loadNotes: () => Promise<void>) {
  const { activeNoteIdRef, draftRecoveryRef, draftsRef, flushAllDraftsRef,
    setActiveNote, setErrorMessage, setNotes, setSaveStatus } = context;
  const saveWithRetry = useCallback(
    async (id: string, content: string, yjsState?: number[], attempts = 3): Promise<Note> => {
      let lastError: unknown;
      for (let attempt = 1; attempt <= attempts; attempt += 1) {
        try {
          if (attempt > 1 && activeNoteIdRef.current === id) setSaveStatus("retrying");
          const updated = await invoke<Note | null>("update_note", { id, content, yjsState });
          if (!updated) throw new Error("便签已不存在或已被删除");
          return updated;
        } catch (err) {
          lastError = err;
          if (attempt < attempts) {
            await new Promise((resolve) => setTimeout(resolve, attempt * 600));
          }
        }
      }
      throw lastError;
    },
    []
  );

  const flushDraft = useCallback(
    async (id: string): Promise<boolean> => {
      const draft = draftsRef.current.get(id);
      if (!draft) return true;

      if (draft.timer) {
        clearTimeout(draft.timer);
        draft.timer = null;
      }
      if (draft.retryTimer) {
        clearTimeout(draft.retryTimer);
        draft.retryTimer = null;
      }
      if (draft.inFlight) return draft.inFlight;

      const task = (async () => {
        while (draft.persistedRevision < draft.revision) {
          const targetRevision = draft.revision;
          const targetContent = draft.content;
          if (activeNoteIdRef.current === id) setSaveStatus("saving");

          try {
            const updated = await saveWithRetry(id, targetContent, draft.yjsState);
            draft.persistedRevision = targetRevision;
            setErrorMessage(null);

            if (draft.revision === targetRevision) {
              setNotes((current) =>
                current.map((summary) =>
                  summary.id === id
                    ? {
                        ...summary,
                        title: updated.title,
                        preview: createSummaryFromContent(updated.content).preview,
                        updated_at: updated.updated_at,
                      }
                    : summary
                )
              );

              if (activeNoteIdRef.current === id) {
                setActiveNote(updated);
                setSaveStatus("saved");
              }
            }
          } catch (err) {
            const message = getErrorMessage(err);
            setErrorMessage(message);
            if (activeNoteIdRef.current === id) setSaveStatus("error");
            draft.retryTimer = setTimeout(() => void flushDraft(id), 5_000);
            return false;
          }
        }

        return true;
      })();

      draft.inFlight = task;
      try {
        return await task;
      } finally {
        draft.inFlight = null;
        if (draft.persistedRevision >= draft.revision && !draft.retryTimer) {
          draftsRef.current.delete(id);
          try {
            persistDraftJournal(draftsRef.current);
          } catch (err) {
            console.error("Failed to clean persisted draft:", err);
          }
        }
      }
    },
    [saveWithRetry]
  );

  const flushAllDrafts = useCallback(async (): Promise<boolean> => {
    if (draftRecoveryRef.current) {
      try {
        await draftRecoveryRef.current;
      } catch {
        return false;
      }
    }
    const ids = Array.from(draftsRef.current.keys());
    const results = await Promise.all(ids.map((id) => flushDraft(id)));
    return results.every(Boolean);
  }, [flushDraft]);

  const refreshAfterSync = useCallback(async () => {
    await loadNotes();
    const activeId = activeNoteIdRef.current;
    if (!activeId) return;

    const draft = draftsRef.current.get(activeId);
    if (draft && draft.persistedRevision < draft.revision) return;

    const note = await invoke<Note | null>("get_note", { id: activeId });
    if (activeNoteIdRef.current !== activeId) return;
    setActiveNote(note);
    if (!note) activeNoteIdRef.current = null;
  }, [loadNotes]);

  useEffect(() => {
    flushAllDraftsRef.current = flushAllDrafts;
  }, [flushAllDrafts]);

  useEffect(() => {
    if (draftRecoveryRef.current || draftsRef.current.size === 0) return;
    const recovery = (async () => {
      for (const [id, draft] of Array.from(draftsRef.current.entries())) {
        const note = await invoke<Note | null>("get_note", { id });
        if (note) continue;
        await invoke<Note>("create_note", { content: draft.content });
        draftsRef.current.delete(id);
      }
      persistDraftJournal(draftsRef.current);
    })();
    draftRecoveryRef.current = recovery;
    void recovery
      .then(async () => {
        await flushAllDrafts();
        await loadNotes();
      })
      .catch((err) => setErrorMessage(`草稿恢复失败：${getErrorMessage(err)}`));
  }, [flushAllDrafts, loadNotes]);

  return { flushAllDrafts, flushDraft, refreshAfterSync };
}
