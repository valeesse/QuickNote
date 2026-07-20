import { useState, useCallback, useEffect, useRef } from "react";
import { invoke, isTauri } from "@/utils/tauri";
import type { Note, NoteSummary, NoteVersion, SaveStatus, TagSummary } from "@/types";

import { loadDraftJournal, getErrorMessage, type DraftState } from "./draftJournal";
import { useDraftManager } from "./useDraftManager";
import { useCoreNoteActions } from "./useCoreNoteActions";
import { useArchiveNoteActions } from "./useArchiveNoteActions";

export function useNotes() {
  const [notes, setNotes] = useState<NoteSummary[]>([]);
  const [activeNote, setActiveNote] = useState<Note | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [deletedNotes, setDeletedNotes] = useState<NoteSummary[]>([]);
  const [tags, setTags] = useState<TagSummary[]>([]);
  const [selectedTag, setSelectedTagState] = useState<string | null>(null);
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

  const setSelectedTag = useCallback((tag: string | null) => {
    setSearchQuery("");
    setSelectedTagState(tag);
  }, []);

  const alignActiveNoteWithTag = useCallback(async (
    results: NoteSummary[],
    requestId: number,
  ) => {
    const currentId = activeNoteIdRef.current;
    if (currentId && results.some((note) => note.id === currentId)) return;
    if (!(await flushAllDraftsRef.current()) || requestId !== loadRequestRef.current) return;

    const nextId = results[0]?.id;
    if (!nextId) {
      activeNoteIdRef.current = null;
      setActiveNote(null);
      setSaveStatus("idle");
      return;
    }

    const selectRequestId = ++selectRequestRef.current;
    const note = await invoke<Note | null>("get_note", { id: nextId });
    if (
      requestId !== loadRequestRef.current ||
      selectRequestId !== selectRequestRef.current
    ) return;
    activeNoteIdRef.current = note?.id ?? null;
    setActiveNote(note);
    setSaveStatus("idle");
  }, []);

  const loadNotes = useCallback(async () => {
    const requestId = ++loadRequestRef.current;
    try {
      setIsLoading(true);
      setErrorMessage(null);
      if (selectedTag) {
        const results = await invoke<NoteSummary[]>("list_notes_by_tag", {
          tag: selectedTag,
        });
        if (requestId === loadRequestRef.current) {
          setNotes(results);
          await alignActiveNoteWithTag(results, requestId);
        }
      } else if (searchQuery.trim()) {
        const results = await invoke<NoteSummary[]>("search_notes", {
          query: searchQuery,
        });
        if (requestId === loadRequestRef.current) setNotes(results);
      } else {
        const results = await invoke<NoteSummary[]>("list_notes");
        if (requestId === loadRequestRef.current) setNotes(results);
      }
      const tagResults = await invoke<TagSummary[]>("list_tags");
      if (requestId === loadRequestRef.current) setTags(tagResults);
    } catch (err) {
      console.error("Failed to load notes:", err);
      setErrorMessage(getErrorMessage(err));
    } finally {
      if (requestId === loadRequestRef.current) setIsLoading(false);
    }
  }, [alignActiveNoteWithTag, searchQuery, selectedTag]);

  const draftActions = useDraftManager({ activeNoteIdRef, draftRecoveryRef, draftsRef,
    flushAllDraftsRef, setActiveNote, setErrorMessage, setNotes, setSaveStatus }, loadNotes);
  const { flushAllDrafts, refreshAfterSync } = draftActions;
  const coreActions = useCoreNoteActions({ activeNote, activeNoteIdRef, draftsRef,
    lastDeletedIdRef, selectRequestRef, setActiveNote, setErrorMessage, setNotes,
    setSaveStatus }, loadNotes, draftActions);
  const archiveActions = useArchiveNoteActions({ activeNoteIdRef, deletedNotes, draftsRef,
    lastDeletedIdRef, setActiveNote, setDeletedNotes, setErrorMessage, setVersions }, loadNotes);
  const { createNote, deleteNote, reorderNotes, selectNote, togglePin, updateNote,
    updateNoteTags } = coreActions;
  const { clearVersions, deleteVersion, loadDeletedNotes, loadVersions, purgeAllNotes,
    purgeNote, resolveAttachment, restoreNote, restoreVersion, saveAttachment,
    toggleVersionPin, undoDelete } = archiveActions;
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
    tags,
    selectedTag,
    setSelectedTag,
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
    updateNoteTags,
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
  };
}
