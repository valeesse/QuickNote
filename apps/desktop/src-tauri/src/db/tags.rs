use super::*;

impl Database {
    pub fn get_tag_for_sync(&self, id: &str) -> Result<Option<Tag>> {
        let conn = self.conn.lock().unwrap();
        get_tag_locked(&conn, id, true)
    }

    pub fn get_note_tag_for_sync(&self, id: &str) -> Result<Option<NoteTag>> {
        let conn = self.conn.lock().unwrap();
        get_note_tag_locked(&conn, id)
    }

    pub fn apply_remote_tag(
        &self,
        remote: &Tag,
        remote_version: &CausalVersion,
        local_device_id: &str,
    ) -> Result<bool> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        if should_skip_remote_entity(&tx, "tag", &remote.id, remote_version, local_device_id)? {
            tx.commit()?;
            return Ok(false);
        }
        tx.execute(
            "INSERT INTO tags(id, name, normalized_name, color, created_at, updated_at, is_deleted)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                normalized_name = excluded.normalized_name,
                color = excluded.color,
                updated_at = excluded.updated_at,
                is_deleted = excluded.is_deleted",
            params![
                remote.id,
                remote.name,
                remote.normalized_name,
                remote.color,
                remote.created_at,
                remote.updated_at,
                remote.is_deleted,
            ],
        )?;
        set_entity_version_locked(&tx, "tag", &remote.id, remote_version, false)?;
        tx.commit()?;
        Ok(true)
    }

    pub fn apply_remote_note_tag(
        &self,
        remote: &NoteTag,
        remote_version: &CausalVersion,
        local_device_id: &str,
    ) -> Result<bool> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        if should_skip_remote_entity(&tx, "note_tag", &remote.id, remote_version, local_device_id)?
        {
            tx.commit()?;
            return Ok(false);
        }
        tx.execute(
            "INSERT OR IGNORE INTO note_tags(id, note_id, tag_id, created_at)
             VALUES(?1, ?2, ?3, ?4)",
            params![remote.id, remote.note_id, remote.tag_id, remote.created_at],
        )?;
        set_entity_version_locked(&tx, "note_tag", &remote.id, remote_version, false)?;
        tx.commit()?;
        Ok(true)
    }

    pub fn apply_remote_note_tag_delete(
        &self,
        id: &str,
        remote_version: &CausalVersion,
        local_device_id: &str,
    ) -> Result<bool> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        if should_skip_remote_entity(&tx, "note_tag", id, remote_version, local_device_id)? {
            tx.commit()?;
            return Ok(false);
        }
        let changed = tx.execute("DELETE FROM note_tags WHERE id = ?1", params![id])?;
        set_entity_version_locked(&tx, "note_tag", id, remote_version, false)?;
        tx.commit()?;
        Ok(changed > 0)
    }

    #[allow(dead_code)]
    pub fn get_notes_since(&self, since: &str) -> Result<Vec<Note>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, title, content, yjs_state, yjs_state_version, is_pinned, sort_order, created_at, updated_at, version, is_deleted
             FROM notes WHERE updated_at > ?1",
        )?;

        let notes = stmt
            .query_map(params![since], |row| {
                Ok(Note {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    content: row.get(2)?,
                    yjs_state: row.get(3)?,
                    yjs_state_version: row.get(4)?,
                    is_pinned: row.get(5)?,
                    sort_order: row.get(6)?,
                    created_at: row.get(7)?,
                    updated_at: row.get(8)?,
                    version: row.get(9)?,
                    is_deleted: row.get(10)?,
                    tags: Vec::new(),
                })
            })?
            .collect::<Result<Vec<_>>>()?;

        Ok(notes
            .into_iter()
            .map(|mut note| {
                note.tags = tags_for_note(&conn, &note.id).unwrap_or_default();
                note
            })
            .collect())
    }
}
