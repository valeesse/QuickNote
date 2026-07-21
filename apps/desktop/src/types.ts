export type { AppView, ClipboardItem, Note, NoteSummary, NoteVersion, SaveStatus, TagSummary } from "@contracts";

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

export interface ShortcutConfig {
  quick_note: string;
  clipboard_history: string;
  quick_note_alternate: string;
}

export type ShortcutConfigInput = ShortcutConfig;

export interface SyncReport {
  pushed: number;
  pulled: number;
  conflicts: number;
}

export interface WebDavStorageStatus {
  protocol_version: number;
  workspace_id: string;
  epoch: number;
  devices: number;
  stored_objects: number;
  reachable_objects: number;
  pending_gc_objects: number;
  stored_bytes: number;
}

export interface WebDavGcReport {
  deleted_objects: number;
  status: WebDavStorageStatus;
}

export type SyncStatus = "disabled" | "idle" | "waiting" | "syncing" | "retrying" | "synced" | "error";
