import { useCallback } from "react";
import { convertFileSrc, invoke } from "@/utils/tauri";
import type { Attachment, Note, NoteSummary, NoteVersion } from "@/types";
import type { Dispatch, MutableRefObject, SetStateAction } from "react";
import { getErrorMessage, persistDraftJournal, type DraftState } from "./draftJournal";

type Context = {
  activeNoteIdRef: MutableRefObject<string | null>;
  deletedNotes: NoteSummary[];
  draftsRef: MutableRefObject<Map<string, DraftState>>;
  lastDeletedIdRef: MutableRefObject<string | null>;
  setActiveNote: Dispatch<SetStateAction<Note | null>>;
  setDeletedNotes: Dispatch<SetStateAction<NoteSummary[]>>;
  setErrorMessage: Dispatch<SetStateAction<string | null>>;
  setVersions: Dispatch<SetStateAction<NoteVersion[]>>;
};

export function useArchiveNoteActions(context: Context, loadNotes: () => Promise<void>) {
  const { activeNoteIdRef, deletedNotes, draftsRef, lastDeletedIdRef, setActiveNote,
    setDeletedNotes, setErrorMessage, setVersions } = context;
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

  const purgeAllNotes = useCallback(async () => {
    try {
      for (const note of deletedNotes) {
        await invoke("purge_note", { id: note.id });
        draftsRef.current.delete(note.id);
      }
      persistDraftJournal(draftsRef.current);
      await invoke("cleanup_attachments");
      await loadDeletedNotes();
    } catch (err) {
      console.error("Failed to purge all notes:", err);
      setErrorMessage(getErrorMessage(err));
    }
  }, [deletedNotes, loadDeletedNotes]);

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
  return { clearVersions, deleteVersion, loadDeletedNotes, loadVersions, purgeAllNotes,
    purgeNote, resolveAttachment, restoreNote, restoreVersion, saveAttachment,
    toggleVersionPin, undoDelete };
}
