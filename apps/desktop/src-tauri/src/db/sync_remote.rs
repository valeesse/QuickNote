use super::*;

impl Database {
    pub fn apply_remote_note(
        &self,
        remote: &Note,
        remote_version: &CausalVersion,
        local_device_id: &str,
    ) -> Result<(bool, bool)> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let local = get_note_locked(&tx, &remote.id, true)?;
        let local_dirty: bool = tx.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM sync_changes
                WHERE synced = 0 AND entity_type = 'note' AND entity_id = ?1
            )",
            params![remote.id],
            |row| row.get(0),
        )?;
        let mut local_version = get_entity_version_locked(&tx, "note", &remote.id)?;
        if local_dirty {
            local_version = Some(ensure_local_causal_version_locked(
                &tx,
                "note",
                &remote.id,
                local_device_id,
            )?);
        }

        let mut conflict = false;
        let version_to_store = if let Some(local_version) = &local_version {
            match local_version.relation(remote_version) {
                CausalRelation::Equal | CausalRelation::Dominates => {
                    tx.commit()?;
                    return Ok((false, false));
                }
                CausalRelation::Dominated => remote_version.clone(),
                CausalRelation::Concurrent => {
                    if let Some(local_note) = local.as_ref() {
                        if let Some(merged_note) = merge_collaborative_notes(local_note, remote)? {
                            let winner = if remote_version.deterministic_cmp(local_version).is_gt()
                            {
                                remote_version
                            } else {
                                local_version
                            };
                            let merged_version =
                                local_version.merge_with_winner(remote_version, winner);
                            upsert_remote_note_locked(&tx, &merged_note)?;
                            set_entity_version_locked(
                                &tx,
                                "note",
                                &remote.id,
                                &merged_version,
                                false,
                            )?;
                            tx.commit()?;
                            return Ok((true, false));
                        }
                    }
                    conflict = local
                        .as_ref()
                        .is_some_and(|note| note.is_deleted || note.content != remote.content);
                    let remote_wins = remote_version.deterministic_cmp(local_version).is_gt();
                    let winner = if remote_wins {
                        remote_version
                    } else {
                        local_version
                    };
                    let merged = local_version.merge_with_winner(remote_version, winner);
                    if conflict {
                        if remote_wins {
                            if let Some(local_note) = local.as_ref().filter(|note| !note.is_deleted)
                            {
                                insert_conflict_copy(&tx, local_note, local_version)?;
                            }
                        } else if !remote.is_deleted {
                            insert_conflict_copy(&tx, remote, remote_version)?;
                        }
                    }
                    if !remote_wins {
                        set_entity_version_locked(&tx, "note", &remote.id, &merged, false)?;
                        tx.commit()?;
                        return Ok((false, conflict));
                    }
                    merged
                }
            }
        } else {
            remote_version.clone()
        };

        upsert_remote_note_locked(&tx, remote)?;
        set_entity_version_locked(&tx, "note", &remote.id, &version_to_store, false)?;
        tx.commit()?;
        Ok((true, conflict))
    }

    pub fn apply_remote_delete(
        &self,
        id: &str,
        deleted_at: &str,
        remote_version: &CausalVersion,
        local_device_id: &str,
    ) -> Result<(bool, bool)> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let local = get_note_locked(&tx, id, true)?;
        let local_dirty: bool = tx.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM sync_changes
                WHERE synced = 0 AND entity_type = 'note' AND entity_id = ?1
            )",
            params![id],
            |row| row.get(0),
        )?;
        let mut local_version = get_entity_version_locked(&tx, "note", id)?;
        if local_dirty {
            local_version = Some(ensure_local_causal_version_locked(
                &tx,
                "note",
                id,
                local_device_id,
            )?);
        }
        let mut conflict = false;
        let version_to_store = if let Some(local_version) = &local_version {
            match local_version.relation(remote_version) {
                CausalRelation::Equal | CausalRelation::Dominates => {
                    tx.commit()?;
                    return Ok((false, false));
                }
                CausalRelation::Dominated => remote_version.clone(),
                CausalRelation::Concurrent => {
                    conflict = local.as_ref().is_some_and(|note| !note.is_deleted);
                    let remote_wins = remote_version.deterministic_cmp(local_version).is_gt();
                    let winner = if remote_wins {
                        remote_version
                    } else {
                        local_version
                    };
                    let merged = local_version.merge_with_winner(remote_version, winner);
                    if remote_wins {
                        if let Some(local_note) = local.as_ref().filter(|note| !note.is_deleted) {
                            insert_conflict_copy(&tx, local_note, local_version)?;
                        }
                    } else {
                        set_entity_version_locked(&tx, "note", id, &merged, false)?;
                        tx.commit()?;
                        return Ok((false, conflict));
                    }
                    merged
                }
            }
        } else {
            remote_version.clone()
        };
        tx.execute(
            "UPDATE notes SET is_deleted = 1, updated_at = ?1 WHERE id = ?2",
            params![deleted_at, id],
        )?;
        tx.execute(
            "INSERT INTO note_tombstones(id, deleted_at) VALUES(?1, ?2)
             ON CONFLICT(id) DO UPDATE SET deleted_at = excluded.deleted_at",
            params![id, deleted_at],
        )?;
        set_entity_version_locked(&tx, "note", id, &version_to_store, false)?;
        tx.commit()?;
        Ok((true, conflict))
    }

    pub fn register_remote_attachment(&self, record: &AttachmentRecord) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO attachments(id, relative_path, mime_type, size, created_at)
             VALUES(?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
                relative_path = excluded.relative_path,
                mime_type = excluded.mime_type,
                size = excluded.size",
            params![
                record.id,
                record.relative_path,
                record.mime_type,
                record.size,
                record.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn orphan_attachments(&self) -> Result<Vec<AttachmentRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut referenced_content = String::new();
        {
            let mut stmt = conn.prepare(
                "SELECT content FROM notes
                     UNION ALL SELECT content FROM note_versions
                     UNION ALL SELECT content FROM clipboard_items",
            )?;
            let contents = stmt
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>>>()?;
            for content in contents {
                referenced_content.push_str(&content);
            }
        }

        let mut stmt =
            conn.prepare("SELECT id, relative_path, mime_type, size, created_at FROM attachments")?;
        let records = stmt
            .query_map([], attachment_from_row)?
            .collect::<Result<Vec<_>>>()?;
        Ok(records
            .into_iter()
            .filter(|record| !referenced_content.contains(&format!("attachment://{}", record.id)))
            .collect())
    }

    pub fn remove_attachment_record(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM attachments WHERE id = ?1", params![id])?;
        Ok(())
    }
}
