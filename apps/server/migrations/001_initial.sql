-- Initial schema
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- Users
CREATE TABLE IF NOT EXISTS users (
    id UUID PRIMARY KEY,
    email VARCHAR(255) NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Notes
CREATE TABLE IF NOT EXISTS notes (
    id VARCHAR(64) NOT NULL,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title TEXT NOT NULL DEFAULT '',
    content TEXT NOT NULL DEFAULT '',
    is_pinned BOOLEAN NOT NULL DEFAULT false,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    version BIGINT NOT NULL DEFAULT 1,
    is_deleted BOOLEAN NOT NULL DEFAULT false,
    search_vector TSVECTOR GENERATED ALWAYS AS (
        setweight(to_tsvector('simple', coalesce(title, '')), 'A') ||
        setweight(to_tsvector('simple', coalesce(content, '')), 'B')
    ) STORED,
    PRIMARY KEY(user_id, id)
);

CREATE INDEX IF NOT EXISTS idx_notes_user_id ON notes(user_id);
CREATE INDEX IF NOT EXISTS idx_notes_search ON notes USING GIN(search_vector);
CREATE INDEX IF NOT EXISTS idx_notes_updated ON notes(user_id, is_pinned DESC, updated_at DESC);

-- Note versions
CREATE TABLE IF NOT EXISTS note_versions (
    id BIGSERIAL PRIMARY KEY,
    note_id VARCHAR(64) NOT NULL,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title TEXT NOT NULL DEFAULT '',
    content TEXT NOT NULL DEFAULT '',
    version BIGINT NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    is_pinned BOOLEAN NOT NULL DEFAULT false,
    FOREIGN KEY(user_id, note_id) REFERENCES notes(user_id, id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_note_versions_note ON note_versions(note_id);

-- Clipboard items
CREATE TABLE IF NOT EXISTS clipboard_items (
    id VARCHAR(64) NOT NULL,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind VARCHAR(10) NOT NULL DEFAULT 'text',
    content TEXT NOT NULL,
    preview TEXT NOT NULL DEFAULT '',
    source_device VARCHAR(64) NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    last_copied_at TEXT NOT NULL,
    capture_count BIGINT NOT NULL DEFAULT 1,
    is_pinned BOOLEAN NOT NULL DEFAULT false,
    is_deleted BOOLEAN NOT NULL DEFAULT false,
    PRIMARY KEY(user_id, id)
);

CREATE INDEX IF NOT EXISTS idx_clipboard_user ON clipboard_items(user_id, is_deleted, last_copied_at DESC);

-- Attachments
CREATE TABLE IF NOT EXISTS attachments (
    id VARCHAR(64) NOT NULL,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    relative_path TEXT NOT NULL,
    mime_type VARCHAR(128) NOT NULL DEFAULT '',
    size BIGINT NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    PRIMARY KEY(user_id, id)
);

CREATE INDEX IF NOT EXISTS idx_attachments_user ON attachments(user_id);
