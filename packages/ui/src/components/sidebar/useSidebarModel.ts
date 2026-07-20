import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { SidebarProps } from "../Sidebar";

type DropPlacement = "before" | "after";
type ClipboardContextMenu = { itemId: string; x: number; y: number };

export function useSidebarModel(props: SidebarProps) {
  const { notes, clipboardItems, onReorderNotes, onSelectNote } = props;
  const [dragReadyId, setDragReadyId] = useState<string | null>(null);
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const [dragTarget, setDragTarget] = useState<{ id: string; placement: DropPlacement } | null>(null);
  const [dragPosition, setDragPosition] = useState<{ x: number; y: number } | null>(null);
  const [clipboardContextMenu, setClipboardContextMenu] = useState<ClipboardContextMenu | null>(null);
  const longPressTimerRef = useRef<number | null>(null);
  const pointerDragRef = useRef<{ noteId: string; pointerId: number } | null>(null);
  const dragTargetRef = useRef<{ id: string; placement: DropPlacement } | null>(null);
  const suppressClickRef = useRef(false);

  const closeMenus = useCallback(() => setClipboardContextMenu(null), []);

  useEffect(() => {
    if (!clipboardContextMenu) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") closeMenus();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [clipboardContextMenu, closeMenus]);

  useEffect(() => {
    return () => clearLongPressTimer(longPressTimerRef);
  }, []);

  const pinnedNotes = useMemo(() => notes.filter((note) => note.is_pinned), [notes]);
  const unpinnedNotes = useMemo(() => notes.filter((note) => !note.is_pinned), [notes]);
  const pinnedClipboardItems = useMemo(
    () => clipboardItems.filter((item) => item.is_pinned),
    [clipboardItems],
  );
  const recentClipboardItems = useMemo(
    () => clipboardItems.filter((item) => !item.is_pinned),
    [clipboardItems],
  );

  const startLongPress = (noteId: string, pointerId: number) => {
    clearLongPressTimer(longPressTimerRef);
    pointerDragRef.current = { noteId, pointerId };
    longPressTimerRef.current = window.setTimeout(() => {
      setDragReadyId(noteId);
      setDraggingId(noteId);
      suppressClickRef.current = true;
    }, 260);
  };

  const resetLongPress = () => {
    clearLongPressTimer(longPressTimerRef);
    pointerDragRef.current = null;
    dragTargetRef.current = null;
    setDragTarget(null);
    setDragPosition(null);
    if (!draggingId) setDragReadyId(null);
  };

  const handleDropNote = (
    sourceId: string,
    targetId: string,
    placement: DropPlacement,
    targetPinnedOverride: boolean | null = null,
  ) => {
    if (sourceId === targetId) return;
    const targetNote = notes.find((note) => note.id === targetId);
    const sourceNote = notes.find((note) => note.id === sourceId);
    if (!targetNote || !sourceNote) return;
    const targetPinned = targetPinnedOverride ?? targetNote.is_pinned;
    const group = targetPinned ? pinnedNotes : unpinnedNotes;
    const dragged = { ...sourceNote, is_pinned: targetPinned };

    const next = group.filter((note) => note.id !== sourceId);
    const targetIndex = next.findIndex((note) => note.id === targetId);
    const insertIndex = targetIndex + (placement === "after" ? 1 : 0);
    next.splice(Math.max(insertIndex, 0), 0, dragged);
    onReorderNotes(next.map((note) => note.id), targetPinned);
  };

  const finishPointerDrag = (targetId: string | null) => {
    const sourceId = pointerDragRef.current?.noteId;
    const targetGroup = dragPosition
      ? document.elementFromPoint(dragPosition.x, dragPosition.y)?.closest<HTMLElement>("[data-note-group]")
      : null;
    const groupPinned =
      targetGroup?.dataset.noteGroup === "pinned"
        ? true
        : targetGroup?.dataset.noteGroup === "all"
          ? false
          : null;
    const activeDragTarget = dragTargetRef.current;
    const fallbackTargetId =
      groupPinned === true
        ? pinnedNotes[pinnedNotes.length - 1]?.id ?? null
        : groupPinned === false
          ? unpinnedNotes[unpinnedNotes.length - 1]?.id ?? null
          : null;
    const effectiveTargetId = targetId ?? activeDragTarget?.id ?? fallbackTargetId;
    const placement = activeDragTarget?.id === effectiveTargetId ? activeDragTarget.placement : "before";
    pointerDragRef.current = null;
    clearLongPressTimer(longPressTimerRef);
    setDraggingId(null);
    setDragReadyId(null);
    dragTargetRef.current = null;
    setDragTarget(null);
    setDragPosition(null);
    if (sourceId && dragReadyId) {
      suppressClickRef.current = true;
      window.setTimeout(() => {
        suppressClickRef.current = false;
      }, 0);
    }
    if (sourceId && !effectiveTargetId && groupPinned !== null) {
      onReorderNotes([sourceId], groupPinned);
      return;
    }
    if (!sourceId || !effectiveTargetId) return;
    handleDropNote(sourceId, effectiveTargetId, placement, groupPinned);
  };

  const updateDragTarget = (clientX: number, clientY: number) => {
    const sourceId = pointerDragRef.current?.noteId;
    if (!sourceId || !dragReadyId) return;
    setDragPosition({ x: clientX, y: clientY });
    const target = document
      .elementFromPoint(clientX, clientY)
      ?.closest<HTMLElement>("[data-note-id]");
    const targetId = target?.dataset.noteId;
    if (!targetId || targetId === sourceId) {
      setDragTarget(null);
      dragTargetRef.current = null;
      return;
    }
    const sourceNote = notes.find((note) => note.id === sourceId);
    const targetNote = notes.find((note) => note.id === targetId);
    if (!sourceNote || !targetNote) {
      setDragTarget(null);
      dragTargetRef.current = null;
      return;
    }
    const rect = target.getBoundingClientRect();
    const nextTarget: { id: string; placement: DropPlacement } = {
      id: targetId,
      placement: clientY < rect.top + rect.height / 2 ? "before" : "after",
    };
    if (
      dragTargetRef.current?.id === nextTarget.id &&
      dragTargetRef.current?.placement === nextTarget.placement
    ) {
      return;
    }
    dragTargetRef.current = nextTarget;
    setDragTarget(nextTarget);
  };

  const selectNote = (id: string) => {
    if (suppressClickRef.current) return;
    onSelectNote(id);
  };

  const floatingNote = draggingId ? notes.find((note) => note.id === draggingId) : null;

  return {
    clipboardContextMenu, closeMenus, dragPosition, dragReadyId, draggingId,
    dragTarget, finishPointerDrag, floatingNote, pinnedClipboardItems, pinnedNotes,
    recentClipboardItems, resetLongPress, selectNote, setClipboardContextMenu,
    startLongPress, unpinnedNotes, updateDragTarget,
  };
}

function clearLongPressTimer(ref: React.MutableRefObject<number | null>): void {
  if (ref.current !== null) window.clearTimeout(ref.current);
  ref.current = null;
}
