import { Search } from "lucide-react";
import type { ReactNode } from "react";
import type { Note, SaveStatus } from "@contracts";
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
  children,
}: {
  editor: any;
  note: Pick<Note, "title" | "updated_at">;
  saveStatus: SaveStatus;
  errorMessage: string | null;
  isSyncing?: boolean;
  onInsertImage: () => void;
  findReplace: FindReplaceControls;
  onOpenHistory?: () => void;
  children: ReactNode;
}) {
  return (
    <div className="relative flex h-full flex-col" aria-busy={isSyncing}>
      {isSyncing && (
        <div className="absolute inset-0 z-20 flex items-start justify-center bg-white/30 pt-16 cursor-wait">
          <span className="rounded bg-gray-800 px-3 py-1.5 text-xs text-white shadow">
            同步中，编辑暂时锁定
          </span>
        </div>
      )}
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
        {children}
      </div>

      <div className="flex items-center justify-between border-t border-gray-100 px-8 py-2 text-xs text-gray-400">
        <span>
          {new Date(note.updated_at).toLocaleString("zh-CN", {
            month: "short",
            day: "numeric",
            hour: "2-digit",
            minute: "2-digit",
          })}
        </span>
        <span className={saveStatus === "error" ? "text-red-500" : ""}>
          {formatSaveStatus(saveStatus, errorMessage)}
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
