use super::*;

pub async fn list_versions(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
) -> Result<Json<Vec<NoteVersion>>, AppError> {
    let versions = if let Some(cutoff) = version_history_cutoff(&state, user_id).await? {
        sqlx::query_as(
            "SELECT id,note_id,title,content,version,created_at,is_pinned
             FROM note_versions
             WHERE note_id=$1 AND user_id=$2 AND created_at >= $3
             ORDER BY created_at DESC",
        )
        .bind(&id)
        .bind(user_id)
        .bind(cutoff)
        .fetch_all(state.db.inner())
        .await?
    } else {
        sqlx::query_as(
            "SELECT id,note_id,title,content,version,created_at,is_pinned
             FROM note_versions
             WHERE note_id=$1 AND user_id=$2
             ORDER BY created_at DESC",
        )
        .bind(&id)
        .bind(user_id)
        .fetch_all(state.db.inner())
        .await?
    };
    Ok(Json(versions))
}

pub async fn restore_version(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Path((note_id, version_id)): Path<(String, i64)>,
) -> Result<Json<Note>, AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    let mut tx = state.db.inner().begin().await?;

    // Fetch the version content
    let ver: Option<NoteVersion> = sqlx::query_as(
        "SELECT id,note_id,title,content,version,created_at,is_pinned FROM note_versions WHERE id=$1 AND note_id=$2 AND user_id=$3",
    )
    .bind(version_id)
    .bind(&note_id)
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;
    let ver = ver.ok_or(AppError::NotFound)?;
    if let Some(cutoff) = version_history_cutoff(&state, user_id).await? {
        if ver.created_at < cutoff {
            return Err(AppError::BadRequest(
                "This version is outside the current plan's history window.".into(),
            ));
        }
    }

    // Update the note with version content
    let query = format!(
        "UPDATE notes
         SET content=$3,title=$4,updated_at=$5,version=version+1,updated_by=$2
         WHERE id=$1 AND user_id=$2 AND is_deleted=false
         RETURNING {NOTE_COLUMNS}"
    );
    let note: Note = sqlx::query_as(&query)
        .bind(&note_id)
        .bind(user_id)
        .bind(&ver.content)
        .bind(&ver.title)
        .bind(now)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound)?;

    append_change(
        &mut tx,
        user_id,
        "note",
        &note_id,
        "upsert",
        ChangePayload::Note(note.clone()),
    )
    .await?;
    tx.commit().await?;
    notify(&state, user_id, &note_id, "upsert");
    Ok(Json(note))
}

pub async fn toggle_version_pin(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Path(version_id): Path<i64>,
) -> Result<Json<bool>, AppError> {
    let result = sqlx::query(
        "UPDATE note_versions
             SET is_pinned=NOT is_pinned, updated_at=$3, updated_by=$2
             WHERE id=$1 AND user_id=$2",
    )
    .bind(version_id)
    .bind(user_id)
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(state.db.inner())
    .await?;
    Ok(Json(result.rows_affected() > 0))
}

pub async fn delete_version(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Path(version_id): Path<i64>,
) -> Result<Json<bool>, AppError> {
    let result = sqlx::query("DELETE FROM note_versions WHERE id=$1 AND user_id=$2")
        .bind(version_id)
        .bind(user_id)
        .execute(state.db.inner())
        .await?;
    Ok(Json(result.rows_affected() > 0))
}

pub async fn clear_versions(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Path(note_id): Path<String>,
) -> Result<Json<bool>, AppError> {
    let result = sqlx::query(
        "DELETE FROM note_versions WHERE note_id=$1 AND user_id=$2 AND is_pinned=false",
    )
    .bind(&note_id)
    .bind(user_id)
    .execute(state.db.inner())
    .await?;
    Ok(Json(result.rows_affected() > 0))
}
