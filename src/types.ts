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
  path: string;
}

export type SaveStatus = "idle" | "saving" | "saved" | "retrying" | "error";
