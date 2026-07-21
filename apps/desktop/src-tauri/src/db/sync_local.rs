use super::*;

impl Database {
    pub fn register_attachment(
        &self,
        id: &str,
        relative_path: &str,
        mime_type: &str,
        size: i64,
    ) -> Result<AttachmentRecord> {
        let mut conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO attachments(id, relative_path, mime_type, size, created_at)
             VALUES(?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO NOTHING",
            params![id, relative_path, mime_type, size, now],
        )?;
        enqueue_change(&tx, "attachment", id, "upsert", &now)?;
        let record = tx.query_row(
            "SELECT id, relative_path, mime_type, size, created_at FROM attachments WHERE id = ?1",
            params![id],
            attachment_from_row,
        )?;
        tx.commit()?;
        Ok(record)
    }

    pub fn get_attachment(&self, id: &str) -> Result<Option<AttachmentRecord>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, relative_path, mime_type, size, created_at FROM attachments WHERE id = ?1",
            params![id],
            attachment_from_row,
        )
        .optional()
    }

    #[cfg(test)]
    pub fn list_attachments_for_sync(&self) -> Result<Vec<AttachmentRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT id, relative_path, mime_type, size, created_at FROM attachments")?;
        let records = stmt
            .query_map([], attachment_from_row)?
            .collect::<Result<Vec<_>>>()?;
        Ok(records)
    }

    pub fn list_pending_changes(&self, limit: i64) -> Result<Vec<SyncChange>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT seq, entity_type, entity_id, operation, changed_at
             FROM sync_changes WHERE synced = 0 ORDER BY seq LIMIT ?1",
        )?;
        let changes = stmt
            .query_map(params![limit], |row| {
                Ok(SyncChange {
                    seq: row.get(0)?,
                    entity_type: row.get(1)?,
                    entity_id: row.get(2)?,
                    operation: row.get(3)?,
                    changed_at: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>>>()?;
        Ok(changes)
    }

    pub fn pending_sync_change_count(&self) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT COUNT(*) FROM sync_changes WHERE synced = 0",
            [],
            |row| row.get(0),
        )
    }

    pub fn ensure_sync_bootstrap(&self, scope: &str) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_bootstrap WHERE scope = ?1)",
            params![scope],
            |row| row.get(0),
        )?;
        if exists {
            return Ok(());
        }

        let now = Utc::now().to_rfc3339();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO sync_changes(entity_type, entity_id, operation, changed_at, synced)
             SELECT 'note', id, CASE WHEN is_deleted = 1 THEN 'delete' ELSE 'upsert' END, ?1, 0
             FROM notes",
            params![now],
        )?;
        tx.execute(
            "INSERT INTO sync_changes(entity_type, entity_id, operation, changed_at, synced)
             SELECT 'attachment', id, 'upsert', ?1, 0 FROM attachments",
            params![now],
        )?;
        tx.execute(
            "INSERT INTO sync_changes(entity_type, entity_id, operation, changed_at, synced)
             SELECT 'clipboard', id, CASE WHEN is_deleted = 1 THEN 'delete' ELSE 'upsert' END, ?1, 0
             FROM clipboard_items",
            params![now],
        )?;
        tx.execute(
            "INSERT INTO sync_bootstrap(scope, created_at) VALUES(?1, ?2)",
            params![scope, now],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn mark_change_synced(&self, seq: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE sync_changes SET synced = 1 WHERE seq = ?1",
            params![seq],
        )?;
        Ok(())
    }

    /// Synced outbox rows are acknowledgements, not history. Keeping them forever makes the
    /// local database grow with edit count even after WebDAV has committed the current state.
    pub fn prune_synced_changes(&self) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM sync_changes WHERE synced = 1", [])
    }

    #[allow(dead_code)]
    pub fn get_sync_cursor(&self, provider: &str, device_id: &str) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let cursor = conn
            .query_row(
                "SELECT cursor FROM sync_cursors WHERE provider = ?1 AND device_id = ?2",
                params![provider, device_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        Ok(cursor.and_then(|value| value.parse().ok()).unwrap_or(0))
    }

    #[allow(dead_code)]
    pub fn set_sync_cursor(&self, provider: &str, device_id: &str, cursor: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO sync_cursors(provider, device_id, cursor) VALUES(?1, ?2, ?3)
             ON CONFLICT(provider, device_id) DO UPDATE SET cursor = excluded.cursor",
            params![provider, device_id, cursor.to_string()],
        )?;
        Ok(())
    }

    pub fn get_sync_cursor_value(&self, provider: &str, device_id: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT cursor FROM sync_cursors WHERE provider = ?1 AND device_id = ?2",
            params![provider, device_id],
            |row| row.get(0),
        )
        .optional()
    }

    pub fn set_sync_cursor_value(
        &self,
        provider: &str,
        device_id: &str,
        cursor: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO sync_cursors(provider, device_id, cursor) VALUES(?1, ?2, ?3)
             ON CONFLICT(provider, device_id) DO UPDATE SET cursor = excluded.cursor",
            params![provider, device_id, cursor],
        )?;
        Ok(())
    }

    pub fn get_note_for_sync(&self, id: &str) -> Result<Option<Note>> {
        let conn = self.conn.lock().unwrap();
        get_note_locked(&conn, id, true)
    }

    pub fn ensure_local_causal_version(
        &self,
        entity_type: &str,
        entity_id: &str,
        device_id: &str,
    ) -> Result<CausalVersion> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let version = ensure_local_causal_version_locked(&tx, entity_type, entity_id, device_id)?;
        tx.commit()?;
        Ok(version)
    }

    /// Read the current causal version for an entity without incrementing or marking dirty.
    #[cfg(test)]
    pub fn get_entity_causal_version(
        &self,
        entity_type: &str,
        entity_id: &str,
    ) -> Result<Option<CausalVersion>> {
        let conn = self.conn.lock().unwrap();
        let row = conn
            .query_row(
                "SELECT version_json FROM sync_entity_versions
                 WHERE entity_type = ?1 AND entity_id = ?2",
                params![entity_type, entity_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        match row {
            Some(raw) => {
                // Parse outside query_row to avoid error type mismatch
                match serde_json::from_str(&raw) {
                    Ok(version) => Ok(Some(version)),
                    Err(_) => Ok(None),
                }
            }
            None => Ok(None),
        }
    }

    /// Mark all pending changes for a specific entity as synced.
    #[allow(dead_code)]
    pub fn mark_entity_changes_synced(&self, entity_type: &str, entity_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE sync_changes SET synced = 1
             WHERE entity_type = ?1 AND entity_id = ?2 AND synced = 0",
            params![entity_type, entity_id],
        )?;
        Ok(())
    }
}
