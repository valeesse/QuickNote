CREATE EXTENSION IF NOT EXISTS pg_trgm;

ALTER TABLE notes ADD COLUMN IF NOT EXISTS search_text TEXT NOT NULL DEFAULT '';

UPDATE notes
SET search_text = trim(regexp_replace(content, '<[^>]+>', ' ', 'g'))
WHERE search_text = '' AND content <> '';

CREATE INDEX IF NOT EXISTS idx_notes_search_trgm
    ON notes USING GIN ((lower(title || ' ' || search_text)) gin_trgm_ops);
