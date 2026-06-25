-- Yjs collaboration storage.
ALTER TABLE notes ADD COLUMN IF NOT EXISTS yjs_state BYTEA;
ALTER TABLE notes ADD COLUMN IF NOT EXISTS yjs_state_version BIGINT NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS yjs_updates (
    id BIGSERIAL PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    note_id VARCHAR(64) NOT NULL,
    update BYTEA NOT NULL,
    source_client_id VARCHAR(128),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_yjs_updates_note
    ON yjs_updates(user_id, note_id, id);
