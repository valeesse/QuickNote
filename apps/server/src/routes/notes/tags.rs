use super::*;

#[derive(serde::Deserialize)]
pub struct ListNotesQuery {
    pub tag: Option<String>,
    pub limit: Option<i64>,
}

pub async fn list_tags(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
) -> Result<Json<Vec<TagSummary>>, AppError> {
    let tags = sqlx::query_as(
        "SELECT t.id,t.name,t.normalized_name,t.color,COUNT(n.id)::BIGINT AS note_count
         FROM tags t
         LEFT JOIN note_tags nt ON nt.user_id=t.user_id AND nt.tag_id=t.id
         LEFT JOIN notes n ON n.user_id=t.user_id AND n.id=nt.note_id AND n.is_deleted=false
         WHERE t.user_id=$1 AND t.is_deleted=false
         GROUP BY t.id,t.name,t.normalized_name,t.color
         HAVING COUNT(n.id) > 0
         ORDER BY note_count DESC, lower(t.name) ASC",
    )
    .bind(user_id)
    .fetch_all(state.db.inner())
    .await?;
    Ok(Json(tags))
}

pub async fn update_note_tags(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateNoteTagsRequest>,
) -> Result<Json<Note>, AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    let mut tx = state.db.inner().begin().await?;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM notes WHERE user_id=$1 AND id=$2 AND is_deleted=false)",
    )
    .bind(user_id)
    .bind(&id)
    .fetch_one(&mut *tx)
    .await?;
    if !exists {
        tx.rollback().await?;
        return Err(AppError::NotFound);
    }
    let tag_names = normalize_tag_names(&req.tags);
    let mut next_tag_ids = Vec::new();
    for name in tag_names {
        let normalized = normalize_tag_name(&name);
        let tag_id = sqlx::query_scalar::<_, String>(
            "INSERT INTO tags(id,user_id,name,normalized_name,color,created_at,updated_at,is_deleted,created_by,updated_by)
             VALUES ($1,$2,$3,$4,NULL,$5,$5,false,$2,$2)
             ON CONFLICT(user_id,normalized_name) DO UPDATE SET
                name=EXCLUDED.name, updated_at=EXCLUDED.updated_at, is_deleted=false, updated_by=EXCLUDED.updated_by
             RETURNING id",
        )
        .bind(tag_id_from_normalized(&normalized))
        .bind(user_id)
        .bind(&name)
        .bind(&normalized)
        .bind(&now)
        .fetch_one(&mut *tx)
        .await?;
        let tag_payload = fetch_tag(&mut tx, user_id, &tag_id).await?;
        append_change(
            &mut tx,
            user_id,
            "tag",
            &tag_id,
            "upsert",
            ChangePayload::Tag(tag_payload),
        )
        .await?;
        next_tag_ids.push(tag_id);
    }
    let current_ids: Vec<String> =
        sqlx::query_scalar("SELECT tag_id FROM note_tags WHERE user_id=$1 AND note_id=$2")
            .bind(user_id)
            .bind(&id)
            .fetch_all(&mut *tx)
            .await?;
    for tag_id in &current_ids {
        if !next_tag_ids.iter().any(|next| next == tag_id) {
            let relation_id = note_tag_id(&id, tag_id);
            sqlx::query("DELETE FROM note_tags WHERE user_id=$1 AND id=$2")
                .bind(user_id)
                .bind(&relation_id)
                .execute(&mut *tx)
                .await?;
            append_change(
                &mut tx,
                user_id,
                "note_tag",
                &relation_id,
                "delete",
                ChangePayload::None,
            )
            .await?;
        }
    }
    delete_unused_tags(&mut tx, user_id, &current_ids, &now).await?;
    for tag_id in &next_tag_ids {
        let relation_id = note_tag_id(&id, tag_id);
        let inserted = sqlx::query(
            "INSERT INTO note_tags(id,user_id,note_id,tag_id,created_at,created_by,updated_by)
             VALUES($1,$2,$3,$4,$5,$2,$2)
             ON CONFLICT(user_id,note_id,tag_id) DO NOTHING",
        )
        .bind(&relation_id)
        .bind(user_id)
        .bind(&id)
        .bind(tag_id)
        .bind(&now)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if inserted > 0 {
            let relation_payload = fetch_note_tag(&mut tx, user_id, &relation_id).await?;
            append_change(
                &mut tx,
                user_id,
                "note_tag",
                &relation_id,
                "upsert",
                ChangePayload::NoteTag(relation_payload),
            )
            .await?;
        }
    }
    let note: Note = sqlx::query_as(&format!(
        "SELECT {NOTE_COLUMNS} FROM notes WHERE id=$1 AND user_id=$2"
    ))
    .bind(&id)
    .bind(user_id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    notify(&state, user_id, &id, "upsert");
    Ok(Json(note))
}
