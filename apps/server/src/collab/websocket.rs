use super::store::{load_note_state, persist_projection, persist_update, ProjectionResult};
use super::CollabEvent;
use crate::error::AppError;
use crate::middleware::{authenticate_token, extract_session_cookie};
use crate::AppState;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct CollabQuery {
    token: Option<String>,
    client_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientMessage {
    Projection {
        projection_id: Uuid,
        html: String,
        state_vector: String,
    },
    Awareness {
        update: String,
    },
}

const MAX_YJS_UPDATE_BYTES: usize = 2 * 1024 * 1024;
const MAX_PROJECTION_BYTES: usize = 10 * 1024 * 1024;
const UPDATE_MAGIC: &[u8; 4] = b"QNUP";

pub async fn note_socket(
    State(state): State<Arc<AppState>>,
    Path(note_id): Path<String>,
    Query(query): Query<CollabQuery>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, AppError> {
    let token = query
        .token
        .or_else(|| {
            headers
                .get(axum::http::header::COOKIE)
                .and_then(|value| value.to_str().ok())
                .and_then(extract_session_cookie)
        })
        .ok_or(AppError::Auth)?;
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
    let Ok(Some((initial, version, bootstrap))) = load_note_state(&state, user_id, &note_id).await
    else {
        return;
    };
    let (mut sender, mut incoming) = socket.split();
    let sync = json_message(serde_json::json!({
        "type": "sync", "bootstrap": bootstrap, "state_version": version
    }));
    if sender.send(sync).await.is_err()
        || sender.send(Message::Binary(initial.into())).await.is_err()
    {
        return;
    }

    let (out_tx, mut out_rx) = mpsc::channel::<Message>(256);
    let writer = tokio::spawn(async move {
        while let Some(message) = out_rx.recv().await {
            if sender.send(message).await.is_err() {
                break;
            }
        }
    });
    let mut receiver = state.collab_hub.subscribe(&room);
    let broadcast_tx = out_tx.clone();
    let outbound = tokio::spawn(async move {
        while let Ok(event) = receiver.recv().await {
            let message = match event {
                CollabEvent::Update(update) => Message::Binary(update.into()),
                CollabEvent::Awareness(update) => {
                    json_message(serde_json::json!({ "type": "awareness", "update": update }))
                }
            };
            if broadcast_tx.send(message).await.is_err() {
                break;
            }
        }
    });

    while let Some(Ok(message)) = incoming.next().await {
        let result = match message {
            Message::Binary(frame) => {
                handle_update(
                    &state, user_id, &note_id, &client_id, &room, &out_tx, &frame,
                )
                .await
            }
            Message::Text(text) => {
                handle_text(&state, user_id, &note_id, &out_tx, text.as_str()).await
            }
            Message::Close(_) => break,
            _ => Ok(()),
        };
        if let Err(error) = result {
            tracing::warn!(%user_id, note_id = %note_id, error = %error, "collaboration message failed");
            let _ = out_tx
                .send(json_message(serde_json::json!({ "type": "error" })))
                .await;
            break;
        }
    }
    outbound.abort();
    writer.abort();
}

async fn handle_update(
    state: &AppState,
    user_id: Uuid,
    note_id: &str,
    client_id: &str,
    room: &str,
    out: &mpsc::Sender<Message>,
    frame: &[u8],
) -> Result<(), AppError> {
    let (update_id, update) = decode_update_frame(frame)?;
    if update.len() > MAX_YJS_UPDATE_BYTES {
        return Err(AppError::BadRequest("Yjs update is too large".into()));
    }
    let version = persist_update(state, user_id, note_id, client_id, update_id, update).await?;
    state
        .collab_hub
        .broadcast(room, CollabEvent::Update(update.to_vec()));
    out.send(json_message(serde_json::json!({
        "type": "ack", "update_id": update_id, "state_version": version
    })))
    .await
    .map_err(|_| AppError::Internal("Collaboration socket closed".into()))
}

async fn handle_text(
    state: &AppState,
    user_id: Uuid,
    note_id: &str,
    out: &mpsc::Sender<Message>,
    text: &str,
) -> Result<(), AppError> {
    let Ok(message) = serde_json::from_str(text) else {
        return Ok(());
    };
    let ClientMessage::Projection {
        projection_id,
        html,
        state_vector,
    } = message
    else {
        let ClientMessage::Awareness { update } = message else {
            unreachable!()
        };
        if update.len() > 128 * 1024 {
            return Err(AppError::BadRequest("Awareness update is too large".into()));
        }
        state.collab_hub.broadcast(
            &format!("{user_id}:{note_id}"),
            CollabEvent::Awareness(update),
        );
        return Ok(());
    };
    if html.len() > MAX_PROJECTION_BYTES {
        return Err(AppError::BadRequest("Projection is too large".into()));
    }
    let (message_type, version) =
        match persist_projection(state, user_id, note_id, html, &state_vector).await? {
            ProjectionResult::Accepted(version) => ("projection_ack", version),
            ProjectionResult::Stale(version) => ("projection_rejected", version),
        };
    out.send(json_message(serde_json::json!({
        "type": message_type, "projection_id": projection_id, "state_version": version
    })))
    .await
    .map_err(|_| AppError::Internal("Collaboration socket closed".into()))
}

fn decode_update_frame(frame: &[u8]) -> Result<(Uuid, &[u8]), AppError> {
    if frame.len() < 20 || &frame[..4] != UPDATE_MAGIC {
        return Err(AppError::BadRequest(
            "Unsupported collaboration frame".into(),
        ));
    }
    let update_id = Uuid::from_slice(&frame[4..20])
        .map_err(|_| AppError::BadRequest("Invalid update identifier".into()))?;
    Ok((update_id, &frame[20..]))
}

fn json_message(value: serde_json::Value) -> Message {
    Message::Text(value.to_string().into())
}
