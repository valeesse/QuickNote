use crate::error::AppError;
use crate::middleware::AuthUser;
use crate::models::{CloudChange, PullRequest, PullResponse, PushRequest, PushResponse, SyncEnvelope, SyncEvent};
use crate::AppState;
use axum::extract::State;
use axum::response::sse::{Event, Sse};
use axum::Json;
use futures::stream::Stream;
use std::convert::Infallible;
use std::sync::Arc;
use tokio_stream::StreamExt as _;

pub async fn pull(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Json(req): Json<PullRequest>,
) -> Result<Json<PullResponse>, AppError> {
    let changes: Vec<CloudChange> = sqlx::query_as(
        "SELECT * FROM cloud_changes WHERE user_id = $1 AND seq > $2 ORDER BY seq ASC LIMIT 500",
    )
    .bind(user_id)
    .bind(req.since_seq)
    .fetch_all(state.db.inner())
    .await?;

    let server_seq = changes.last().map(|c| c.seq).unwrap_or(req.since_seq);

    let envelopes: Vec<SyncEnvelope> = changes
        .into_iter()
        .filter_map(|change| {
            serde_json::from_value(change.envelope).ok()
        })
        .collect();

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
    let mut accepted = 0;
    let mut conflicts = 0;

    for envelope in &req.envelopes {
        let envelope_json = serde_json::to_value(envelope)
            .map_err(|e| AppError::Internal(format!("Serialize error: {e}")))?;

        let result = sqlx::query(
            "INSERT INTO cloud_changes (user_id, seq, entity_type, entity_id, operation, envelope)
             VALUES ($1, nextval('cloud_changes_seq'), $2, $3, $4, $5)
             ON CONFLICT DO NOTHING",
        )
        .bind(user_id)
        .bind(&envelope.entity_type)
        .bind(&envelope.entity_id)
        .bind(&envelope.operation)
        .bind(&envelope_json)
        .execute(state.db.inner())
        .await?;

        if result.rows_affected() > 0 {
            accepted += 1;
        } else {
            conflicts += 1;
        }
    }

    if accepted > 0 {
        let _ = state.event_tx.send(SyncEvent {
            user_id,
            entity_type: "batch".to_string(),
            entity_id: String::new(),
            operation: "push".to_string(),
        });
    }

    Ok(Json(PushResponse { accepted, conflicts }))
}

pub async fn events_sse(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.event_tx.subscribe();

    let stream = tokio_stream::wrappers::BroadcastStream::new(rx)
        .filter_map(move |result| {
            match result {
                Ok(event) if event.user_id == user_id => {
                    let data = serde_json::json!({
                        "entity_type": event.entity_type,
                        "entity_id": event.entity_id,
                        "operation": event.operation,
                    });
                    Some(Ok(Event::default().data(data.to_string())))
                }
                _ => None,
            }
        });

    Sse::new(stream)
}
