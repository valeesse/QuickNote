use super::*;

pub(super) async fn fetch_tag(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: Uuid,
    id: &str,
) -> Result<crate::models::Tag, AppError> {
    Ok(sqlx::query_as(
        "SELECT id,name,normalized_name,color,created_at,updated_at,is_deleted
         FROM tags WHERE user_id=$1 AND id=$2",
    )
    .bind(user_id)
    .bind(id)
    .fetch_one(&mut **tx)
    .await?)
}

pub(super) async fn fetch_note_tag(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: Uuid,
    id: &str,
) -> Result<crate::models::NoteTag, AppError> {
    Ok(sqlx::query_as(
        "SELECT id,note_id,tag_id,created_at
         FROM note_tags WHERE user_id=$1 AND id=$2",
    )
    .bind(user_id)
    .bind(id)
    .fetch_one(&mut **tx)
    .await?)
}

pub(super) async fn delete_unused_tags(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: Uuid,
    tag_ids: &[String],
    now: &str,
) -> Result<(), AppError> {
    for tag_id in tag_ids {
        let deleted: Option<String> = sqlx::query_scalar(
            "UPDATE tags
             SET is_deleted=true, updated_at=$3, updated_by=$1
             WHERE user_id=$1 AND id=$2 AND is_deleted=false
               AND NOT EXISTS (
                   SELECT 1
                   FROM note_tags nt
                   JOIN notes n ON n.user_id=nt.user_id AND n.id=nt.note_id
                   WHERE nt.user_id=$1 AND nt.tag_id=tags.id AND n.is_deleted=false
               )
             RETURNING id",
        )
        .bind(user_id)
        .bind(tag_id)
        .bind(now)
        .fetch_optional(&mut **tx)
        .await?;
        if deleted.is_some() {
            append_change(tx, user_id, "tag", tag_id, "delete", ChangePayload::None).await?;
        }
    }
    Ok(())
}

pub(super) async fn restore_note_tags(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: Uuid,
    note_id: &str,
    now: &str,
) -> Result<(), AppError> {
    let tag_ids: Vec<String> = sqlx::query_scalar(
        "SELECT t.id
         FROM tags t
         JOIN note_tags nt ON nt.user_id=t.user_id AND nt.tag_id=t.id
         WHERE t.user_id=$1 AND nt.note_id=$2 AND t.is_deleted=true",
    )
    .bind(user_id)
    .bind(note_id)
    .fetch_all(&mut **tx)
    .await?;

    for tag_id in tag_ids {
        sqlx::query(
            "UPDATE tags SET is_deleted=false, updated_at=$3, updated_by=$1
             WHERE user_id=$1 AND id=$2",
        )
        .bind(user_id)
        .bind(&tag_id)
        .bind(now)
        .execute(&mut **tx)
        .await?;
        let payload = fetch_tag(tx, user_id, &tag_id).await?;
        append_change(
            tx,
            user_id,
            "tag",
            &tag_id,
            "upsert",
            ChangePayload::Tag(payload),
        )
        .await?;
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
                Some(display.chars().take(40).collect())
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

pub(super) fn extract_title(content: &str) -> String {
    let plain = extract_plain_text(content);
    let title = plain
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim_start_matches('#')
        .trim();
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    let title: String = title.chars().take(100).collect();
    if title.is_empty() {
        "Untitled".to_string()
    } else {
        title
    }
}

pub(super) fn extract_plain_text(content: &str) -> String {
    let mut plain = String::with_capacity(content.len());
    let mut in_tag = false;
    let mut tag = String::new();
    for ch in content.chars() {
        match ch {
            '<' => {
                in_tag = true;
                tag.clear();
            }
            '>' => {
                in_tag = false;
                let tag = tag.trim().to_ascii_lowercase();
                if tag.starts_with("br")
                    || tag.starts_with("/p")
                    || tag.starts_with("/h")
                    || tag.starts_with("/li")
                    || tag.starts_with("/div")
                    || tag.starts_with("/pre")
                {
                    plain.push('\n');
                } else {
                    plain.push(' ');
                }
            }
            _ if in_tag => tag.push(ch),
            _ if !in_tag => plain.push(ch),
            _ => {}
        }
    }
    plain
}

pub(super) fn notify(state: &AppState, user_id: Uuid, id: &str, operation: &str) {
    let _ = state.event_tx.send(crate::models::SyncEvent {
        user_id,
        entity_type: "note".into(),
        entity_id: id.into(),
        operation: operation.into(),
    });
}

pub(super) fn attachment_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX
        .get_or_init(|| Regex::new(r"attachment://([a-f0-9]{64})").expect("valid attachment regex"))
}

pub(super) fn collect_attachment_candidates<'a>(
    contents: impl IntoIterator<Item = &'a str>,
) -> Vec<String> {
    let mut ids = BTreeSet::new();
    for content in contents {
        for captures in attachment_regex().captures_iter(content) {
            if let Some(id) = captures.get(1) {
                ids.insert(id.as_str().to_string());
            }
        }
    }
    ids.into_iter().collect()
}

pub(super) async fn attachment_is_still_referenced(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: Uuid,
    attachment_id: &str,
    purged_note_id: &str,
) -> Result<bool, AppError> {
    let pattern = format!("%attachment://{attachment_id}%");
    let referenced_in_notes: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM notes
            WHERE user_id=$1 AND id<>$2 AND content LIKE $3
        )",
    )
    .bind(user_id)
    .bind(purged_note_id)
    .bind(&pattern)
    .fetch_one(&mut **tx)
    .await?;
    if referenced_in_notes {
        return Ok(true);
    }
    let referenced_in_versions: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM note_versions
            WHERE user_id=$1 AND note_id<>$2 AND content LIKE $3
        )",
    )
    .bind(user_id)
    .bind(purged_note_id)
    .bind(&pattern)
    .fetch_one(&mut **tx)
    .await?;
    if referenced_in_versions {
        return Ok(true);
    }
    let referenced_in_clipboard: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM clipboard_items
            WHERE user_id=$1 AND is_deleted=false AND content LIKE $2
        )",
    )
    .bind(user_id)
    .bind(&pattern)
    .fetch_one(&mut **tx)
    .await?;
    Ok(referenced_in_clipboard)
}
