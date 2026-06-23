import { FileEdit, Plus } from "lucide-react";

interface EmptyStateProps {
  onCreateNote: () => void;
  /** Extra hint lines shown below the main CTA (e.g. keyboard shortcuts). */
  hints?: React.ReactNode;
}

export function EmptyState({ onCreateNote, hints }: EmptyStateProps) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center p-8 text-gray-400">
      <div className="mb-6 flex h-24 w-24 items-center justify-center rounded-2xl bg-gray-100">
        <FileEdit className="h-12 w-12 text-gray-300" />
      </div>
      <h2 className="mb-2 text-xl font-medium text-gray-500">选择或创建一个便签</h2>
      <p className="mb-6 max-w-xs text-center text-sm text-gray-400">
        从左侧列表选择一个便签开始编辑，或创建一个新的便签
      </p>
      <button
        onClick={onCreateNote}
        className="flex items-center gap-2 rounded-lg bg-blue-600 px-6 py-2.5 font-medium text-white shadow-sm transition-colors hover:bg-blue-700"
      >
        <Plus className="h-4 w-4" />
        新建便签
      </button>
      {hints && <div className="mt-8 space-y-1 text-center text-xs text-gray-300">{hints}</div>}
    </div>
  );
}
