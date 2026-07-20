import { useCallback } from "react";
import { invoke } from "@/utils/tauri";
import type { Note, NoteSummary, SaveStatus } from "@/types";
import type { Dispatch, MutableRefObject, SetStateAction } from "react";
import { createSummaryFromContent, getErrorMessage, persistDraftJournal, type DraftState } from "./draftJournal";

type Context = {
  activeNote: Note | null;
  activeNoteIdRef: MutableRefObject<string | null>;
  draftsRef: MutableRefObject<Map<string, DraftState>>;
  lastDeletedIdRef: MutableRefObject<string | null>;
  selectRequestRef: MutableRefObject<number>;
  setActiveNote: Dispatch<SetStateAction<Note | null>>;
  setErrorMessage: Dispatch<SetStateAction<string | null>>;
  setNotes: Dispatch<SetStateAction<NoteSummary[]>>;
  setSaveStatus: Dispatch<SetStateAction<SaveStatus>>;
};

type DraftActions = { flushAllDrafts: () => Promise<boolean>; flushDraft: (id: string) => Promise<boolean> };

export function useCoreNoteActions(context: Context, loadNotes: () => Promise<void>, drafts: DraftActions) {
  const { activeNote, activeNoteIdRef, draftsRef, lastDeletedIdRef, selectRequestRef,
    setActiveNote, setErrorMessage, setNotes, setSaveStatus } = context;
  const { flushAllDrafts, flushDraft } = drafts;
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
          return (order.get(b.id) ?? Number.MAX_SAFE_INTEGER) - (order.get(a.id) ?? Number.MAX_SAFE_INTEGER);
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
    async (id: string, content: string, yjsState?: number[]) => {
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
      draft.yjsState = yjsState;
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

  const updateNoteTags = useCallback(
    async (noteId: string, nextTags: string[]) => {
      try {
        setErrorMessage(null);
        const updated = await invoke<Note | null>("set_note_tags", { noteId, tags: nextTags });
        if (!updated) throw new Error("便签已不存在或已被删除");
        setActiveNote((current) => (current?.id === noteId ? updated : current));
        await loadNotes();
      } catch (err) {
        console.error("Failed to update note tags:", err);
        setErrorMessage(getErrorMessage(err));
      }
    },
    [loadNotes]
  );

  return { createNote, deleteNote, reorderNotes, selectNote, togglePin, updateNote, updateNoteTags };
}
