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

export type SaveStatus = "idle" | "saving" | "saved" | "retrying" | "error";

export type AppView = "notes" | "clipboard";

export interface ClipboardItem {
  id: string;
  kind: string;
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

export interface AuthUser {
  id: string;
  email: string;
}

export interface AuthResponse {
  token: string;
  user: AuthUser;
}
