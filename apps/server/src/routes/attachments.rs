use crate::error::AppError;
use crate::middleware::AuthUser;
use crate::models::AttachmentRecord;
use crate::routes::billing::ensure_attachment_quota;
use crate::routes::sync::{append_change, ChangePayload};
use crate::AppState;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use uuid::Uuid;

const MAX_ATTACHMENT_SIZE: usize = 20 * 1024 * 1024;

pub async fn upload(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<AttachmentRecord>, AppError> {
    if body.is_empty() || body.len() > MAX_ATTACHMENT_SIZE {
        return Err(AppError::BadRequest(
            "Attachment must be between 1 byte and 20 MB".into(),
        ));
    }
    let existing_size: i64 = sqlx::query_scalar(
        "SELECT COALESCE((SELECT size FROM attachments WHERE user_id = $1 AND id = $2), 0)",
    )
    .bind(user_id)
    .bind(&id)
    .fetch_one(state.db.inner())
    .await?;
    let additional_bytes = (body.len() as i64 - existing_size).max(0);
    ensure_attachment_quota(&state, user_id, additional_bytes).await?;
    let actual_id = format!("{:x}", Sha256::digest(&body));
    if actual_id != id.to_ascii_lowercase() {
        return Err(AppError::BadRequest(
            "Attachment hash does not match its ID".into(),
        ));
    }
    let mime_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .filter(|value| value.starts_with("image/"))
        .unwrap_or("application/octet-stream")
        .to_string();
    let filer_url = format!(
        "{}/quicknote/{}/{}",
        state.config.seaweedfs_filer.trim_end_matches('/'),
        user_id,
        id
    );
    let response = state
        .http
        .put(&filer_url)
        .header(header::CONTENT_TYPE, &mime_type)
        .body(body.clone())
        .send()
        .await
        .map_err(|error| AppError::Internal(format!("Object storage upload failed: {error}")))?;
    if !response.status().is_success() {
        return Err(AppError::Internal(format!(
            "Object storage upload failed with {}",
            response.status()
        )));
    }

    let record = AttachmentRecord {
        id: id.clone(),
        relative_path: format!("{id}.bin"),
        mime_type,
        size: body.len() as i64,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    let mut tx = state.db.inner().begin().await?;
    let existed: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM attachments WHERE user_id=$1 AND id=$2)")
            .bind(user_id)
            .bind(&id)
            .fetch_one(&mut *tx)
            .await?;
    sqlx::query(
        "INSERT INTO attachments
            (user_id,id,relative_path,mime_type,size,created_at,updated_at,created_by,updated_by)
         VALUES ($1,$2,$3,$4,$5,$6,$6,$1,$1)
         ON CONFLICT (user_id,id) DO UPDATE SET
            mime_type=EXCLUDED.mime_type,
            size=EXCLUDED.size,
            updated_at=EXCLUDED.updated_at,
            updated_by=EXCLUDED.updated_by",
    )
    .bind(user_id)
    .bind(&record.id)
    .bind(&record.relative_path)
    .bind(&record.mime_type)
    .bind(record.size)
    .bind(&record.created_at)
    .execute(&mut *tx)
    .await?;
    if !existed {
        append_change(
            &mut tx,
            user_id,
            "attachment",
            &id,
            "upsert",
            ChangePayload::Attachment(record.clone()),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(Json(record))
}

pub async fn download(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let record: AttachmentRecord = sqlx::query_as("SELECT id,relative_path,mime_type,size,created_at FROM attachments WHERE user_id=$1 AND id=$2")
        .bind(user_id).bind(&id).fetch_optional(state.db.inner()).await?.ok_or(AppError::NotFound)?;
    let filer_url = format!(
        "{}/quicknote/{}/{}",
        state.config.seaweedfs_filer.trim_end_matches('/'),
        user_id,
        id
    );
    let response =
        state.http.get(filer_url).send().await.map_err(|error| {
            AppError::Internal(format!("Object storage download failed: {error}"))
        })?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(AppError::NotFound);
    }
    if !response.status().is_success() {
        return Err(AppError::Internal(format!(
            "Object storage download failed with {}",
            response.status()
        )));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| AppError::Internal(format!("Invalid object response: {error}")))?;
    if bytes.len() as i64 != record.size || format!("{:x}", Sha256::digest(&bytes)) != id {
        return Err(AppError::Internal(
            "Object storage integrity check failed".into(),
        ));
    }
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, record.mime_type),
            (
                header::CACHE_CONTROL,
                "private, max-age=31536000, immutable".into(),
            ),
        ],
        bytes,
    )
        .into_response())
}

pub async fn delete_attachment_object(
    state: &AppState,
    user_id: Uuid,
    id: &str,
) -> Result<(), AppError> {
    let filer_url = format!(
        "{}/quicknote/{}/{}",
        state.config.seaweedfs_filer.trim_end_matches('/'),
        user_id,
        id
    );
    let response =
        state.http.delete(filer_url).send().await.map_err(|error| {
            AppError::Internal(format!("Object storage delete failed: {error}"))
        })?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(());
    }
    if !response.status().is_success() {
        return Err(AppError::Internal(format!(
            "Object storage delete failed with {}",
            response.status()
        )));
    }
    Ok(())
}
