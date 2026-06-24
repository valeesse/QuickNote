import { useState, useCallback, useEffect, useRef } from "react";
import { notesApi } from "@/api/client";
import type { Note, NoteSummary, NoteVersion, SaveStatus } from "@/types";

const SAVE_DELAY_MS = 800;
const DRAFT_KEY = "quicknote-web-drafts-v1";

export function useNotes() {
  const [notes, setNotes] = useState<NoteSummary[]>([]);
  const [activeNote, setActiveNote] = useState<Note | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [deletedNotes, setDeletedNotes] = useState<NoteSummary[]>([]);
  const [versions, setVersions] = useState<NoteVersion[]>([]);
  const [saveStatus, setSaveStatus] = useState<SaveStatus>("idle");
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const activeNoteIdRef = useRef<string | null>(null);
  const lastDeletedIdRef = useRef<string | null>(null);
  const pendingRef = useRef(new Map<string, string>());
  const timersRef = useRef(new Map<string, ReturnType<typeof setTimeout>>());
  const queuesRef = useRef(new Map<string, Promise<boolean>>());

  useEffect(() => { activeNoteIdRef.current = activeNote?.id ?? null; }, [activeNote?.id]);

  const loadNotes = useCallback(async () => {
    try {
      setIsLoading(true);
      setErrorMessage(null);
      setNotes(searchQuery.trim() ? await notesApi.search(searchQuery) : await notesApi.list());
    } catch (error) {
      setErrorMessage(messageOf(error));
    } finally {
      setIsLoading(false);
    }
  }, [searchQuery]);

  const flushSave = useCallback(async (id: string): Promise<boolean> => {
    const timer = timersRef.current.get(id);
    if (timer) clearTimeout(timer);
    timersRef.current.delete(id);
    const previous = queuesRef.current.get(id) ?? Promise.resolve(true);
    const task = previous.catch(() => false).then(async () => {
      const content = pendingRef.current.get(id);
      if (content === undefined) return true;
      if (activeNoteIdRef.current === id) setSaveStatus("saving");
      try {
        const updated = await notesApi.update(id, content);
        if (pendingRef.current.get(id) === content) pendingRef.current.delete(id);
        if (!pendingRef.current.has(id)) removeDraft(id);
        setNotes((current) => current.map((summary) => summary.id === id ? {
          ...summary,
          title: updated.title,
          preview: updated.content.slice(0, 200),
          updated_at: updated.updated_at,
        } : summary));
        if (activeNoteIdRef.current === id) {
          setActiveNote((current) => current?.id === id && !pendingRef.current.has(id) ? updated : current);
          setSaveStatus(pendingRef.current.has(id) ? "saving" : "saved");
        }
        setErrorMessage(null);
        return true;
      } catch (error) {
        setErrorMessage(messageOf(error));
        if (activeNoteIdRef.current === id) setSaveStatus("error");
        return false;
      }
    });
    queuesRef.current.set(id, task);
    const ok = await task;
    if (queuesRef.current.get(id) === task) queuesRef.current.delete(id);
    if (ok && pendingRef.current.has(id)) return flushSave(id);
    return ok;
  }, []);

  const createNote = useCallback(async (content = "") => {
    if (activeNoteIdRef.current && !(await flushSave(activeNoteIdRef.current))) return;
    try {
      const note = await notesApi.create(content);
      activeNoteIdRef.current = note.id;
      setActiveNote(note);
      await loadNotes();
      return note;
    } catch (error) { setErrorMessage(messageOf(error)); }
  }, [flushSave, loadNotes]);

  const reorderNotes = useCallback(async (orderedIds: string[], isPinned: boolean) => {
    const order = new Map(orderedIds.map((id, index) => [id, index]));
    setNotes((current) =>
      current.map((note) =>
        order.has(note.id) ? { ...note, is_pinned: isPinned } : note
      ).sort((a, b) => {
        const pinnedDelta = Number(b.is_pinned) - Number(a.is_pinned);
        if (pinnedDelta !== 0) return pinnedDelta;
        return (order.get(b.id) ?? Number.MAX_SAFE_INTEGER) - (order.get(a.id) ?? Number.MAX_SAFE_INTEGER);
      })
    );
    try {
      await notesApi.reorder(orderedIds, isPinned);
      await loadNotes();
    } catch (error) {
      setErrorMessage(messageOf(error));
      await loadNotes();
    }
  }, [loadNotes]);

  const selectNote = useCallback(async (id: string) => {
    if (id === activeNoteIdRef.current) return;
    if (activeNoteIdRef.current && !(await flushSave(activeNoteIdRef.current))) return;
    try {
      const note = await notesApi.get(id);
      const draft = readDrafts()[id];
      const displayed = draft === undefined ? note : { ...note, content: draft };
      activeNoteIdRef.current = note.id;
      setActiveNote(displayed);
      if (draft !== undefined) {
        pendingRef.current.set(id, draft);
        setSaveStatus("retrying");
        timersRef.current.set(id, setTimeout(() => { void flushSave(id); }, 0));
      } else {
        setSaveStatus("idle");
      }
    } catch (error) { setErrorMessage(messageOf(error)); }
  }, [flushSave]);

  const updateNote = useCallback((id: string, content: string) => {
    pendingRef.current.set(id, content);
    writeDraft(id, content);
    if (activeNoteIdRef.current === id) setSaveStatus("saving");
    const existing = timersRef.current.get(id);
    if (existing) clearTimeout(existing);
    timersRef.current.set(id, setTimeout(() => { void flushSave(id); }, SAVE_DELAY_MS));
  }, [flushSave]);

  const deleteNote = useCallback(async (id: string) => {
    if (!(await flushSave(id))) return false;
    try {
      await notesApi.delete(id);
      removeDraft(id);
      lastDeletedIdRef.current = id;
      if (activeNoteIdRef.current === id) { activeNoteIdRef.current = null; setActiveNote(null); }
      await loadNotes();
      return true;
    } catch (error) {
      setErrorMessage(messageOf(error));
      return false;
    }
  }, [flushSave, loadNotes]);

  const togglePin = useCallback(async (id: string) => {
    if (!(await flushSave(id))) return;
    try {
      await notesApi.togglePin(id);
      await loadNotes();
      if (activeNoteIdRef.current === id) setActiveNote(await notesApi.get(id));
    } catch (error) { setErrorMessage(messageOf(error)); }
  }, [flushSave, loadNotes]);

  const loadDeletedNotes = useCallback(async () => {
    try {
      const results = await notesApi.listDeleted();
      setDeletedNotes(results);
      return results;
    } catch (error) {
      setErrorMessage(messageOf(error));
      return [];
    }
  }, []);

  const restoreNote = useCallback(async (id: string) => {
    try {
      await notesApi.restore(id);
      await loadNotes();
      await loadDeletedNotes();
      const note = await notesApi.get(id);
      if (note) setActiveNote(note);
      return true;
    }
    catch (error) {
      setErrorMessage(messageOf(error));
      return false;
    }
  }, [loadDeletedNotes, loadNotes]);

  const undoDelete = useCallback(async () => {
    if (!lastDeletedIdRef.current) return;
    await restoreNote(lastDeletedIdRef.current);
    lastDeletedIdRef.current = null;
  }, [restoreNote]);

  const purgeNote = useCallback(async (id: string) => {
    try {
      await notesApi.purge(id);
      removeDraft(id);
      await loadDeletedNotes();
    } catch (error) {
      setErrorMessage(messageOf(error));
    }
  }, [loadDeletedNotes]);

  const purgeAllNotes = useCallback(async () => {
    try {
      for (const note of deletedNotes) {
        await notesApi.purge(note.id);
        removeDraft(note.id);
      }
      await loadDeletedNotes();
    } catch (error) {
      setErrorMessage(messageOf(error));
    }
  }, [deletedNotes, loadDeletedNotes]);

  const loadVersions = useCallback(async (id: string) => {
    try {
      const results = await notesApi.listVersions(id);
      setVersions(results);
      return results;
    } catch (error) {
      setErrorMessage(messageOf(error));
      return [];
    }
  }, []);

  const restoreVersion = useCallback(async (noteId: string, versionId: number) => {
    try {
      const restored = await notesApi.restoreVersion(noteId, versionId);
      removeDraft(noteId);
      activeNoteIdRef.current = noteId;
      setActiveNote(restored);
      await loadNotes();
      await loadVersions(noteId);
    } catch (error) {
      setErrorMessage(messageOf(error));
    }
  }, [loadNotes, loadVersions]);

  const toggleVersionPin = useCallback(async (noteId: string, versionId: number) => {
    try {
      await notesApi.toggleVersionPin(versionId);
      await loadVersions(noteId);
    } catch (error) {
      setErrorMessage(messageOf(error));
    }
  }, [loadVersions]);

  const deleteVersion = useCallback(async (noteId: string, versionId: number) => {
    try {
      await notesApi.deleteVersion(versionId);
      await loadVersions(noteId);
    } catch (error) {
      setErrorMessage(messageOf(error));
    }
  }, [loadVersions]);

  const clearVersions = useCallback(async (noteId: string) => {
    try {
      await notesApi.clearVersions(noteId);
      await loadVersions(noteId);
    } catch (error) {
      setErrorMessage(messageOf(error));
    }
  }, [loadVersions]);

  useEffect(() => {
    const timer = setTimeout(() => void loadNotes(), searchQuery.trim() ? 300 : 0);
    return () => clearTimeout(timer);
  }, [loadNotes, searchQuery]);

  useEffect(() => {
    const flushAll = () => { for (const id of pendingRef.current.keys()) void flushSave(id); };
    const onVisibility = () => { if (document.visibilityState === "hidden") flushAll(); };
    window.addEventListener("pagehide", flushAll);
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      window.removeEventListener("pagehide", flushAll);
      document.removeEventListener("visibilitychange", onVisibility);
      for (const timer of timersRef.current.values()) clearTimeout(timer);
    };
  }, [flushSave]);

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
    purgeAllNotes,
    loadVersions,
    restoreVersion,
    toggleVersionPin,
    deleteVersion,
    clearVersions,
  };
}

function messageOf(error: unknown): string { return error instanceof Error ? error.message : String(error); }

function readDrafts(): Record<string, string> {
  try { return JSON.parse(localStorage.getItem(DRAFT_KEY) || "{}"); }
  catch { return {}; }
}

function writeDraft(id: string, content: string): void {
  try {
    const drafts = readDrafts();
    drafts[id] = content;
    localStorage.setItem(DRAFT_KEY, JSON.stringify(drafts));
  } catch (error) { console.warn("Unable to persist draft", error); }
}

function removeDraft(id: string): void {
  const drafts = readDrafts();
  if (!(id in drafts)) return;
  delete drafts[id];
  try { localStorage.setItem(DRAFT_KEY, JSON.stringify(drafts)); }
  catch (error) { console.warn("Unable to remove persisted draft", error); }
}
