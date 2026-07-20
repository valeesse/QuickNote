use super::*;

pub async fn reorder_notes(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Json(req): Json<ReorderNotesRequest>,
) -> Result<Json<bool>, AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    let mut tx = state.db.inner().begin().await?;
    let len = req.ids.len() as i64;
    for (index, id) in req.ids.iter().enumerate() {
        let query = format!(
            "UPDATE notes
             SET is_pinned=$3,sort_order=$4,updated_at=$5,updated_by=$2
             WHERE id=$1 AND user_id=$2 AND is_deleted=false
             RETURNING {NOTE_COLUMNS}"
        );
        let note: Option<Note> = sqlx::query_as(&query)
            .bind(id)
            .bind(user_id)
            .bind(req.is_pinned)
            .bind(len - index as i64)
            .bind(&now)
            .fetch_optional(&mut *tx)
            .await?;
        if let Some(note) = note {
            append_change(
                &mut tx,
                user_id,
                "note",
                id,
                "upsert",
                ChangePayload::Note(note),
            )
            .await?;
            notify(&state, user_id, id, "upsert");
        }
    }
    tx.commit().await?;
    Ok(Json(true))
}

pub async fn update_note(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateNoteRequest>,
) -> Result<Json<Note>, AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    let title = extract_title(&req.content);
    let search_text = extract_plain_text(&req.content);
    let version_cutoff = version_history_cutoff(&state, user_id).await?;
    let mut tx = state.db.inner().begin().await?;

    // Snapshot current version before updating, then prune by plan policy.
    let existing: Option<Note> = sqlx::query_as(&format!(
        "SELECT {NOTE_COLUMNS} FROM notes WHERE id=$1 AND user_id=$2 AND is_deleted=false"
    ))
    .bind(&id)
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(old) = existing {
        sqlx::query(
            "INSERT INTO note_versions
                (note_id,user_id,title,content,version,created_at,updated_at,is_pinned,created_by,updated_by)
             VALUES ($1,$2,$3,$4,$5,$6,$6,false,$2,$2)",
        )
            .bind(&id)
            .bind(user_id)
            .bind(&old.title)
            .bind(&old.content)
            .bind(old.version)
            .bind(&old.updated_at)
            .execute(&mut *tx)
            .await?;
        if let Some(cutoff) = version_cutoff.as_deref() {
            sqlx::query(
                "DELETE FROM note_versions
                 WHERE user_id=$1 AND is_pinned=false AND created_at < $2",
            )
            .bind(user_id)
            .bind(cutoff)
            .execute(&mut *tx)
            .await?;
        }
    }

    let query = format!(
        "UPDATE notes
         SET content=$3,title=$4,search_text=$5,updated_at=$6,version=version+1,updated_by=$2
         WHERE id=$1 AND user_id=$2 AND is_deleted=false
         RETURNING {NOTE_COLUMNS}"
    );
    let note: Note = sqlx::query_as(&query)
        .bind(&id)
        .bind(user_id)
        .bind(req.content)
        .bind(title)
        .bind(search_text)
        .bind(now)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound)?;
    append_change(
        &mut tx,
        user_id,
        "note",
        &id,
        "upsert",
        ChangePayload::Note(note.clone()),
    )
    .await?;
    tx.commit().await?;
    notify(&state, user_id, &id, "upsert");
    Ok(Json(note))
}

pub async fn delete_note(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
) -> Result<Json<bool>, AppError> {
    mutate_flag(
        &state,
        user_id,
        &id,
        "UPDATE notes SET is_deleted=true,updated_at=$3,updated_by=$4 WHERE id=$1 AND user_id=$2 AND is_deleted=false",
        "delete",
    )
    .await
}

pub async fn restore_note(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
) -> Result<Json<bool>, AppError> {
    mutate_flag(
        &state,
        user_id,
        &id,
        "UPDATE notes SET is_deleted=false,updated_at=$3,updated_by=$4 WHERE id=$1 AND user_id=$2 AND is_deleted=true",
        "upsert",
    )
    .await
}

pub async fn toggle_pin(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
) -> Result<Json<bool>, AppError> {
    let mut tx = state.db.inner().begin().await?;
    let now = chrono::Utc::now().to_rfc3339();
    // Read current pin state to compute target sort_order
    let current: Option<(bool, i64)> = sqlx::query_as(
        "SELECT is_pinned, sort_order FROM notes WHERE id=$1 AND user_id=$2 AND is_deleted=false",
    )
    .bind(&id)
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some((is_pinned, _)) = current {
        let target_pinned = !is_pinned;
        let next_sort_order: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM notes WHERE user_id=$1 AND is_deleted=false AND is_pinned=$2",
        )
        .bind(user_id)
        .bind(target_pinned)
        .fetch_one(&mut *tx)
        .await?;
        let query = format!(
            "UPDATE notes
             SET is_pinned=$3,sort_order=$4,updated_at=$5,updated_by=$2
             WHERE id=$1 AND user_id=$2 AND is_deleted=false
             RETURNING {NOTE_COLUMNS}"
        );
        let note: Option<Note> = sqlx::query_as(&query)
            .bind(&id)
            .bind(user_id)
            .bind(target_pinned)
            .bind(next_sort_order)
            .bind(&now)
            .fetch_optional(&mut *tx)
            .await?;
        if let Some(note) = note {
            append_change(
                &mut tx,
                user_id,
                "note",
                &id,
                "upsert",
                ChangePayload::Note(note),
            )
            .await?;
            tx.commit().await?;
            notify(&state, user_id, &id, "upsert");
            return Ok(Json(true));
        }
    }
    tx.rollback().await?;
    Ok(Json(false))
}

async fn mutate_flag(
    state: &Arc<AppState>,
    user_id: Uuid,
    id: &str,
    sql: &str,
    operation: &str,
) -> Result<Json<bool>, AppError> {
    let mut tx = state.db.inner().begin().await?;
    let now = chrono::Utc::now().to_rfc3339();
    let result = sqlx::query(sql)
        .bind(id)
        .bind(user_id)
        .bind(&now)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    if result.rows_affected() == 0 {
        tx.rollback().await?;
        return Ok(Json(false));
    }
    let note = if operation == "upsert" {
        let query = format!("SELECT {NOTE_COLUMNS} FROM notes WHERE id=$1 AND user_id=$2");
        Some(
            sqlx::query_as(&query)
                .bind(id)
                .bind(user_id)
                .fetch_one(&mut *tx)
                .await?,
        )
    } else {
        None
    };
    let payload = note.map(ChangePayload::Note).unwrap_or(ChangePayload::None);
    append_change(&mut tx, user_id, "note", id, operation, payload).await?;
    if operation == "delete" {
        let tag_ids: Vec<String> =
            sqlx::query_scalar("SELECT tag_id FROM note_tags WHERE user_id=$1 AND note_id=$2")
                .bind(user_id)
                .bind(id)
                .fetch_all(&mut *tx)
                .await?;
        delete_unused_tags(&mut tx, user_id, &tag_ids, &now).await?;
    } else {
        restore_note_tags(&mut tx, user_id, id, &now).await?;
    }
    tx.commit().await?;
    notify(state, user_id, id, operation);
    Ok(Json(true))
}
