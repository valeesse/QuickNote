use super::*;

pub async fn create_note(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Json(req): Json<CreateNoteRequest>,
) -> Result<Json<Note>, AppError> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let title = extract_title(&req.content);
    let search_text = extract_plain_text(&req.content);
    let mut tx = state.db.inner().begin().await?;
    let sort_order: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM notes WHERE user_id=$1 AND is_deleted=false AND is_pinned=false",
    )
    .bind(user_id)
    .fetch_one(&mut *tx)
    .await?;
    let query = format!(
        "INSERT INTO notes (id,user_id,title,content,search_text,is_pinned,sort_order,created_at,updated_at,version,is_deleted,created_by,updated_by)
         VALUES ($1,$2,$3,$4,$5,false,$6,$7,$7,1,false,$2,$2)
         RETURNING {NOTE_COLUMNS}"
    );
    let note: Note = sqlx::query_as(&query)
        .bind(&id)
        .bind(user_id)
        .bind(title)
        .bind(req.content)
        .bind(search_text)
        .bind(sort_order)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;
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

pub async fn get_note(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
) -> Result<Json<Note>, AppError> {
    let query = format!("SELECT {NOTE_COLUMNS} FROM notes WHERE id=$1 AND user_id=$2");
    let note = sqlx::query_as(&query)
        .bind(id)
        .bind(user_id)
        .fetch_optional(state.db.inner())
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(note))
}

pub async fn list_notes(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Query(params): Query<ListNotesQuery>,
) -> Result<Json<Vec<NoteSummary>>, AppError> {
    let limit = params.limit.unwrap_or(200).clamp(1, 500);
    let notes = if let Some(tag) = params
        .tag
        .as_deref()
        .map(normalize_tag_name)
        .filter(|tag| !tag.is_empty())
    {
        sqlx::query_as(
            "SELECT n.id,n.title,LEFT(n.content,200) AS preview,n.is_pinned,n.created_at,n.updated_at,
                    COALESCE(array_agg(all_tags.name ORDER BY lower(all_tags.name)) FILTER (WHERE all_tags.id IS NOT NULL), ARRAY[]::TEXT[]) AS tags
             FROM notes n
             JOIN note_tags filter_nt ON filter_nt.user_id=n.user_id AND filter_nt.note_id=n.id
             JOIN tags filter_t ON filter_t.user_id=n.user_id AND filter_t.id=filter_nt.tag_id
             LEFT JOIN note_tags all_nt ON all_nt.user_id=n.user_id AND all_nt.note_id=n.id
             LEFT JOIN tags all_tags ON all_tags.user_id=n.user_id AND all_tags.id=all_nt.tag_id AND all_tags.is_deleted=false
             WHERE n.user_id=$1 AND n.is_deleted=false AND filter_t.is_deleted=false AND filter_t.normalized_name=$2
             GROUP BY n.id,n.title,n.content,n.is_pinned,n.created_at,n.updated_at,n.sort_order
             ORDER BY n.is_pinned DESC,n.sort_order DESC,n.updated_at DESC LIMIT $3",
        )
        .bind(user_id)
        .bind(tag)
        .bind(limit)
        .fetch_all(state.db.inner())
        .await?
    } else {
        sqlx::query_as(
            "SELECT n.id,n.title,LEFT(n.content,200) AS preview,n.is_pinned,n.created_at,n.updated_at,
                    COALESCE(array_agg(t.name ORDER BY lower(t.name)) FILTER (WHERE t.id IS NOT NULL), ARRAY[]::TEXT[]) AS tags
             FROM notes n
             LEFT JOIN note_tags nt ON nt.user_id=n.user_id AND nt.note_id=n.id
             LEFT JOIN tags t ON t.user_id=n.user_id AND t.id=nt.tag_id AND t.is_deleted=false
             WHERE n.user_id=$1 AND n.is_deleted=false
             GROUP BY n.id,n.title,n.content,n.is_pinned,n.created_at,n.updated_at,n.sort_order
             ORDER BY n.is_pinned DESC,n.sort_order DESC,n.updated_at DESC LIMIT $2",
        )
        .bind(user_id)
        .bind(limit)
        .fetch_all(state.db.inner())
        .await?
    };
    Ok(Json(notes))
}
