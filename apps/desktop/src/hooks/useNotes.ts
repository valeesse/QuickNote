import { useState, useCallback, useEffect, useRef } from "react";
import { convertFileSrc, invoke, isTauri } from "@/utils/tauri";
import type { Attachment, Note, NoteSummary, NoteVersion, SaveStatus } from "@/types";

interface DraftState {
  content: string;
  revision: number;
  persistedRevision: number;
  timer: ReturnType<typeof setTimeout> | null;
  retryTimer: ReturnType<typeof setTimeout> | null;
  inFlight: Promise<boolean> | null;
}

interface DraftJournal {
  schemaVersion: 1;
  drafts: Record<string, { content: string; updatedAt: string }>;
}

const DRAFT_JOURNAL_KEY = "quicknote-draft-journal-v1";

export function useNotes() {
  const [notes, setNotes] = useState<NoteSummary[]>([]);
  const [activeNote, setActiveNote] = useState<Note | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [deletedNotes, setDeletedNotes] = useState<NoteSummary[]>([]);
  const [versions, setVersions] = useState<NoteVersion[]>([]);
  const [saveStatus, setSaveStatus] = useState<SaveStatus>("idle");
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [initialDrafts] = useState(loadDraftJournal);
  const draftsRef = useRef<Map<string, DraftState>>(initialDrafts);
  const activeNoteIdRef = useRef<string | null>(null);
  const lastDeletedIdRef = useRef<string | null>(null);
  const loadRequestRef = useRef(0);
  const selectRequestRef = useRef(0);
  const flushAllDraftsRef = useRef<() => Promise<boolean>>(async () => true);
  const draftRecoveryRef = useRef<Promise<void> | null>(null);

  useEffect(() => {
    activeNoteIdRef.current = activeNote?.id ?? null;
  }, [activeNote?.id]);

  const loadNotes = useCallback(async () => {
    const requestId = ++loadRequestRef.current;
    try {
      setIsLoading(true);
      setErrorMessage(null);
      if (searchQuery.trim()) {
        const results = await invoke<NoteSummary[]>("search_notes", {
          query: searchQuery,
        });
        if (requestId === loadRequestRef.current) setNotes(results);
      } else {
        const results = await invoke<NoteSummary[]>("list_notes");
        if (requestId === loadRequestRef.current) setNotes(results);
      }
    } catch (err) {
      console.error("Failed to load notes:", err);
      setErrorMessage(getErrorMessage(err));
    } finally {
      if (requestId === loadRequestRef.current) setIsLoading(false);
    }
  }, [searchQuery]);

  const saveWithRetry = useCallback(
    async (id: string, content: string, attempts = 3): Promise<Note> => {
      let lastError: unknown;
      for (let attempt = 1; attempt <= attempts; attempt += 1) {
        try {
          if (attempt > 1 && activeNoteIdRef.current === id) setSaveStatus("retrying");
          const updated = await invoke<Note | null>("update_note", { id, content });
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
            const updated = await saveWithRetry(id, targetContent);
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

  const createNote = useCallback(async (content = "") => {
    try {
      if (!(await flushAllDrafts())) return;
      setErrorMessage(null);
      const note = await invoke<Note>("create_note", { content });
      activeNoteIdRef.current = note.id;
      setActiveNote(note);
      await loadNotes();
      return note;
    } catch (err) {
      console.error("Failed to create note:", err);
      setErrorMessage(getErrorMessage(err));
    }
  }, [flushAllDrafts, loadNotes]);

  const reorderNotes = useCallback(
    async (orderedIds: string[], isPinned: boolean) => {
      const order = new Map(orderedIds.map((id, index) => [id, index]));
      setNotes((current) =>
        current.map((note) =>
          order.has(note.id) ? { ...note, is_pinned: isPinned } : note
        ).sort((a, b) => {
          const pinnedDelta = Number(b.is_pinned) - Number(a.is_pinned);
          if (pinnedDelta !== 0) return pinnedDelta;
          return (order.get(a.id) ?? Number.MAX_SAFE_INTEGER) - (order.get(b.id) ?? Number.MAX_SAFE_INTEGER);
        })
      );

      try {
        await invoke("reorder_notes", { ids: orderedIds, isPinned });
        await loadNotes();
      } catch (err) {
        console.error("Failed to reorder notes:", err);
        setErrorMessage(getErrorMessage(err));
        await loadNotes();
      }
    },
    [loadNotes]
  );

  const selectNote = useCallback(
    async (id: string) => {
      const requestId = ++selectRequestRef.current;
      try {
        const currentId = activeNoteIdRef.current;
        if (currentId && currentId !== id && !(await flushDraft(currentId))) return;
        setErrorMessage(null);
        const note = await invoke<Note | null>("get_note", { id });
        if (note && requestId === selectRequestRef.current) {
          const draft = draftsRef.current.get(id);
          activeNoteIdRef.current = id;
          setActiveNote(draft ? { ...note, content: draft.content } : note);
        }
      } catch (err) {
        console.error("Failed to load note:", err);
        setErrorMessage(getErrorMessage(err));
      }
    },
    [flushDraft]
  );

  const updateNote = useCallback(
    async (id: string, content: string) => {
      const optimistic = createSummaryFromContent(content);
      const optimisticUpdatedAt = new Date().toISOString();
      setNotes((current) =>
        current.map((note) =>
          note.id === id
            ? {
                ...note,
                title: optimistic.title,
                preview: optimistic.preview,
                updated_at: optimisticUpdatedAt,
              }
            : note
        )
      );

      let draft = draftsRef.current.get(id);
      if (!draft) {
        draft = {
          content,
          revision: 0,
          persistedRevision: 0,
          timer: null,
          retryTimer: null,
          inFlight: null,
        };
        draftsRef.current.set(id, draft);
      }

      draft.content = content;
      draft.revision += 1;
      if (draft.timer) clearTimeout(draft.timer);
      if (draft.retryTimer) {
        clearTimeout(draft.retryTimer);
        draft.retryTimer = null;
      }
      try {
        persistDraftJournal(draftsRef.current);
      } catch (err) {
        setErrorMessage(`草稿保护写入失败：${getErrorMessage(err)}`);
      }
      if (activeNoteIdRef.current === id) setSaveStatus("saving");
      draft.timer = setTimeout(() => void flushDraft(id), 500);
    },
    [flushDraft]
  );

  const deleteNote = useCallback(
    async (id: string) => {
      try {
        setErrorMessage(null);
        if (!(await flushDraft(id))) return false;
        await invoke("delete_note", { id });
        lastDeletedIdRef.current = id;
        if (activeNote?.id === id) {
          setActiveNote(null);
        }
        await loadNotes();
        return true;
      } catch (err) {
        console.error("Failed to delete note:", err);
        setErrorMessage(getErrorMessage(err));
        return false;
      }
    },
    [activeNote, flushDraft, loadNotes]
  );

  const togglePin = useCallback(
    async (id: string) => {
      try {
        setErrorMessage(null);
        await invoke("toggle_pin", { id });
        await loadNotes();
        // If this is the active note, refresh it
        if (activeNote?.id === id) {
          const note = await invoke<Note | null>("get_note", { id });
          if (note) setActiveNote(note);
        }
      } catch (err) {
        console.error("Failed to toggle pin:", err);
        setErrorMessage(getErrorMessage(err));
      }
    },
    [activeNote, loadNotes]
  );

  const loadDeletedNotes = useCallback(async () => {
    try {
      const results = await invoke<NoteSummary[]>("list_deleted_notes");
      setDeletedNotes(results);
      return results;
    } catch (err) {
      console.error("Failed to load trash:", err);
      setErrorMessage(getErrorMessage(err));
      return [];
    }
  }, []);

  const restoreNote = useCallback(
    async (id: string) => {
      try {
        await invoke("restore_note", { id });
        await loadNotes();
        await loadDeletedNotes();
        const note = await invoke<Note | null>("get_note", { id });
        if (note) setActiveNote(note);
      } catch (err) {
        console.error("Failed to restore note:", err);
        setErrorMessage(getErrorMessage(err));
      }
    },
    [loadDeletedNotes, loadNotes]
  );

  const undoDelete = useCallback(async () => {
    if (!lastDeletedIdRef.current) return;
    await restoreNote(lastDeletedIdRef.current);
    lastDeletedIdRef.current = null;
  }, [restoreNote]);

  const purgeNote = useCallback(
    async (id: string) => {
      try {
        await invoke("purge_note", { id });
        draftsRef.current.delete(id);
        persistDraftJournal(draftsRef.current);
        await invoke("cleanup_attachments");
        await loadDeletedNotes();
      } catch (err) {
        console.error("Failed to purge note:", err);
        setErrorMessage(getErrorMessage(err));
      }
    },
    [loadDeletedNotes]
  );

  const loadVersions = useCallback(async (id: string) => {
    try {
      const results = await invoke<NoteVersion[]>("get_note_versions", { id });
      setVersions(results);
      return results;
    } catch (err) {
      console.error("Failed to load versions:", err);
      setErrorMessage(getErrorMessage(err));
      return [];
    }
  }, []);

  const restoreVersion = useCallback(
    async (noteId: string, versionId: number) => {
      try {
        const restored = await invoke<Note | null>("restore_note_version", {
          noteId,
          versionId,
        });
        if (restored) {
          draftsRef.current.delete(noteId);
          try {
            persistDraftJournal(draftsRef.current);
          } catch (err) {
            console.error("Failed to clean restored draft:", err);
          }
          activeNoteIdRef.current = noteId;
          setActiveNote(restored);
          await loadNotes();
          await loadVersions(noteId);
        }
      } catch (err) {
        console.error("Failed to restore version:", err);
        setErrorMessage(getErrorMessage(err));
      }
    },
    [loadNotes, loadVersions]
  );

  const toggleVersionPin = useCallback(
    async (noteId: string, versionId: number) => {
      try {
        await invoke("toggle_version_pin", { versionId });
        await loadVersions(noteId);
      } catch (err) {
        console.error("Failed to toggle version pin:", err);
        setErrorMessage(getErrorMessage(err));
      }
    },
    [loadVersions]
  );

  const deleteVersion = useCallback(
    async (noteId: string, versionId: number) => {
      try {
        await invoke("delete_note_version", { versionId });
        await loadVersions(noteId);
      } catch (err) {
        console.error("Failed to delete version:", err);
        setErrorMessage(getErrorMessage(err));
      }
    },
    [loadVersions]
  );

  const clearVersions = useCallback(
    async (noteId: string) => {
      try {
        await invoke("clear_note_versions", { noteId });
        await loadVersions(noteId);
      } catch (err) {
        console.error("Failed to clear versions:", err);
        setErrorMessage(getErrorMessage(err));
      }
    },
    [loadVersions]
  );

  const saveAttachment = useCallback(async (dataUrl: string, filename: string) => {
    const attachment = await invoke<Attachment>("save_attachment", { dataUrl, filename });
    return { ...attachment, path: convertFileSrc(attachment.path) };
  }, []);

  const resolveAttachment = useCallback(async (id: string) => {
    const attachment = await invoke<Attachment>("get_attachment", { id });
    return convertFileSrc(attachment.path);
  }, []);

  // Load notes on mount and after a short search debounce.
  useEffect(() => {
    const timer = setTimeout(() => void loadNotes(), searchQuery.trim() ? 200 : 0);
    return () => clearTimeout(timer);
  }, [loadNotes]);

  useEffect(() => {
    const flushWhenHidden = () => {
      if (document.visibilityState === "hidden") void flushAllDraftsRef.current();
    };
    const retryWhenOnline = () => void flushAllDraftsRef.current();
    document.addEventListener("visibilitychange", flushWhenHidden);
    window.addEventListener("online", retryWhenOnline);
    return () => {
      document.removeEventListener("visibilitychange", flushWhenHidden);
      window.removeEventListener("online", retryWhenOnline);
    };
  }, []);

  useEffect(() => {
    if (!isTauri() || !(window as any).__TAURI_INTERNALS__?.metadata?.currentWindow) return;

    let disposed = false;
    let unlisten: (() => void) | undefined;
    void import("@tauri-apps/api/window").then(async ({ getCurrentWindow }) => {
      if (disposed) return;
      const windowHandle = getCurrentWindow();
      unlisten = await windowHandle.onCloseRequested(async (event) => {
        event.preventDefault();
        const saved = await flushAllDraftsRef.current();
        if (saved) await windowHandle.destroy();
      });
    });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  return {
    notes,
    activeNote,
    isLoading,
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
    loadVersions,
    restoreVersion,
    toggleVersionPin,
    deleteVersion,
    clearVersions,
    saveAttachment,
    resolveAttachment,
    flushAllDrafts,
    refreshAfterSync,
  };
}

function loadDraftJournal(): Map<string, DraftState> {
  const drafts = new Map<string, DraftState>();
  if (typeof window === "undefined") return drafts;

  try {
    const raw = window.localStorage.getItem(DRAFT_JOURNAL_KEY);
    if (!raw) return drafts;
    const journal = JSON.parse(raw) as Partial<DraftJournal>;
    if (journal.schemaVersion !== 1 || !journal.drafts || typeof journal.drafts !== "object") {
      return drafts;
    }
    for (const [id, entry] of Object.entries(journal.drafts)) {
      if (!id || !entry || typeof entry.content !== "string") continue;
      drafts.set(id, {
        content: entry.content,
        revision: 1,
        persistedRevision: 0,
        timer: null,
        retryTimer: null,
        inFlight: null,
      });
    }
  } catch (err) {
    console.error("Failed to read draft journal:", err);
  }
  return drafts;
}

function persistDraftJournal(drafts: Map<string, DraftState>): void {
  if (typeof window === "undefined") return;
  const entries: DraftJournal["drafts"] = {};
  const updatedAt = new Date().toISOString();
  for (const [id, draft] of drafts) {
    if (draft.persistedRevision >= draft.revision) continue;
    entries[id] = { content: draft.content, updatedAt };
  }
  if (Object.keys(entries).length === 0) {
    window.localStorage.removeItem(DRAFT_JOURNAL_KEY);
    return;
  }
  const journal: DraftJournal = { schemaVersion: 1, drafts: entries };
  window.localStorage.setItem(DRAFT_JOURNAL_KEY, JSON.stringify(journal));
}

function getErrorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

function createSummaryFromContent(content: string): { title: string; preview: string } {
  const text = htmlToPlainText(content);
  const lines = text
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);

  return {
    title: lines[0]?.slice(0, 100) || "无标题",
    preview: lines.slice(1).join(" ").slice(0, 200),
  };
}

function htmlToPlainText(content: string): string {
  if (typeof DOMParser === "undefined") {
    return content.replace(/<[^>]+>/g, " ").replace(/\s+/g, " ").trim();
  }

  const doc = new DOMParser().parseFromString(`<main>${content}</main>`, "text/html");
  const blocks = Array.from(doc.querySelectorAll("p,h1,h2,h3,h4,h5,h6,li,blockquote"));
  const lines = blocks
    .map((node) => node.textContent?.replace(/\s+/g, " ").trim() ?? "")
    .filter(Boolean);

  if (lines.length > 0) {
    return lines.join("\n");
  }

  return doc.body.textContent?.replace(/\s+/g, " ").trim() ?? "";
}
