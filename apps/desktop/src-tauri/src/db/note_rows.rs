use super::*;

pub(super) fn get_note_locked(
    conn: &Connection,
    id: &str,
    include_deleted: bool,
) -> Result<Option<Note>> {
    let note = conn.query_row(
        "SELECT id, title, content, yjs_state, yjs_state_version, is_pinned, sort_order, created_at, updated_at, version, is_deleted
         FROM notes WHERE id = ?1 AND (?2 = 1 OR is_deleted = 0)",
        params![id, include_deleted],
        |row| {
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
        },
    )
    .optional()?;
    Ok(note.map(|mut note| {
        note.tags = tags_for_note(conn, &note.id).unwrap_or_default();
        note
    }))
}

pub(super) fn get_tag_locked(
    conn: &Connection,
    id: &str,
    include_deleted: bool,
) -> Result<Option<Tag>> {
    conn.query_row(
        "SELECT id, name, normalized_name, color, created_at, updated_at, is_deleted
         FROM tags WHERE id = ?1 AND (?2 = 1 OR is_deleted = 0)",
        params![id, include_deleted],
        |row| {
            Ok(Tag {
                id: row.get(0)?,
                name: row.get(1)?,
                normalized_name: row.get(2)?,
                color: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
                is_deleted: row.get(6)?,
            })
        },
    )
    .optional()
}

pub(super) fn get_note_tag_locked(conn: &Connection, id: &str) -> Result<Option<NoteTag>> {
    conn.query_row(
        "SELECT id, note_id, tag_id, created_at FROM note_tags WHERE id = ?1",
        params![id],
        |row| {
            Ok(NoteTag {
                id: row.get(0)?,
                note_id: row.get(1)?,
                tag_id: row.get(2)?,
                created_at: row.get(3)?,
            })
        },
    )
    .optional()
}

pub(super) fn should_skip_remote_entity(
    conn: &Connection,
    entity_type: &str,
    entity_id: &str,
    remote_version: &CausalVersion,
    local_device_id: &str,
) -> Result<bool> {
    let local_dirty: bool = conn.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM sync_changes
            WHERE synced = 0 AND entity_type = ?1 AND entity_id = ?2
        )",
        params![entity_type, entity_id],
        |row| row.get(0),
    )?;
    let mut local_version = get_entity_version_locked(conn, entity_type, entity_id)?;
    if local_dirty {
        local_version = Some(ensure_local_causal_version_locked(
            conn,
            entity_type,
            entity_id,
            local_device_id,
        )?);
    }
    if let Some(local_version) = &local_version {
        if matches!(
            local_version.relation(remote_version),
            CausalRelation::Equal | CausalRelation::Dominates
        ) {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn attach_summary_tags(
    conn: &Connection,
    notes: Vec<NoteSummary>,
) -> Result<Vec<NoteSummary>> {
    notes
        .into_iter()
        .map(|mut note| {
            note.tags = tags_for_note(conn, &note.id)?;
            Ok(note)
        })
        .collect()
}

pub(super) fn tags_for_note(conn: &Connection, note_id: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT t.name
         FROM tags t
         JOIN note_tags nt ON nt.tag_id = t.id
         WHERE nt.note_id = ?1 AND t.is_deleted = 0
         ORDER BY lower(t.name) ASC",
    )?;
    let tags = stmt
        .query_map(params![note_id], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>>>()?;
    Ok(tags)
}

pub(super) fn delete_unused_tags_locked(
    conn: &Connection,
    tag_ids: &[String],
    now: &str,
) -> Result<()> {
    for tag_id in tag_ids {
        let changed = conn.execute(
            "UPDATE tags
             SET is_deleted = 1, updated_at = ?1
             WHERE id = ?2 AND is_deleted = 0
               AND NOT EXISTS (
                   SELECT 1
                   FROM note_tags nt
                   JOIN notes n ON n.id = nt.note_id
                   WHERE nt.tag_id = tags.id AND n.is_deleted = 0
               )",
            params![now, tag_id],
        )?;
        if changed > 0 {
            enqueue_change(conn, "tag", tag_id, "delete", now)?;
        }
    }
    Ok(())
}

pub(super) fn restore_note_tags_locked(conn: &Connection, note_id: &str, now: &str) -> Result<()> {
    let tag_ids = {
        let mut stmt = conn.prepare(
            "SELECT t.id
             FROM tags t
             JOIN note_tags nt ON nt.tag_id = t.id
             WHERE nt.note_id = ?1 AND t.is_deleted = 1",
        )?;
        let rows = stmt
            .query_map(params![note_id], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>>>()?;
        rows
    };

    for tag_id in tag_ids {
        conn.execute(
            "UPDATE tags SET is_deleted = 0, updated_at = ?1 WHERE id = ?2",
            params![now, tag_id],
        )?;
        enqueue_change(conn, "tag", &tag_id, "upsert", now)?;
    }
    Ok(())
}

pub(super) fn normalize_tag_names(names: &[String]) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    names
        .iter()
        .filter_map(|name| {
            let display = name.trim().trim_start_matches('#').trim();
            let normalized = normalize_tag_name(display);
            if normalized.is_empty() || !seen.insert(normalized) {
                None
            } else {
                Some(display.chars().take(40).collect::<String>())
            }
        })
        .collect()
}

pub(super) fn normalize_tag_name(name: &str) -> String {
    name.trim()
        .trim_start_matches('#')
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

pub(super) fn note_tag_id(note_id: &str, tag_id: &str) -> String {
    format!("{:x}", Sha256::digest(format!("{note_id}:{tag_id}")))
}

pub(super) fn tag_id_from_normalized(normalized_name: &str) -> String {
    format!("{:x}", Sha256::digest(format!("tag:{normalized_name}")))
}
