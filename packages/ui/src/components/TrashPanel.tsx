import { X, RotateCcw, Trash2, Eraser } from "lucide-react";
import type { NoteSummary } from "@contracts";
import { stripHtml } from "../utils/html";

interface TrashPanelProps {
  notes: NoteSummary[];
  onClose: () => void;
  onRestore: (id: string) => void;
  onPurge: (id: string) => void;
  onPurgeAll?: () => void;
}

export function TrashPanel({ notes, onClose, onRestore, onPurge, onPurgeAll }: TrashPanelProps) {
  return (
    <div className="animate-drawer-in fixed inset-y-0 right-0 z-40 w-80 border-l border-gray-200 bg-white shadow-xl">
      {/* Header */}
      <div className="flex items-center justify-between border-b border-gray-100 px-4 py-3">
        <div className="flex items-center gap-2">
          <h2 className="text-sm font-semibold text-gray-800">回收站</h2>
          {notes.length > 0 && onPurgeAll && (
            <button
              type="button"
              onClick={onPurgeAll}
              className="flex items-center gap-1 rounded px-2 py-1 text-xs text-gray-400 hover:bg-red-50 hover:text-red-500"
              title="清空回收站"
              aria-label="清空回收站"
            >
              <Eraser className="h-3.5 w-3.5" />
              <span>清空</span>
            </button>
          )}
        </div>
        <button type="button" onClick={onClose} className="flex h-7 w-7 items-center justify-center rounded hover:bg-gray-100" title="关闭" aria-label="关闭">
          <X className="h-4 w-4 text-gray-500" />
        </button>
      </div>

      {/* List */}
      <div className="h-[calc(100%-49px)] overflow-y-auto">
        {notes.length === 0 ? (
          <p className="p-4 text-sm text-gray-400">回收站为空</p>
        ) : (
          notes.map((note) => (
            <div key={note.id} className="group border-b border-gray-100 px-4 py-3">
              <h3 className="truncate text-sm font-medium text-gray-800">{note.title || "无标题"}</h3>
              <p className="mt-1 line-clamp-2 text-xs text-gray-500">{stripHtml(note.preview)}</p>

              {/* Action footer — visible on hover, right-aligned with separator */}
              <div className="mt-1.5 flex items-center justify-end gap-1 opacity-0 transition-opacity group-hover:opacity-100">
                <button
                  type="button"
                  onClick={() => onRestore(note.id)}
                  className="flex h-7 items-center gap-1 rounded px-2 text-xs text-gray-400 hover:bg-blue-50 hover:text-blue-600"
                  title="恢复"
                  aria-label="恢复便签"
                >
                  <RotateCcw className="h-3.5 w-3.5" />
                  <span>恢复</span>
                </button>
                <button
                  type="button"
                  onClick={() => onPurge(note.id)}
                  className="flex h-7 items-center gap-1 rounded px-2 text-xs text-gray-400 hover:bg-red-50 hover:text-red-500"
                  title="永久删除"
                  aria-label="永久删除"
                >
                  <Trash2 className="h-3.5 w-3.5" />
                  <span>删除</span>
                </button>
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
