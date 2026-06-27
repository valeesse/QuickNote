-- Tags and note-tag relationships
CREATE TABLE IF NOT EXISTS tags (
    id VARCHAR(64) NOT NULL,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    normalized_name TEXT NOT NULL,
    color TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    is_deleted BOOLEAN NOT NULL DEFAULT false,
    created_by UUID,
    updated_by UUID,
    PRIMARY KEY(user_id, id),
    UNIQUE(user_id, normalized_name)
);

CREATE TABLE IF NOT EXISTS note_tags (
    id VARCHAR(64) NOT NULL,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    note_id VARCHAR(64) NOT NULL,
    tag_id VARCHAR(64) NOT NULL,
    created_at TEXT NOT NULL,
    created_by UUID,
    updated_by UUID,
    PRIMARY KEY(user_id, id),
    UNIQUE(user_id, note_id, tag_id),
    FOREIGN KEY(user_id, note_id) REFERENCES notes(user_id, id) ON DELETE CASCADE,
    FOREIGN KEY(user_id, tag_id) REFERENCES tags(user_id, id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_tags_user_normalized ON tags(user_id, normalized_name);
CREATE INDEX IF NOT EXISTS idx_note_tags_user_tag ON note_tags(user_id, tag_id, note_id);
CREATE INDEX IF NOT EXISTS idx_note_tags_user_note ON note_tags(user_id, note_id);
