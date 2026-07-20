import React from "react";
import { Pin, PinOff, Tags, Trash2 } from "lucide-react";
import type { NoteSummary, TagSummary } from "@contracts";
import { formatRelativeTime } from "../../utils/format";
import { stripMarkdown } from "../../utils/html";

type DropPlacement = "before" | "after";

export function NoteTagSection({
  tags,
  selectedTag,
  onSelectTag,
}: {
  tags: TagSummary[];
  selectedTag: string | null;
  onSelectTag: (tag: string | null) => void;
}) {
  return (
    <section className="clipboard-sidebar__section">
      <h3 className="clipboard-sidebar__title flex items-center gap-1">
        <Tags className="h-3 w-3" />
        标签
      </h3>
      <button
        type="button"
        onClick={() => onSelectTag(null)}
        className={`clipboard-sidebar__item ${!selectedTag ? "note-sidebar__item--active" : ""}`}
      >
        <span className="clipboard-sidebar__item-title">
          <span>全部</span>
        </span>
      </button>
      {tags.slice(0, 12).map((tag) => (
        <button
          key={tag.id}
          type="button"
          onClick={() => onSelectTag(selectedTag === tag.normalized_name ? null : tag.normalized_name)}
          className={`clipboard-sidebar__item ${selectedTag === tag.normalized_name ? "note-sidebar__item--active" : ""}`}
          title={`#${tag.name}`}
        >
          <span className="clipboard-sidebar__item-title">
            <span className="truncate">#{tag.name}</span>
            <span className="text-[10px] font-normal text-gray-400">{tag.note_count}</span>
          </span>
        </button>
      ))}
    </section>
  );
}

export function NoteSidebarSection({
  title,
  group,
  children,
}: {
  title: string;
  group: "pinned" | "all";
  children: React.ReactNode;
}) {
  return (
    <section className="clipboard-sidebar__section" data-note-group={group}>
      <h3 className="clipboard-sidebar__title">{title}</h3>
      {children}
    </section>
  );
}

export function NoteSidebarItem({
  note,
  active,
  dragging,
  dragReady,
  dropPlacement,
  onSelect,
  onDelete,
  onTogglePin,
  onSelectTag,
  onPointerDown,
  onPointerUp,
  onPointerLeave,
  onPointerMove,
}: {
  note: NoteSummary;
  active: boolean;
  dragging: boolean;
  dragReady: boolean;
  dropPlacement: DropPlacement | null;
  onSelect: (id: string) => void;
  onDelete: (id: string) => void;
  onTogglePin: (id: string) => void;
  onSelectTag: (tag: string | null) => void;
  onPointerDown: (id: string, pointerId: number) => void;
  onPointerUp: (targetId: string | null) => void;
  onPointerLeave: () => void;
  onPointerMove: (clientX: number, clientY: number) => void;
}) {
  const noteTags = note.tags ?? [];
  return (
    <button
      type="button"
      onClick={() => {
        if (!dragReady && !dragging) onSelect(note.id);
      }}
      data-note-id={note.id}
      onPointerDown={(event) => {
        if (event.button !== 0) return;
        event.currentTarget.setPointerCapture(event.pointerId);
        onPointerDown(note.id, event.pointerId);
      }}
      onPointerUp={(event) => {
        const target = document
          .elementFromPoint(event.clientX, event.clientY)
          ?.closest<HTMLElement>("[data-note-id]");
        onPointerUp(target?.dataset.noteId ?? null);
      }}
      onPointerLeave={() => {
        if (!dragReady) onPointerLeave();
      }}
      onPointerMove={(event) => {
        onPointerMove(event.clientX, event.clientY);
      }}
      className={`note-sidebar__item ${active ? "note-sidebar__item--active" : ""} ${dragging ? "note-sidebar__item--dragging" : ""} ${dropPlacement ? `note-sidebar__item--drop-${dropPlacement}` : ""} ${dragReady ? "select-none" : ""}`}
      title={`${note.title || "无标题"}\n${stripMarkdown(note.preview) || "空便签"}`}
    >
      <span className="note-sidebar__item-title">
        <span className="min-w-0 truncate">{note.title || "无标题"}</span>
        <span className="note-sidebar__item-actions">
          <span
            role="button"
            tabIndex={0}
            className={`note-sidebar__icon-button ${note.is_pinned ? "text-amber-500" : "text-gray-400"}`}
            title={note.is_pinned ? "取消固定" : "固定"}
            aria-label={note.is_pinned ? "取消固定" : "固定"}
            onClick={(event) => {
              event.stopPropagation();
              onTogglePin(note.id);
            }}
            onPointerDown={(event) => event.stopPropagation()}
            onKeyDown={(event) => {
              if (event.key !== "Enter" && event.key !== " ") return;
              event.preventDefault();
              event.stopPropagation();
              onTogglePin(note.id);
            }}
          >
            {note.is_pinned ? <Pin className="h-3.5 w-3.5" /> : <PinOff className="h-3.5 w-3.5" />}
          </span>
          <span
            role="button"
            tabIndex={0}
            className="note-sidebar__icon-button text-gray-400 hover:bg-red-50 hover:text-red-500"
            title="删除"
            aria-label="删除便签"
            onClick={(event) => {
              event.stopPropagation();
              onDelete(note.id);
            }}
            onPointerDown={(event) => event.stopPropagation()}
            onKeyDown={(event) => {
              if (event.key !== "Enter" && event.key !== " ") return;
              event.preventDefault();
              event.stopPropagation();
              onDelete(note.id);
            }}
          >
            <Trash2 className="h-3.5 w-3.5" />
          </span>
        </span>
      </span>
      <span className="note-sidebar__item-text">{stripMarkdown(note.preview) || "空便签"}</span>
      {noteTags.length > 0 && (
        <span className="mt-2 flex flex-wrap gap-1">
          {noteTags.slice(0, 3).map((tag) => (
            <span
              key={tag}
              role="button"
              tabIndex={0}
              className="rounded bg-blue-50 px-1.5 py-0.5 text-[10px] font-medium text-blue-700"
              onClick={(event) => {
                event.stopPropagation();
                onSelectTag(tag.toLowerCase());
              }}
              onPointerDown={(event) => event.stopPropagation()}
              onKeyDown={(event) => {
                if (event.key !== "Enter" && event.key !== " ") return;
                event.preventDefault();
                event.stopPropagation();
                onSelectTag(tag.toLowerCase());
              }}
            >
              #{tag}
            </span>
          ))}
        </span>
      )}
      <span className="mt-2 block truncate text-[10px] text-gray-400">{formatRelativeTime(note.updated_at)}</span>
    </button>
  );
}
