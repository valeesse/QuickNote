interface EditorSkeletonProps {
  /** Whether to show the status bar skeleton at the bottom. */
  showStatusBar?: boolean;
}

export function EditorSkeleton({ showStatusBar = false }: EditorSkeletonProps) {
  return (
    <div className="flex h-full flex-col animate-pulse">
      {/* Toolbar skeleton */}
      <div className="flex items-center gap-2 border-b border-gray-100 px-8 py-2">
        {Array.from({ length: 12 }).map((_, i) => (
          <div key={i} className="h-7 w-7 rounded bg-gray-100" />
        ))}
      </div>
      {/* Content skeleton */}
      <div className="flex-1 space-y-4 px-8 py-6">
        <div className="h-5 w-3/4 rounded bg-gray-100" />
        <div className="h-4 w-full rounded bg-gray-50" />
        <div className="h-4 w-5/6 rounded bg-gray-50" />
        <div className="h-4 w-2/3 rounded bg-gray-50" />
      </div>
      {/* Status bar skeleton */}
      {showStatusBar && (
        <div className="border-t border-gray-100 px-8 py-2">
          <div className="h-3 w-20 rounded bg-gray-100" />
        </div>
      )}
    </div>
  );
}
