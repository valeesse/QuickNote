ALTER TABLE users
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ADD COLUMN IF NOT EXISTS created_by UUID,
    ADD COLUMN IF NOT EXISTS updated_by UUID;

UPDATE users
SET updated_at = COALESCE(updated_at, created_at),
    created_by = COALESCE(created_by, id),
    updated_by = COALESCE(updated_by, id)
WHERE created_by IS NULL
   OR updated_by IS NULL;

ALTER TABLE notes
    ADD COLUMN IF NOT EXISTS created_by UUID,
    ADD COLUMN IF NOT EXISTS updated_by UUID;

UPDATE notes
SET created_by = COALESCE(created_by, user_id),
    updated_by = COALESCE(updated_by, user_id)
WHERE created_by IS NULL
   OR updated_by IS NULL;

ALTER TABLE note_versions
    ADD COLUMN IF NOT EXISTS updated_at TEXT NOT NULL DEFAULT NOW()::text,
    ADD COLUMN IF NOT EXISTS created_by UUID,
    ADD COLUMN IF NOT EXISTS updated_by UUID;

UPDATE note_versions
SET updated_at = COALESCE(NULLIF(updated_at, ''), created_at),
    created_by = COALESCE(created_by, user_id),
    updated_by = COALESCE(updated_by, user_id)
WHERE created_by IS NULL
   OR updated_by IS NULL
   OR updated_at IS NULL
   OR updated_at = '';

ALTER TABLE clipboard_items
    ADD COLUMN IF NOT EXISTS created_by UUID,
    ADD COLUMN IF NOT EXISTS updated_by UUID;

UPDATE clipboard_items
SET created_by = COALESCE(created_by, user_id),
    updated_by = COALESCE(updated_by, user_id)
WHERE created_by IS NULL
   OR updated_by IS NULL;

ALTER TABLE attachments
    ADD COLUMN IF NOT EXISTS updated_at TEXT NOT NULL DEFAULT NOW()::text,
    ADD COLUMN IF NOT EXISTS created_by UUID,
    ADD COLUMN IF NOT EXISTS updated_by UUID;

UPDATE attachments
SET updated_at = COALESCE(NULLIF(updated_at, ''), created_at),
    created_by = COALESCE(created_by, user_id),
    updated_by = COALESCE(updated_by, user_id)
WHERE created_by IS NULL
   OR updated_by IS NULL
   OR updated_at IS NULL
   OR updated_at = '';

ALTER TABLE cloud_changes
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ADD COLUMN IF NOT EXISTS created_by UUID,
    ADD COLUMN IF NOT EXISTS updated_by UUID;

UPDATE cloud_changes
SET updated_at = COALESCE(updated_at, created_at),
    created_by = COALESCE(created_by, user_id),
    updated_by = COALESCE(updated_by, user_id)
WHERE created_by IS NULL
   OR updated_by IS NULL;

ALTER TABLE entity_versions
    ADD COLUMN IF NOT EXISTS created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ADD COLUMN IF NOT EXISTS created_by UUID,
    ADD COLUMN IF NOT EXISTS updated_by UUID;

UPDATE entity_versions
SET created_by = COALESCE(created_by, user_id),
    updated_by = COALESCE(updated_by, user_id)
WHERE created_by IS NULL
   OR updated_by IS NULL;

ALTER TABLE sync_cursors
    ADD COLUMN IF NOT EXISTS created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ADD COLUMN IF NOT EXISTS created_by UUID,
    ADD COLUMN IF NOT EXISTS updated_by UUID;

UPDATE sync_cursors
SET created_by = COALESCE(created_by, user_id),
    updated_by = COALESCE(updated_by, user_id)
WHERE created_by IS NULL
   OR updated_by IS NULL;

ALTER TABLE billing_plans
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ADD COLUMN IF NOT EXISTS created_by UUID,
    ADD COLUMN IF NOT EXISTS updated_by UUID;

ALTER TABLE billing_prices
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ADD COLUMN IF NOT EXISTS created_by UUID,
    ADD COLUMN IF NOT EXISTS updated_by UUID;

ALTER TABLE subscriptions
    ADD COLUMN IF NOT EXISTS created_by UUID,
    ADD COLUMN IF NOT EXISTS updated_by UUID;

UPDATE subscriptions
SET created_by = COALESCE(created_by, user_id),
    updated_by = COALESCE(updated_by, user_id)
WHERE created_by IS NULL
   OR updated_by IS NULL;

ALTER TABLE entitlements
    ADD COLUMN IF NOT EXISTS created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ADD COLUMN IF NOT EXISTS created_by UUID,
    ADD COLUMN IF NOT EXISTS updated_by UUID;

UPDATE entitlements
SET created_by = COALESCE(created_by, user_id),
    updated_by = COALESCE(updated_by, user_id)
WHERE created_by IS NULL
   OR updated_by IS NULL;

ALTER TABLE usage_counters
    ADD COLUMN IF NOT EXISTS created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ADD COLUMN IF NOT EXISTS created_by UUID,
    ADD COLUMN IF NOT EXISTS updated_by UUID;

UPDATE usage_counters
SET created_by = COALESCE(created_by, user_id),
    updated_by = COALESCE(updated_by, user_id)
WHERE created_by IS NULL
   OR updated_by IS NULL;

ALTER TABLE billing_events
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ADD COLUMN IF NOT EXISTS created_by UUID,
    ADD COLUMN IF NOT EXISTS updated_by UUID;

CREATE INDEX IF NOT EXISTS idx_sync_cursors_user_updated
    ON sync_cursors(user_id, updated_at DESC);
