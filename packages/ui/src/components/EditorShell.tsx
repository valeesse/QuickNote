import { Search } from "lucide-react";
import type { ReactNode } from "react";
import { useMemo, useState } from "react";
import type { Note, SaveStatus, TagSummary } from "@contracts";
import { formatSaveStatus } from "../utils/format";
import { FindReplacePanel } from "./FindReplacePanel";
import { Toolbar, ToolbarButton } from "./Toolbar";
import type { FindReplaceControls } from "../hooks/useFindReplace";

export function EditorShell({
  editor,
  note,
  saveStatus,
  errorMessage,
  isSyncing,
  onInsertImage,
  findReplace,
  onOpenHistory,
  onUpdateTags,
  tagSuggestions = [],
  children,
}: {
  editor: any;
  note: Pick<Note, "title" | "updated_at" | "tags">;
  saveStatus: SaveStatus;
  errorMessage: string | null;
  isSyncing?: boolean;
  onInsertImage: () => void;
  findReplace: FindReplaceControls;
  onOpenHistory?: () => void;
  onUpdateTags?: (tags: string[]) => void;
  tagSuggestions?: TagSummary[];
  children: ReactNode;
}) {
  const [tagInput, setTagInput] = useState("");
  const normalizedInput = normalizeTagName(tagInput);
  const matchingTags = useMemo(
    () =>
      tagSuggestions
        .filter((tag) => {
          const normalized = tag.normalized_name || normalizeTagName(tag.name);
          return (
            normalizedInput &&
            normalized.includes(normalizedInput) &&
            !note.tags.some((item) => normalizeTagName(item) === normalized)
          );
        })
        .slice(0, 6),
    [normalizedInput, note.tags, tagSuggestions],
  );

  const addTag = (raw = tagInput) => {
    const value = raw.trim().replace(/^#/, "").trim();
    if (!value || note.tags.some((tag) => tag.toLowerCase() === value.toLowerCase())) return;
    onUpdateTags?.([...note.tags, value]);
    setTagInput("");
  };

  return (
    <div className="relative flex h-full flex-col" aria-busy={isSyncing}>
      <Toolbar
        editor={editor}
        note={note}
        onInsertImage={onInsertImage}
        extraActions={
          <ToolbarButton
            onClick={() => findReplace.setVisible((value) => !value)}
            active={findReplace.visible}
            title="查找替换"
          >
            <Search className="h-4 w-4" />
          </ToolbarButton>
        }
      />

      {findReplace.visible && <FindReplacePanel controls={findReplace} />}

      <div className="flex-1 overflow-y-auto" onDoubleClick={() => editor.chain().focus("end").run()}>
        <div className="px-8 pb-1 pt-3">
          <div className="relative flex min-h-8 flex-wrap items-center gap-2 text-xs">
            {note.tags.map((tag) => (
              <button
                key={tag}
                type="button"
                onClick={() => onUpdateTags?.(note.tags.filter((item) => item !== tag))}
                className="rounded-md bg-blue-50 px-2 py-1 font-medium text-blue-700 hover:bg-blue-100"
                title="移除标签"
              >
                #{tag}
              </button>
            ))}
            {onUpdateTags && (
              <label className="relative">
                <span className="sr-only">添加标签</span>
                <input
                  value={tagInput}
                  onChange={(event) => setTagInput(event.target.value)}
                  onKeyDown={(event) => {
                    if ((event.nativeEvent as KeyboardEvent).isComposing) return;
                    if (event.key === "Enter" || event.key === " ") {
                      event.preventDefault();
                      addTag();
                    }
                  }}
                  onBlur={() => addTag()}
                  placeholder="# 标签"
                  className="h-7 min-w-24 rounded-md bg-transparent px-1.5 text-xs text-gray-600 outline-none placeholder:text-gray-400 focus:bg-gray-50"
                />
                {matchingTags.length > 0 && tagInput.trim() && (
                  <div className="absolute left-0 top-8 z-20 min-w-36 rounded-md border border-gray-200 bg-white py-1 shadow-lg">
                    {matchingTags.map((tag) => (
                      <button
                        key={tag.id}
                        type="button"
                        onMouseDown={(event) => {
                          event.preventDefault();
                          addTag(tag.name);
                        }}
                        className="flex w-full items-center justify-between gap-3 px-2.5 py-1.5 text-left text-xs text-gray-700 hover:bg-blue-50 hover:text-blue-700"
                      >
                        <span className="truncate">#{tag.name}</span>
                        <span className="text-[10px] text-gray-400">{tag.note_count}</span>
                      </button>
                    ))}
                  </div>
                )}
              </label>
            )}
          </div>
        </div>
        {children}
      </div>

      <div className="flex items-center justify-between gap-3 border-t border-gray-100 px-8 py-2 text-xs text-gray-400">
        <span>
          {new Date(note.updated_at).toLocaleString("zh-CN", {
            month: "short",
            day: "numeric",
            hour: "2-digit",
            minute: "2-digit",
          })}
        </span>
        <span className={saveStatus === "error" ? "text-red-500" : isSyncing ? "text-blue-500" : ""}>
          {isSyncing ? "正在同步" : formatSaveStatus(saveStatus, errorMessage)}
        </span>
        {onOpenHistory ? (
          <button
            type="button"
            onClick={onOpenHistory}
            className="hover:text-gray-600"
            title="历史版本"
            aria-label="打开历史版本"
          >
            历史版本
          </button>
        ) : (
          <span>历史版本</span>
        )}
      </div>
    </div>
  );
}

function normalizeTagName(value: string): string {
  return value.trim().replace(/^#/, "").trim().toLowerCase();
}
