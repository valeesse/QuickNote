import { LogOut, RefreshCw, Search, Trash2, X } from "lucide-react";
import type { AppView, Note, NoteSummary } from "@/types";

export function MobileNoteHeader({
  activeNote, notes, searchQuery, onSelect, onCreate, onSearch,
}: {
  activeNote: Note | null;
  notes: NoteSummary[];
  searchQuery: string;
  onSelect: (id: string) => void;
  onCreate: () => void;
  onSearch: (query: string) => void;
}) {
  return (
    <div className="space-y-2 border-b border-gray-200 bg-white px-3 py-2 md:hidden">
      <div className="flex items-center gap-2">
        <select
          value={activeNote?.id ?? ""}
          onChange={(event) => event.target.value && onSelect(event.target.value)}
          className="focus-ring min-w-0 flex-1 rounded-lg border border-gray-200 bg-gray-50 px-3 py-2 text-sm"
          aria-label="选择便签"
        >
          <option value="">选择便签</option>
          {notes.map((note) => <option key={note.id} value={note.id}>{note.title || "无标题"}</option>)}
        </select>
        <button type="button" onClick={onCreate} className="focus-ring rounded-lg bg-blue-600 px-3 py-2 text-sm font-medium text-white">
          新建
        </button>
      </div>
      <label className="relative block">
        <span className="sr-only">搜索便签</span>
        <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-gray-400" />
        <input
          type="search" value={searchQuery} onChange={(event) => onSearch(event.target.value)}
          placeholder="搜索便签..."
          className="focus-ring w-full rounded-lg border border-gray-200 bg-gray-50 py-2 pl-9 pr-9 text-sm placeholder-gray-400"
        />
        {searchQuery && (
          <button type="button" onClick={() => onSearch("")} className="focus-ring absolute right-2 top-1/2 flex h-6 w-6 -translate-y-1/2 items-center justify-center rounded-full text-gray-400 hover:bg-gray-200" aria-label="清空搜索">
            <X className="h-3.5 w-3.5" />
          </button>
        )}
      </label>
    </div>
  );
}

export function MobileBottomNav({
  viewMode, refreshing, trashOpen, userEmail, onChangeView, onRefresh, onTrash, onLogout,
}: {
  viewMode: AppView;
  refreshing: boolean;
  trashOpen: boolean;
  userEmail: string;
  onChangeView: (view: AppView) => void;
  onRefresh: () => void;
  onTrash: () => void;
  onLogout: () => void;
}) {
  return (
    <nav className="fixed inset-x-0 bottom-0 z-30 grid h-14 grid-cols-5 border-t border-gray-200 bg-white/95 px-2 backdrop-blur md:hidden">
      <button type="button" onClick={() => onChangeView("notes")} className={`focus-ring rounded-lg text-sm font-medium ${viewMode === "notes" ? "text-blue-600" : "text-gray-500"}`} aria-pressed={viewMode === "notes"}>便签</button>
      <button type="button" onClick={() => onChangeView("clipboard")} className={`focus-ring rounded-lg text-sm font-medium ${viewMode === "clipboard" ? "text-violet-600" : "text-gray-500"}`} aria-pressed={viewMode === "clipboard"}>剪贴板</button>
      <button type="button" onClick={onRefresh} disabled={refreshing} className={`focus-ring flex items-center justify-center rounded-lg ${refreshing ? "text-blue-600" : "text-gray-500"}`} aria-label={refreshing ? "正在刷新云端数据" : "刷新云端数据"}>
        <RefreshCw className={`h-4 w-4 ${refreshing ? "animate-spin" : ""}`} />
      </button>
      <button type="button" onClick={onTrash} className={`focus-ring flex items-center justify-center rounded-lg ${trashOpen ? "text-blue-600" : "text-gray-500"}`} aria-label={trashOpen ? "收起回收站" : "打开回收站"}>
        <Trash2 className="h-4 w-4" />
      </button>
      <button type="button" onClick={onLogout} className="focus-ring flex items-center justify-center rounded-lg text-gray-500" aria-label={`退出登录，当前用户 ${userEmail}`}>
        <LogOut className="h-4 w-4" />
      </button>
    </nav>
  );
}
