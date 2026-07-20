use super::*;

pub async fn pull(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Json(req): Json<PullRequest>,
) -> Result<Json<PullResponse>, AppError> {
    if req.since_seq < 0 {
        return Err(AppError::BadRequest(
            "since_seq must be non-negative".into(),
        ));
    }

    let changes: Vec<CloudChange> = sqlx::query_as(
        "SELECT * FROM cloud_changes WHERE user_id = $1 AND seq > $2 ORDER BY seq ASC LIMIT 500",
    )
    .bind(user_id)
    .bind(req.since_seq)
    .fetch_all(state.db.inner())
    .await?;

    let server_seq = changes
        .last()
        .map(|change| change.seq)
        .unwrap_or(req.since_seq);
    let envelopes = changes
        .into_iter()
        .map(|change| {
            serde_json::from_value(change.envelope).map_err(|error| {
                AppError::Internal(format!(
                    "Invalid stored sync envelope at seq {}: {error}",
                    change.seq
                ))
            })
        })
        .collect::<Result<Vec<SyncEnvelope>, AppError>>()?;

    Ok(Json(PullResponse {
        envelopes,
        server_seq,
    }))
}

pub async fn push(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Json(req): Json<PushRequest>,
) -> Result<Json<PushResponse>, AppError> {
    if req.envelopes.len() > 500 {
        return Err(AppError::BadRequest(
            "A push is limited to 500 changes".into(),
        ));
    }

    let mut accepted = 0;
    let mut conflicts = 0;
    let mut acknowledged_sequences = Vec::with_capacity(req.envelopes.len());

    for envelope in &req.envelopes {
        validate_envelope(envelope)?;
        ensure_device_allowed(&state, user_id, &envelope.device_id).await?;
        let mut tx = state.db.inner().begin().await?;
        let (causal_version, relation) = next_causal_version(
            &mut tx,
            user_id,
            &envelope.entity_type,
            &envelope.entity_id,
            envelope.causal_version.as_ref(),
        )
        .await?;
        let duplicate: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM cloud_changes WHERE user_id=$1 AND source_device=$2 AND source_seq=$3)",
        ).bind(user_id).bind(&envelope.device_id).bind(envelope.seq).fetch_one(&mut *tx).await?;
        if duplicate {
            acknowledged_sequences.push(envelope.seq);
            conflicts += 1;
            tx.rollback().await?;
            continue;
        }
        if relation == CausalRelation::Dominates {
            acknowledged_sequences.push(envelope.seq);
            conflicts += 1;
            tx.rollback().await?;
            continue;
        }
        let mut accepted_envelope = envelope.clone();
        if relation == CausalRelation::Concurrent && envelope.entity_type == "note" {
            let merged =
                merge_concurrent_yjs_note(&mut tx, user_id, &mut accepted_envelope).await?;
            if !merged {
                preserve_note_conflict(
                    &mut tx,
                    user_id,
                    &envelope.entity_id,
                    &envelope.device_id,
                    envelope.seq,
                )
                .await?;
                conflicts += 1;
            }
        }
        let (server_seq,): (i64,) = sqlx::query_as("SELECT nextval('cloud_changes_seq')")
            .fetch_one(&mut *tx)
            .await?;
        let mut canonical = accepted_envelope.clone();
        canonical.device_id = "cloud".to_string();
        canonical.seq = server_seq;
        canonical.causal_version = Some(causal_version);
        let envelope_json = serde_json::to_value(&canonical)
            .map_err(|error| AppError::Internal(format!("Serialize error: {error}")))?;

        let result = sqlx::query(
            "INSERT INTO cloud_changes
                (user_id, seq, entity_type, entity_id, operation, source_device, source_seq, envelope, created_by, updated_by)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $1, $1)
             ON CONFLICT (user_id, source_device, source_seq) DO NOTHING",
        )
        .bind(user_id)
        .bind(server_seq)
        .bind(&envelope.entity_type)
        .bind(&envelope.entity_id)
        .bind(&envelope.operation)
        .bind(&envelope.device_id)
        .bind(envelope.seq)
        .bind(&envelope_json)
        .execute(&mut *tx)
        .await?;

        debug_assert_eq!(result.rows_affected(), 1);

        apply_to_canonical(&mut tx, user_id, &accepted_envelope).await?;
        touch_sync_cursor(&mut tx, user_id, &envelope.device_id, envelope.seq).await?;
        tx.commit().await?;
        if let Some(note) = accepted_envelope.note.as_ref() {
            if let Some(yjs_state) = note.yjs_state.as_ref() {
                state.collab_hub.broadcast(
                    &format!("{user_id}:{}", note.id),
                    crate::collab::CollabEvent::Update(yjs_state.clone()),
                );
            }
        }
        acknowledged_sequences.push(envelope.seq);
        accepted += 1;
    }

    if accepted > 0 {
        let _ = state.event_tx.send(SyncEvent {
            user_id,
            entity_type: "batch".to_string(),
            entity_id: String::new(),
            operation: "push".to_string(),
        });
    }

    Ok(Json(PushResponse {
        accepted,
        conflicts,
        acknowledged_sequences,
    }))
}
