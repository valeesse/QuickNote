ALTER TABLE yjs_updates ADD COLUMN IF NOT EXISTS update_id UUID;

CREATE UNIQUE INDEX IF NOT EXISTS idx_yjs_updates_delivery
    ON yjs_updates(user_id, note_id, update_id)
    WHERE update_id IS NOT NULL;
