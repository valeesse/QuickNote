export function AppToasts({
  deletedTitle, error, onUndo,
}: {
  deletedTitle: string | null;
  error: string | null;
  onUndo: () => void;
}) {
  return (
    <>
      {deletedTitle && (
        <div role="status" className="animate-toast-in fixed right-4 bottom-20 z-40 flex max-w-sm items-center gap-3 rounded-xl border border-gray-200 bg-white px-4 py-3 text-sm text-gray-700 shadow-lg md:bottom-4">
          <span className="min-w-0 flex-1 truncate">已删除「{deletedTitle}」</span>
          <button type="button" onClick={onUndo} className="focus-ring rounded px-2 py-1 font-medium text-blue-600 hover:bg-blue-50">撤销</button>
        </div>
      )}
      {error && (
        <div className="animate-toast-in fixed right-4 bottom-4 max-w-sm rounded border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700 shadow">
          {error}
        </div>
      )}
    </>
  );
}
