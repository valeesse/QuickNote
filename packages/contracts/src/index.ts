export interface Note {
  id: string;
  title: string;
  content: string;
  is_pinned: boolean;
  sort_order: number;
  created_at: string;
  updated_at: string;
  version: number;
  is_deleted: boolean;
}

export interface NoteSummary {
  id: string;
  title: string;
  preview: string;
  is_pinned: boolean;
  created_at: string;
  updated_at: string;
}

export interface ClipboardItem {
  id: string;
  kind: "text" | "link" | "code" | "image" | "rich";
  content: string;
  preview: string;
  source_device: string;
  created_at: string;
  updated_at: string;
  last_copied_at: string;
  capture_count: number;
  is_pinned: boolean;
  is_deleted: boolean;
}

export interface AttachmentRecord {
  id: string;
  relative_path: string;
  mime_type: string;
  size: number;
  created_at: string;
}

export interface NoteVersion {
  id: number;
  note_id: string;
  title: string;
  content: string;
  version: number;
  created_at: string;
  is_pinned: boolean;
}

export type SaveStatus = "idle" | "saving" | "saved" | "retrying" | "error";
export type AppView = "notes" | "clipboard";

export interface AuthUser { id: string; email: string }
export interface AuthResponse { token: string; user: AuthUser }
