export interface Note {
  id: string;
  title: string;
  content: string;
  is_pinned: boolean;
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

export interface NoteVersion {
  id: number;
  note_id: string;
  title: string;
  content: string;
  version: number;
  created_at: string;
  is_pinned: boolean;
}

export interface Attachment {
  id: string;
  path: string;
}

export type SaveStatus = "idle" | "saving" | "saved" | "retrying" | "error";

export interface SyncConfig {
  enabled: boolean;
  provider: "webdav";
  endpoint: string;
  username: string;
  device_id: string;
}

export interface SyncConfigInput {
  enabled: boolean;
  provider: "webdav";
  endpoint: string;
  username: string;
  password?: string;
}

export interface SyncReport {
  pushed: number;
  pulled: number;
  conflicts: number;
}

export type SyncStatus = "disabled" | "idle" | "syncing" | "synced" | "error";

export type AppView = "notes" | "clipboard";

export interface ClipboardItem {
  id: string;
  kind: "text" | "link" | "code";
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
