use crate::error::AppError;
use crate::middleware::authenticate_token;
use crate::AppState;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;
use yrs::updates::decoder::Decode;
use yrs::{Doc, ReadTxn, StateVector, Transact, Update};

#[derive(Debug, Deserialize)]
pub struct CollabQuery {
    token: Option<String>,
    client_id: Option<String>,
}

pub async fn note_socket(
    State(state): State<Arc<AppState>>,
    Path(note_id): Path<String>,
    Query(query): Query<CollabQuery>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, AppError> {
    let token = query.token.ok_or(AppError::Auth)?;
    let user_id = authenticate_token(&token, &state.config.jwt_secret)?;
    let client_id = query.client_id.unwrap_or_else(|| "websocket".to_string());

    Ok(ws.on_upgrade(move |socket| handle_socket(state, user_id, note_id, client_id, socket)))
}

async fn handle_socket(
    state: Arc<AppState>,
    user_id: Uuid,
    note_id: String,
    client_id: String,
    socket: WebSocket,
) {
    let room = format!("{user_id}:{note_id}");
    let mut receiver = state.collab_hub.subscribe(&room);
    let (mut sender, mut incoming) = socket.split();

    if let Ok(Some(state_update)) = load_note_state(&state, user_id, &note_id).await {
        if sender
            .send(Message::Binary(state_update.into()))
            .await
            .is_err()
        {
            return;
        }
    }

    let outbound = tokio::spawn(async move {
        while let Ok(update) = receiver.recv().await {
            if sender.send(Message::Binary(update.into())).await.is_err() {
                break;
            }
        }
    });

    while let Some(Ok(message)) = incoming.next().await {
        let Message::Binary(update) = message else {
            if matches!(message, Message::Close(_)) {
                break;
            }
            continue;
        };
        let update = update.to_vec();
        match persist_update(&state, user_id, &note_id, &client_id, &update).await {
            Ok(()) => state.collab_hub.broadcast(&room, update),
            Err(error) => {
                tracing::warn!(%user_id, note_id = %note_id, error = %error, "failed to persist yjs update");
                break;
            }
        }
    }

    outbound.abort();
}

async fn load_note_state(
    state: &AppState,
    user_id: Uuid,
    note_id: &str,
) -> Result<Option<Vec<u8>>, AppError> {
    sqlx::query_scalar("SELECT yjs_state FROM notes WHERE user_id=$1 AND id=$2 AND is_deleted=false")
        .bind(user_id)
        .bind(note_id)
        .fetch_optional(state.db.inner())
        .await
        .map_err(AppError::from)
}

async fn persist_update(
    state: &AppState,
    user_id: Uuid,
    note_id: &str,
    client_id: &str,
    update: &[u8],
) -> Result<(), AppError> {
    let mut tx = state.db.inner().begin().await?;
    let existing: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT yjs_state FROM notes WHERE user_id=$1 AND id=$2 AND is_deleted=false FOR UPDATE",
    )
    .bind(user_id)
    .bind(note_id)
    .fetch_optional(&mut *tx)
    .await?;

    let Some(compacted) = merge_update(existing.as_deref(), update)? else {
        tx.rollback().await?;
        return Err(AppError::NotFound);
    };

    sqlx::query(
        "INSERT INTO yjs_updates(user_id, note_id, update, source_client_id)
         VALUES ($1,$2,$3,$4)",
    )
    .bind(user_id)
    .bind(note_id)
    .bind(update)
    .bind(client_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE notes
         SET yjs_state=$3,yjs_state_version=yjs_state_version+1,updated_at=$4,updated_by=$1
         WHERE user_id=$1 AND id=$2",
    )
    .bind(user_id)
    .bind(note_id)
    .bind(compacted)
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

fn merge_update(existing: Option<&[u8]>, incoming: &[u8]) -> Result<Option<Vec<u8>>, AppError> {
    let incoming = Update::decode_v1(incoming)
        .map_err(|error| AppError::BadRequest(format!("Invalid Yjs update: {error}")))?;
    let doc = Doc::new();
    if let Some(existing) = existing {
        let existing = Update::decode_v1(existing)
            .map_err(|error| AppError::Internal(format!("Invalid stored Yjs state: {error}")))?;
        doc.transact_mut()
            .apply_update(existing)
            .map_err(|error| AppError::Internal(format!("Apply stored Yjs state failed: {error}")))?;
    }

    doc.transact_mut()
        .apply_update(incoming)
        .map_err(|error| AppError::BadRequest(format!("Apply Yjs update failed: {error}")))?;

    let compacted = doc
        .transact()
        .encode_state_as_update_v1(&StateVector::default());
    Ok(Some(compacted))
}
