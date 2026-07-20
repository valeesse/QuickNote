use super::*;

const CLIPBOARD_RETENTION_LIMIT: i64 = 500;

impl Database {
    pub fn capture_clipboard(&self, content: &str, source_device: &str) -> Result<ClipboardItem> {
        let now = Utc::now().to_rfc3339();
        self.capture_clipboard_at(content, source_device, &now)
    }

    pub fn capture_clipboard_at(
        &self,
        content: &str,
        source_device: &str,
        captured_at: &str,
    ) -> Result<ClipboardItem> {
        let normalized = normalize_clipboard_content(content);
        if normalized.trim().is_empty() {
            return Err(rusqlite::Error::InvalidParameterName(
                "clipboard content is empty".to_string(),
            ));
        }
        if normalized.len() > 5_000_000 {
            return Err(rusqlite::Error::InvalidParameterName(
                "clipboard content exceeds 5 MB".to_string(),
            ));
        }
        let id = format!(
            "{:x}",
            Sha256::digest(format!("clipboard:text:{normalized}"))
        );
        let kind = classify_clipboard_content(&normalized);
        let preview = clipboard_preview(&normalized, &kind);
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO clipboard_items(
                id, kind, content, preview, source_device, created_at, updated_at,
                last_copied_at, capture_count, is_pinned, is_deleted
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?6, ?6, 1, 0, 0)
             ON CONFLICT(id) DO UPDATE SET
                source_device = excluded.source_device,
                updated_at = CASE
                    WHEN excluded.last_copied_at > clipboard_items.last_copied_at THEN excluded.updated_at
                    ELSE clipboard_items.updated_at
                END,
                last_copied_at = CASE
                    WHEN excluded.last_copied_at > clipboard_items.last_copied_at THEN excluded.last_copied_at
                    ELSE clipboard_items.last_copied_at
                END,
                capture_count = CASE
                    WHEN excluded.last_copied_at > clipboard_items.last_copied_at THEN clipboard_items.capture_count + 1
                    ELSE clipboard_items.capture_count
                END,
                is_deleted = 0",
            params![id, kind, normalized, preview, source_device, captured_at],
        )?;
        enqueue_change(&tx, "clipboard", &id, "upsert", captured_at)?;
        let item = get_clipboard_item_locked(&tx, &id, true)?.expect("inserted clipboard item");
        prune_clipboard_items_locked(&tx, captured_at, true)?;
        tx.commit()?;
        Ok(item)
    }

    pub fn list_clipboard_items(&self, query: &str, limit: i64) -> Result<Vec<ClipboardItem>> {
        let conn = self.conn.lock().unwrap();
        let pattern = format!("%{}%", query.trim());
        let mut stmt = conn.prepare(
            "SELECT id, kind, content, preview, source_device, created_at, updated_at,
                    last_copied_at, capture_count, is_pinned, is_deleted
             FROM clipboard_items
             WHERE is_deleted = 0 AND (?1 = '%%' OR content LIKE ?1)
             ORDER BY is_pinned DESC, last_copied_at DESC, created_at DESC
             LIMIT ?2",
        )?;
        let items = stmt
            .query_map(
                params![pattern, limit.clamp(1, 500)],
                clipboard_item_from_row,
            )?
            .collect();
        items
    }

    pub fn get_clipboard_item(&self, id: &str) -> Result<Option<ClipboardItem>> {
        let conn = self.conn.lock().unwrap();
        get_clipboard_item_locked(&conn, id, false)
    }

    pub fn get_clipboard_item_for_sync(&self, id: &str) -> Result<Option<ClipboardItem>> {
        let conn = self.conn.lock().unwrap();
        get_clipboard_item_locked(&conn, id, true)
    }

    pub fn toggle_clipboard_pin(&self, id: &str) -> Result<bool> {
        let now = Utc::now().to_rfc3339();
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let changed = tx.execute(
            "UPDATE clipboard_items
             SET is_pinned = NOT is_pinned, updated_at = ?1
             WHERE id = ?2 AND is_deleted = 0",
            params![now, id],
        )?;
        if changed > 0 {
            enqueue_change(&tx, "clipboard", id, "upsert", &now)?;
        }
        tx.commit()?;
        Ok(changed > 0)
    }

    pub fn delete_clipboard_item(&self, id: &str) -> Result<bool> {
        let now = Utc::now().to_rfc3339();
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let changed = tx.execute("DELETE FROM clipboard_items WHERE id = ?1", params![id])?;
        if changed > 0 {
            tx.execute(
                "DELETE FROM sync_changes
                 WHERE entity_type = 'clipboard' AND entity_id = ?1 AND synced = 0",
                params![id],
            )?;
            enqueue_change(&tx, "clipboard", id, "delete", &now)?;
        }
        tx.commit()?;
        Ok(changed > 0)
    }

    pub fn clear_clipboard_items(&self) -> Result<usize> {
        let now = Utc::now().to_rfc3339();
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        let ids = {
            let mut stmt = tx
                .prepare("SELECT id FROM clipboard_items WHERE is_deleted = 0 AND is_pinned = 0")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            rows.collect::<Result<Vec<_>>>()?
        };

        let changed = tx.execute("DELETE FROM clipboard_items WHERE is_pinned = 0", [])?;

        if changed > 0 {
            for id in &ids {
                enqueue_change(&tx, "clipboard", id, "delete", &now)?;
            }
        }

        tx.commit()?;
        Ok(changed)
    }

    pub fn apply_remote_clipboard(
        &self,
        remote: &ClipboardItem,
        remote_version: &CausalVersion,
        local_device_id: &str,
    ) -> Result<bool> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let local_dirty: bool = tx.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM sync_changes
                WHERE synced = 0 AND entity_type = 'clipboard' AND entity_id = ?1
            )",
            params![remote.id],
            |row| row.get(0),
        )?;
        let mut local_version = get_entity_version_locked(&tx, "clipboard", &remote.id)?;
        if local_dirty {
            local_version = Some(ensure_local_causal_version_locked(
                &tx,
                "clipboard",
                &remote.id,
                local_device_id,
            )?);
        }
        let version_to_store = if let Some(local_version) = &local_version {
            match local_version.relation(remote_version) {
                CausalRelation::Equal | CausalRelation::Dominates => {
                    tx.commit()?;
                    return Ok(false);
                }
                CausalRelation::Dominated => remote_version.clone(),
                CausalRelation::Concurrent => {
                    let remote_wins = remote_version.deterministic_cmp(local_version).is_gt();
                    let winner = if remote_wins {
                        remote_version
                    } else {
                        local_version
                    };
                    let merged = local_version.merge_with_winner(remote_version, winner);
                    if !remote_wins {
                        set_entity_version_locked(&tx, "clipboard", &remote.id, &merged, false)?;
                        tx.commit()?;
                        return Ok(false);
                    }
                    merged
                }
            }
        } else {
            remote_version.clone()
        };
        upsert_remote_clipboard_locked(&tx, remote)?;
        set_entity_version_locked(&tx, "clipboard", &remote.id, &version_to_store, false)?;
        prune_clipboard_items_locked(&tx, &remote.updated_at, false)?;
        tx.commit()?;
        Ok(true)
    }
}

fn prune_clipboard_items_locked(
    tx: &rusqlite::Transaction<'_>,
    changed_at: &str,
    enqueue_deletes: bool,
) -> Result<Vec<String>> {
    let ids = {
        let mut stmt = tx.prepare(
            "SELECT id FROM clipboard_items
             WHERE is_deleted = 0
             ORDER BY is_pinned DESC, last_copied_at DESC, created_at DESC, id DESC
             LIMIT -1 OFFSET ?1",
        )?;
        let rows = stmt.query_map(params![CLIPBOARD_RETENTION_LIMIT], |row| {
            row.get::<_, String>(0)
        })?;
        rows.collect::<Result<Vec<_>>>()?
    };
    for id in &ids {
        tx.execute("DELETE FROM clipboard_items WHERE id = ?1", params![id])?;
        if enqueue_deletes {
            tx.execute(
                "DELETE FROM sync_changes
                 WHERE entity_type = 'clipboard' AND entity_id = ?1 AND synced = 0",
                params![id],
            )?;
            enqueue_change(tx, "clipboard", id, "delete", changed_at)?;
        }
    }
    Ok(ids)
}
