-- Sync tables
CREATE SEQUENCE IF NOT EXISTS cloud_changes_seq START 1;

CREATE TABLE IF NOT EXISTS cloud_changes (
    id BIGSERIAL PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    seq BIGINT NOT NULL,
    entity_type VARCHAR(32) NOT NULL,
    entity_id VARCHAR(64) NOT NULL,
    operation VARCHAR(16) NOT NULL,
    source_device VARCHAR(128) NOT NULL,
    source_seq BIGINT NOT NULL,
    envelope JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(user_id, source_device, source_seq)
);

CREATE INDEX IF NOT EXISTS idx_cloud_changes_user_seq ON cloud_changes(user_id, seq);

CREATE TABLE IF NOT EXISTS entity_versions (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    entity_type VARCHAR(32) NOT NULL,
    entity_id VARCHAR(64) NOT NULL,
    version JSONB NOT NULL,
    PRIMARY KEY(user_id, entity_type, entity_id)
);

-- Sync cursors (track per-device sync position)
CREATE TABLE IF NOT EXISTS sync_cursors (
    id BIGSERIAL PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    device_id VARCHAR(64) NOT NULL,
    cursor_seq BIGINT NOT NULL DEFAULT 0,
    UNIQUE(user_id, device_id)
);
