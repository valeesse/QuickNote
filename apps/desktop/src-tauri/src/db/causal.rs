use super::*;

pub(super) fn get_entity_version_locked(
    conn: &Connection,
    entity_type: &str,
    entity_id: &str,
) -> Result<Option<CausalVersion>> {
    let raw = conn
        .query_row(
            "SELECT version_json FROM sync_entity_versions
             WHERE entity_type = ?1 AND entity_id = ?2",
            params![entity_type, entity_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    raw.map(|value| {
        serde_json::from_str(&value).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })
    })
    .transpose()
}

pub(super) fn set_entity_version_locked(
    conn: &Connection,
    entity_type: &str,
    entity_id: &str,
    version: &CausalVersion,
    dirty: bool,
) -> Result<()> {
    let version_json = serde_json::to_string(version)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    conn.execute(
        "INSERT INTO sync_entity_versions(entity_type, entity_id, version_json, dirty)
         VALUES(?1, ?2, ?3, ?4)
         ON CONFLICT(entity_type, entity_id) DO UPDATE SET
            version_json = excluded.version_json,
            dirty = excluded.dirty",
        params![entity_type, entity_id, version_json, dirty],
    )?;
    Ok(())
}

pub(super) fn ensure_local_causal_version_locked(
    conn: &Connection,
    entity_type: &str,
    entity_id: &str,
    device_id: &str,
) -> Result<CausalVersion> {
    let row = conn
        .query_row(
            "SELECT version_json, dirty FROM sync_entity_versions
             WHERE entity_type = ?1 AND entity_id = ?2",
            params![entity_type, entity_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?)),
        )
        .optional()?;
    let (mut version, dirty) = match row {
        Some((raw, dirty)) => (
            serde_json::from_str(&raw).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?,
            dirty,
        ),
        None => (CausalVersion::default(), true),
    };
    if dirty {
        version.increment(device_id);
        set_entity_version_locked(conn, entity_type, entity_id, &version, false)?;
    }
    Ok(version)
}

pub(super) fn upsert_remote_note_locked(conn: &Connection, remote: &Note) -> Result<()> {
    conn.execute(
        "INSERT INTO notes(
            id, title, content, plain_text, preview, is_pinned, sort_order,
            created_at, updated_at, version, is_deleted
         ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT(id) DO UPDATE SET
            title = excluded.title,
            content = excluded.content,
            plain_text = excluded.plain_text,
            preview = excluded.preview,
            is_pinned = excluded.is_pinned,
            sort_order = excluded.sort_order,
            updated_at = excluded.updated_at,
            version = excluded.version,
            is_deleted = excluded.is_deleted",
        params![
            remote.id,
            remote.title,
            remote.content,
            html_to_text(&remote.content),
            make_preview_without_title(&remote.content),
            remote.is_pinned,
            remote.sort_order,
            remote.created_at,
            remote.updated_at,
            remote.version,
            remote.is_deleted,
        ],
    )?;
    Ok(())
}

pub(super) fn enqueue_change(
    conn: &Connection,
    entity_type: &str,
    entity_id: &str,
    operation: &str,
    changed_at: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO sync_changes(entity_type, entity_id, operation, changed_at, synced)
         VALUES(?1, ?2, ?3, ?4, 0)",
        params![entity_type, entity_id, operation, changed_at],
    )?;
    conn.execute(
        "INSERT INTO sync_entity_versions(entity_type, entity_id, version_json, dirty)
         VALUES(?1, ?2, '{}', 1)
         ON CONFLICT(entity_type, entity_id) DO UPDATE SET dirty = 1",
        params![entity_type, entity_id],
    )?;
    Ok(())
}

pub(super) fn insert_conflict_copy(
    conn: &Connection,
    source: &Note,
    source_version: &CausalVersion,
) -> Result<String> {
    let version_json = serde_json::to_string(source_version)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    let digest = format!(
        "{:x}",
        Sha256::digest(format!("{}:{version_json}", source.id))
    );
    let id = format!("conflict-{}", &digest[..32]);
    let now = Utc::now().to_rfc3339();
    let title = format!("{} (冲突副本)", source.title);
    let plain_text = html_to_text(&source.content);
    let preview = make_preview_without_title(&source.content);
    let inserted = conn.execute(
        "INSERT OR IGNORE INTO notes(
            id, title, content, plain_text, preview, is_pinned,
            created_at, updated_at, version, is_deleted
         ) VALUES(?1, ?2, ?3, ?4, ?5, 0, ?6, ?7, 1, 0)",
        params![id, title, source.content, plain_text, preview, now, now],
    )?;
    if inserted > 0 {
        enqueue_change(conn, "note", &id, "upsert", &now)?;
    }
    Ok(id)
}
