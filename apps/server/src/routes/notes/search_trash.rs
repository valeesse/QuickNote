use super::*;

#[derive(serde::Deserialize)]
pub struct SearchQuery {
    pub q: String,
}

pub async fn search_notes(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Query(params): Query<SearchQuery>,
) -> Result<Json<Vec<NoteSummary>>, AppError> {
    if params.q.trim().is_empty() {
        return Ok(Json(vec![]));
    }
    let notes = sqlx::query_as(
        "SELECT n.id,n.title,LEFT(n.content,200) AS preview,n.is_pinned,n.created_at,n.updated_at,
                COALESCE(array_agg(t.name ORDER BY lower(t.name)) FILTER (WHERE t.id IS NOT NULL), ARRAY[]::TEXT[]) AS tags
         FROM notes n
         LEFT JOIN note_tags nt ON nt.user_id=n.user_id AND nt.note_id=n.id
         LEFT JOIN tags t ON t.user_id=n.user_id AND t.id=nt.tag_id AND t.is_deleted=false
         WHERE n.user_id=$1 AND n.is_deleted=false AND (
            n.search_vector @@ plainto_tsquery('simple',$2)
            OR lower(n.title || ' ' || n.search_text) LIKE '%' || lower($2) || '%'
         )
         GROUP BY n.id,n.title,n.content,n.is_pinned,n.created_at,n.updated_at,n.search_vector
         ORDER BY ts_rank(n.search_vector,plainto_tsquery('simple',$2)) DESC LIMIT 50",
    )
        .bind(user_id).bind(params.q).fetch_all(state.db.inner()).await?;
    Ok(Json(notes))
}

pub async fn list_deleted_notes(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
) -> Result<Json<Vec<NoteSummary>>, AppError> {
    let notes = sqlx::query_as(
        "SELECT n.id,n.title,LEFT(n.content,200) AS preview,n.is_pinned,n.created_at,n.updated_at,
                COALESCE(array_agg(t.name ORDER BY lower(t.name)) FILTER (WHERE t.id IS NOT NULL), ARRAY[]::TEXT[]) AS tags
         FROM notes n
         LEFT JOIN note_tags nt ON nt.user_id=n.user_id AND nt.note_id=n.id
         LEFT JOIN tags t ON t.user_id=n.user_id AND t.id=nt.tag_id AND t.is_deleted=false
         WHERE n.user_id=$1 AND n.is_deleted=true
         GROUP BY n.id,n.title,n.content,n.is_pinned,n.created_at,n.updated_at
         ORDER BY n.updated_at DESC",
    )
    .bind(user_id)
    .fetch_all(state.db.inner())
    .await?;
    Ok(Json(notes))
}

pub async fn purge_note(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
) -> Result<Json<bool>, AppError> {
    let mut tx = state.db.inner().begin().await?;
    let note_content: Option<String> = sqlx::query_scalar(
        "SELECT content FROM notes WHERE id=$1 AND user_id=$2 AND is_deleted=true",
    )
    .bind(&id)
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(note_content) = note_content else {
        tx.rollback().await?;
        return Ok(Json(false));
    };
    let version_contents: Vec<String> =
        sqlx::query_scalar("SELECT content FROM note_versions WHERE note_id=$1 AND user_id=$2")
            .bind(&id)
            .bind(user_id)
            .fetch_all(&mut *tx)
            .await?;
    let candidate_attachment_ids = collect_attachment_candidates(
        std::iter::once(note_content.as_str()).chain(version_contents.iter().map(String::as_str)),
    );
    let mut orphaned_attachments = Vec::new();
    for attachment_id in candidate_attachment_ids {
        if !attachment_is_still_referenced(&mut tx, user_id, &attachment_id, &id).await? {
            orphaned_attachments.push(attachment_id);
        }
    }

    sqlx::query("DELETE FROM note_versions WHERE note_id=$1 AND user_id=$2")
        .bind(&id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    for attachment_id in &orphaned_attachments {
        sqlx::query("DELETE FROM attachments WHERE user_id=$1 AND id=$2")
            .bind(user_id)
            .bind(attachment_id)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("DELETE FROM notes WHERE id=$1 AND user_id=$2")
        .bind(&id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    for attachment_id in &orphaned_attachments {
        if let Err(error) = delete_attachment_object(&state, user_id, attachment_id).await {
            tracing::warn!(
                user_id = %user_id,
                attachment_id,
                error = %error,
                "attachment object cleanup failed after note purge"
            );
        }
    }
    notify(&state, user_id, &id, "delete");
    Ok(Json(true))
}
