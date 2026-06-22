-- Sync tables
CREATE SEQUENCE IF NOT EXISTS cloud_changes_seq START 1;

CREATE TABLE IF NOT EXISTS cloud_changes (
    id BIGSERIAL PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    seq BIGINT NOT NULL,
    entity_type VARCHAR(32) NOT NULL,
    entity_id VARCHAR(64) NOT NULL,
    operation VARCHAR(16) NOT NULL,
    envelope JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_cloud_changes_user_seq ON cloud_changes(user_id, seq);

-- Sync cursors (track per-device sync position)
CREATE TABLE IF NOT EXISTS sync_cursors (
    id BIGSERIAL PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    device_id VARCHAR(64) NOT NULL,
    cursor_seq BIGINT NOT NULL DEFAULT 0,
    UNIQUE(user_id, device_id)
);
