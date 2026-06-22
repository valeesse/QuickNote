import { useState, useCallback, useEffect, useRef } from "react";
import { notesApi } from "@/api/client";
import type { Note, NoteSummary, SaveStatus } from "@/types";

export function useNotes() {
  const [notes, setNotes] = useState<NoteSummary[]>([]);
  const [activeNote, setActiveNote] = useState<Note | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [saveStatus, setSaveStatus] = useState<SaveStatus>("idle");
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const activeNoteIdRef = useRef<string | null>(null);
  const saveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pendingContentRef = useRef<string | null>(null);

  useEffect(() => {
    activeNoteIdRef.current = activeNote?.id ?? null;
  }, [activeNote?.id]);

  const loadNotes = useCallback(async () => {
    try {
      setIsLoading(true);
      setErrorMessage(null);
      if (searchQuery.trim()) {
        const results = await notesApi.search(searchQuery);
        setNotes(results);
      } else {
        const results = await notesApi.list();
        setNotes(results);
      }
    } catch (err) {
      setErrorMessage(err instanceof Error ? err.message : String(err));
    } finally {
      setIsLoading(false);
    }
  }, [searchQuery]);

  const createNote = useCallback(async () => {
    try {
      setErrorMessage(null);
      const note = await notesApi.create("");
      activeNoteIdRef.current = note.id;
      setActiveNote(note);
      await loadNotes();
      return note;
    } catch (err) {
      setErrorMessage(err instanceof Error ? err.message : String(err));
    }
  }, [loadNotes]);

  const selectNote = useCallback(async (id: string) => {
    try {
      setErrorMessage(null);
      const note = await notesApi.get(id);
      if (note) {
        activeNoteIdRef.current = note.id;
        setActiveNote(note);
      }
    } catch (err) {
      setErrorMessage(err instanceof Error ? err.message : String(err));
    }
  }, []);

  const flushSave = useCallback(
    async (id: string, content: string) => {
      try {
        if (activeNoteIdRef.current === id) setSaveStatus("saving");
        const updated = await notesApi.update(id, content);
        setNotes((current) =>
          current.map((s) =>
            s.id === id
              ? { ...s, title: updated.title, preview: updated.content.slice(0, 200), updated_at: updated.updated_at }
              : s,
          ),
        );
        if (activeNoteIdRef.current === id) {
          setActiveNote(updated);
          setSaveStatus("saved");
        }
        pendingContentRef.current = null;
      } catch (err) {
        setErrorMessage(err instanceof Error ? err.message : String(err));
        if (activeNoteIdRef.current === id) setSaveStatus("error");
      }
    },
    [],
  );

  const updateNote = useCallback(
    (id: string, content: string) => {
      setNotes((current) =>
        current.map((s) =>
          s.id === id
            ? { ...s, updated_at: new Date().toISOString() }
            : s,
        ),
      );

      pendingContentRef.current = content;
      if (saveTimerRef.current) clearTimeout(saveTimerRef.current);
      if (activeNoteIdRef.current === id) setSaveStatus("saving");
      saveTimerRef.current = setTimeout(() => {
        if (pendingContentRef.current !== null) {
          void flushSave(id, pendingContentRef.current);
        }
      }, 800);
    },
    [flushSave],
  );

  const deleteNote = useCallback(
    async (id: string) => {
      try {
        setErrorMessage(null);
        await notesApi.delete(id);
        if (activeNote?.id === id) setActiveNote(null);
        await loadNotes();
      } catch (err) {
        setErrorMessage(err instanceof Error ? err.message : String(err));
      }
    },
    [activeNote, loadNotes],
  );

  const togglePin = useCallback(
    async (id: string) => {
      try {
        setErrorMessage(null);
        await notesApi.togglePin(id);
        await loadNotes();
        if (activeNote?.id === id) {
          const note = await notesApi.get(id);
          if (note) setActiveNote(note);
        }
      } catch (err) {
        setErrorMessage(err instanceof Error ? err.message : String(err));
      }
    },
    [activeNote, loadNotes],
  );

  const restoreNote = useCallback(
    async (id: string) => {
      try {
        await notesApi.restore(id);
        await loadNotes();
      } catch (err) {
        setErrorMessage(err instanceof Error ? err.message : String(err));
      }
    },
    [loadNotes],
  );

  useEffect(() => {
    const timer = setTimeout(() => void loadNotes(), searchQuery.trim() ? 300 : 0);
    return () => clearTimeout(timer);
  }, [loadNotes]);

  useEffect(() => {
    const handler = () => {
      if (document.visibilityState === "hidden" && pendingContentRef.current !== null && activeNoteIdRef.current) {
        void flushSave(activeNoteIdRef.current, pendingContentRef.current);
      }
    };
    document.addEventListener("visibilitychange", handler);
    return () => document.removeEventListener("visibilitychange", handler);
  }, [flushSave]);

  return {
    notes,
    activeNote,
    isLoading,
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
    restoreNote,
  };
}
