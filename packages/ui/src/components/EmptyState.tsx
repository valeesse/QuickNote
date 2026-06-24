import { FileEdit, Plus } from "lucide-react";

interface EmptyStateProps {
  onCreateNote: () => void;
  /** Extra hint lines shown below the main CTA (e.g. keyboard shortcuts). */
  hints?: React.ReactNode;
}

export function EmptyState({ onCreateNote, hints }: EmptyStateProps) {
  return (
    <div className="empty-state">
      <div className="empty-state__icon">
        <FileEdit className="h-12 w-12 text-gray-300" />
      </div>
      <h2 className="empty-state__title">选择或创建一个便签</h2>
      <p className="empty-state__text">
        从左侧列表选择一个便签开始编辑，或创建一个新的便签
      </p>
      <button
        type="button"
        onClick={() => onCreateNote()}
        className="focus-ring empty-state__button"
      >
        <Plus className="h-4 w-4" />
        新建便签
      </button>
      {hints && <div className="empty-state__hints">{hints}</div>}
    </div>
  );
}
