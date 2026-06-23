import { X, RotateCcw, Pin, PinOff, Trash2, Eraser } from "lucide-react";
import type { NoteVersion } from "@contracts";
import { stripHtml } from "../utils/html";

interface HistoryPanelProps {
  versions: NoteVersion[];
  onClose: () => void;
  onRestore: (versionId: number) => void;
  onTogglePin: (versionId: number) => void;
  onDelete: (versionId: number) => void;
  onClear: () => void;
}

export function HistoryPanel({
  versions,
  onClose,
  onRestore,
  onTogglePin,
  onDelete,
  onClear,
}: HistoryPanelProps) {
  const unpinnedCount = versions.filter((v) => !v.is_pinned).length;

  return (
    <div className="animate-drawer-in fixed inset-y-0 right-0 z-40 w-80 border-l border-gray-200 bg-white shadow-xl">
      <div className="flex items-center justify-between border-b border-gray-100 px-4 py-3">
        <div>
          <h2 className="text-sm font-semibold text-gray-800">历史版本</h2>
          <p className="mt-0.5 text-xs text-gray-400">每 5 分钟自动保存，未固定最多 10 个</p>
        </div>
        <div className="flex items-center gap-1">
          {unpinnedCount > 0 && (
            <button type="button" onClick={onClear} className="flex h-7 items-center gap-1 rounded px-2 text-xs text-red-500 hover:bg-red-50" title="清空所有未固定版本" aria-label="清空所有未固定版本">
              <Eraser className="h-3.5 w-3.5" />
            </button>
          )}
          <button type="button" onClick={onClose} className="h-7 w-7 rounded hover:bg-gray-100 flex items-center justify-center" title="关闭" aria-label="关闭">
            <X className="h-4 w-4 text-gray-500" />
          </button>
        </div>
      </div>
      <div className="h-[calc(100%-65px)] overflow-y-auto">
        {versions.length === 0 ? (
          <p className="p-4 text-sm text-gray-400">暂无历史版本</p>
        ) : (
          versions.map((version) => (
            <div key={version.id} className="border-b border-gray-100 px-4 py-3">
              <div className="flex items-center justify-between gap-2">
                <h3 className="truncate text-sm font-medium text-gray-800">
                  {new Date(version.created_at).toLocaleString("zh-CN")}
                </h3>
                {version.is_pinned && (
                  <span className="flex h-5 w-5 flex-shrink-0 items-center justify-center text-blue-600" title="已固定">
                    <Pin className="h-3.5 w-3.5" />
                  </span>
                )}
              </div>
              <p className="mt-1 line-clamp-3 text-xs text-gray-500">{stripHtml(version.content)}</p>
              <div className="mt-3 flex gap-1">
                <button type="button" onClick={() => onRestore(version.id)} className="flex h-8 w-8 items-center justify-center rounded text-blue-600 hover:bg-blue-50" title="恢复此版本" aria-label="恢复此版本">
                  <RotateCcw className="h-4 w-4" />
                </button>
                <button
                  type="button"
                  onClick={() => onTogglePin(version.id)}
                  className={`flex h-8 w-8 items-center justify-center rounded hover:bg-gray-100 ${version.is_pinned ? "text-blue-600" : "text-gray-600"}`}
                  title={version.is_pinned ? "取消固定" : "固定此版本"}
                  aria-label={version.is_pinned ? "取消固定" : "固定此版本"}
                >
                  {version.is_pinned ? <PinOff className="h-4 w-4" /> : <Pin className="h-4 w-4" />}
                </button>
                <button type="button" onClick={() => onDelete(version.id)} className="flex h-8 w-8 items-center justify-center rounded text-gray-400 hover:bg-red-50 hover:text-red-500" title="删除此版本" aria-label="删除此版本">
                  <Trash2 className="h-4 w-4" />
                </button>
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
