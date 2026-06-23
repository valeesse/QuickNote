export type { AppView, ClipboardItem, Note, NoteSummary, SaveStatus } from "@contracts";

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

export interface SyncConfig {
  enabled: boolean;
  provider: "webdav";
  endpoint: string;
  username: string;
  device_id: string;
  cloud_enabled: boolean;
  cloud_url: string;
  cloud_email: string;
  cloud_cursor_seq: number;
  cloud_token_created_at: number;
}

export interface SyncConfigInput {
  enabled: boolean;
  provider: "webdav";
  endpoint: string;
  username: string;
  password?: string;
  cloud_enabled: boolean;
  cloud_url: string;
  cloud_email: string;
  cloud_password?: string;
}

export interface SyncReport {
  pushed: number;
  pulled: number;
  conflicts: number;
}

export type SyncStatus = "disabled" | "idle" | "syncing" | "synced" | "error";
