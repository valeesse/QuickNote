import type { AppView, ClipboardItem, NoteSummary, TagSummary } from "@contracts";
import { SidebarView } from "./sidebar/SidebarView";
import { useSidebarModel } from "./sidebar/useSidebarModel";

export type SidebarSyncStatus = "disabled" | "idle" | "syncing" | "synced" | "error";

export interface SidebarProps {
  viewMode: AppView;
  onViewModeChange: (mode: AppView) => void;
  clipboardCount: number;
  clipboardItems: ClipboardItem[];
  notes: NoteSummary[];
  tags: TagSummary[];
  selectedTag: string | null;
  activeNoteId: string | null;
  searchQuery: string;
  onSearchChange: (query: string) => void;
  onSelectTag: (tag: string | null) => void;
  onSelectNote: (id: string) => void;
  onCreateNote: () => void;
  onDeleteNote: (id: string) => void;
  onTogglePin: (id: string) => void;
  onReorderNotes: (orderedIds: string[], isPinned: boolean) => void;
  onLoadMoreNotes?: () => void;
  hasMoreNotes?: boolean;
  isLoadingMoreNotes?: boolean;
  onOpenTrash: () => void;
  isTrashOpen: boolean;
  onSelectClipboardItem: (id: string) => void;
  onCreateNoteFromClipboard: (id: string) => void;
  syncStatus?: SidebarSyncStatus;
  onSync?: () => void;
  onOpenSettings?: () => void;
  settingsLabel?: string;
  userEmail?: string;
  onLogout?: () => void;
}

export function Sidebar(props: SidebarProps) {
  const model = useSidebarModel(props);
  return <SidebarView props={props} model={model} />;
}
