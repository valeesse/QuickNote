import { useState, useCallback, useEffect, useRef } from "react";
import { convertFileSrc, invoke } from "@/utils/tauri";
import type { Attachment, Note, NoteSummary, NoteVersion, SaveStatus } from "@/types";

export function useNotes() {
  const [notes, setNotes] = useState<NoteSummary[]>([]);
  const [activeNote, setActiveNote] = useState<Note | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [deletedNotes, setDeletedNotes] = useState<NoteSummary[]>([]);
  const [versions, setVersions] = useState<NoteVersion[]>([]);
  const [saveStatus, setSaveStatus] = useState<SaveStatus>("idle");
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const saveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const activeNoteIdRef = useRef<string | null>(null);
  const lastDeletedIdRef = useRef<string | null>(null);

  useEffect(() => {
    activeNoteIdRef.current = activeNote?.id ?? null;
  }, [activeNote?.id]);

  const loadNotes = useCallback(async () => {
    try {
      setIsLoading(true);
      setErrorMessage(null);
      if (searchQuery.trim()) {
        const results = await invoke<NoteSummary[]>("search_notes", {
          query: searchQuery,
        });
        setNotes(results);
      } else {
        const results = await invoke<NoteSummary[]>("list_notes");
        setNotes(results);
      }
    } catch (err) {
      console.error("Failed to load notes:", err);
      setErrorMessage(getErrorMessage(err));
    } finally {
      setIsLoading(false);
    }
  }, [searchQuery]);

  const createNote = useCallback(async () => {
    try {
      setErrorMessage(null);
      const note = await invoke<Note>("create_note", { content: "" });
      await loadNotes();
      // Load the full note to set as active
      const fullNote = await invoke<Note | null>("get_note", { id: note.id });
      if (fullNote) {
        setActiveNote(fullNote);
      }
      return note;
    } catch (err) {
      console.error("Failed to create note:", err);
      setErrorMessage(getErrorMessage(err));
    }
  }, [loadNotes]);

  const selectNote = useCallback(async (id: string) => {
    try {
      setErrorMessage(null);
      const note = await invoke<Note | null>("get_note", { id });
      if (note) {
        setActiveNote(note);
      }
    } catch (err) {
      console.error("Failed to load note:", err);
      setErrorMessage(getErrorMessage(err));
    }
  }, []);

  const saveWithRetry = useCallback(
    async (id: string, content: string, attempts = 3): Promise<Note | null> => {
      let lastError: unknown;
      for (let attempt = 1; attempt <= attempts; attempt += 1) {
        try {
          setSaveStatus(attempt === 1 ? "saving" : "retrying");
          const updated = await invoke<Note | null>("update_note", { id, content });
          setSaveStatus("saved");
          setErrorMessage(null);
          return updated;
        } catch (err) {
          lastError = err;
          if (attempt < attempts) {
            await new Promise((resolve) => setTimeout(resolve, attempt * 600));
          }
        }
      }

      setSaveStatus("error");
      setErrorMessage(getErrorMessage(lastError));
      return null;
    },
    []
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

      // Debounced save - wait 500ms after last keystroke
      if (saveTimerRef.current) {
        clearTimeout(saveTimerRef.current);
      }

      setSaveStatus("saving");
      saveTimerRef.current = setTimeout(async () => {
        const updated = await saveWithRetry(id, content);
        if (updated) {
          if (activeNoteIdRef.current === id) {
            setActiveNote(updated);
          }
          loadNotes();
        }
      }, 500);
    },
    [loadNotes, saveWithRetry]
  );

  const deleteNote = useCallback(
    async (id: string) => {
      try {
        setErrorMessage(null);
        await invoke("delete_note", { id });
        lastDeletedIdRef.current = id;
        if (activeNote?.id === id) {
          setActiveNote(null);
        }
        await loadNotes();
      } catch (err) {
        console.error("Failed to delete note:", err);
        setErrorMessage(getErrorMessage(err));
      }
    },
    [activeNote, loadNotes]
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

  const saveAttachment = useCallback(async (dataUrl: string, filename: string) => {
    const attachment = await invoke<Attachment>("save_attachment", { dataUrl, filename });
    return convertFileSrc(attachment.path);
  }, []);

  // Load notes on mount and when search changes
  useEffect(() => {
    loadNotes();
  }, [loadNotes]);

  useEffect(() => {
    return () => {
      if (saveTimerRef.current) {
        clearTimeout(saveTimerRef.current);
      }
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
    loadNotes,
    loadDeletedNotes,
    restoreNote,
    undoDelete,
    purgeNote,
    loadVersions,
    restoreVersion,
    toggleVersionPin,
    saveAttachment,
  };
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
