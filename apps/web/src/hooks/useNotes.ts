import { useState, useCallback, useEffect, useRef } from "react";
import { notesApi } from "@/api/client";
import type { Note, NoteSummary, NoteVersion, SaveStatus, TagSummary } from "@/types";

export function useNotes() {
  const [notes, setNotes] = useState<NoteSummary[]>([]);
  const [activeNote, setActiveNote] = useState<Note | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [deletedNotes, setDeletedNotes] = useState<NoteSummary[]>([]);
  const [tags, setTags] = useState<TagSummary[]>([]);
  const [selectedTag, setSelectedTagState] = useState<string | null>(null);
  const [versions, setVersions] = useState<NoteVersion[]>([]);
  const saveStatus: SaveStatus = "idle";
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [isLoadingMore, setIsLoadingMore] = useState(false);
  const activeNoteIdRef = useRef<string | null>(null);
  const lastDeletedIdRef = useRef<string | null>(null);
  const loadRequestRef = useRef(0);

  useEffect(() => { activeNoteIdRef.current = activeNote?.id ?? null; }, [activeNote?.id]);

  const setSelectedTag = useCallback((tag: string | null) => {
    setSearchQuery("");
    setNextCursor(null);
    setSelectedTagState(tag);
  }, []);

  const alignActiveNoteWithTag = useCallback(async (
    results: NoteSummary[],
    requestId: number,
  ) => {
    const currentId = activeNoteIdRef.current;
    if (currentId && results.some((note) => note.id === currentId)) return;

    const nextId = results[0]?.id;
    const note = nextId ? await notesApi.get(nextId) : null;
    if (requestId !== loadRequestRef.current) return;
    activeNoteIdRef.current = note?.id ?? null;
    setActiveNote(note);
  }, []);

  const loadNotes = useCallback(async () => {
    const requestId = ++loadRequestRef.current;
    try {
      setIsLoading(true);
      setErrorMessage(null);
      if (searchQuery.trim()) {
        const results = await notesApi.search(searchQuery);
        if (requestId === loadRequestRef.current) {
          setNotes(results);
          setNextCursor(null);
        }
      } else {
        const page = await notesApi.page(null, selectedTag);
        if (requestId === loadRequestRef.current) {
          setNotes(page.items);
          setNextCursor(page.next_cursor);
          if (selectedTag) await alignActiveNoteWithTag(page.items, requestId);
        }
      }
      const tagResults = await notesApi.tags();
      if (requestId === loadRequestRef.current) setTags(tagResults);
    } catch (error) {
      if (requestId === loadRequestRef.current) setErrorMessage(messageOf(error));
    } finally {
      if (requestId === loadRequestRef.current) setIsLoading(false);
    }
  }, [alignActiveNoteWithTag, searchQuery, selectedTag]);

  const loadMoreNotes = useCallback(async () => {
    if (!nextCursor || searchQuery.trim() || isLoadingMore) return;
    setIsLoadingMore(true);
    try {
      const page = await notesApi.page(nextCursor, selectedTag);
      setNotes((current) => {
        const known = new Set(current.map((note) => note.id));
        return [...current, ...page.items.filter((note) => !known.has(note.id))];
      });
      setNextCursor(page.next_cursor);
    } catch (error) {
      setErrorMessage(messageOf(error));
    } finally {
      setIsLoadingMore(false);
    }
  }, [isLoadingMore, nextCursor, searchQuery, selectedTag]);

  const createNote = useCallback(async (content = "") => {
    try {
      const note = await notesApi.create(content);
      activeNoteIdRef.current = note.id;
      setActiveNote(note);
      await loadNotes();
      return note;
    } catch (error) { setErrorMessage(messageOf(error)); }
  }, [loadNotes]);

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
    try {
      const note = await notesApi.get(id);
      activeNoteIdRef.current = note.id;
      setActiveNote(note);
    } catch (error) { setErrorMessage(messageOf(error)); }
  }, []);

  const updateNote = useCallback((_id: string, _content: string) => {}, []);

  const deleteNote = useCallback(async (id: string) => {
    try {
      await notesApi.delete(id);
      lastDeletedIdRef.current = id;
      if (activeNoteIdRef.current === id) { activeNoteIdRef.current = null; setActiveNote(null); }
      await loadNotes();
      return true;
    } catch (error) {
      setErrorMessage(messageOf(error));
      return false;
    }
  }, [loadNotes]);

  const togglePin = useCallback(async (id: string) => {
    try {
      await notesApi.togglePin(id);
      await loadNotes();
      if (activeNoteIdRef.current === id) setActiveNote(await notesApi.get(id));
    } catch (error) { setErrorMessage(messageOf(error)); }
  }, [loadNotes]);

  const updateNoteTags = useCallback(async (noteId: string, nextTags: string[]) => {
    try {
      const updated = await notesApi.setTags(noteId, nextTags);
      setActiveNote((current) => current?.id === noteId ? updated : current);
      await loadNotes();
    } catch (error) {
      setErrorMessage(messageOf(error));
    }
  }, [loadNotes]);

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
      await loadDeletedNotes();
    } catch (error) {
      setErrorMessage(messageOf(error));
    }
  }, [loadDeletedNotes]);

  const purgeAllNotes = useCallback(async () => {
    try {
      for (const note of deletedNotes) {
        await notesApi.purge(note.id);
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
    loadMoreNotes,
    hasMoreNotes: nextCursor !== null,
    isLoadingMoreNotes: isLoadingMore,
  };
}
function messageOf(error: unknown): string { return error instanceof Error ? error.message : String(error); }
