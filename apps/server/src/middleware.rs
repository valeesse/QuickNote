use crate::error::AppError;
use crate::AppState;
use axum::{extract::FromRequestParts, http::request::Parts};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

pub const SESSION_COOKIE_NAME: &str = "quicknote_session";

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,
    pub exp: usize,
    pub iat: usize,
}

pub struct AuthUser(pub Uuid);

impl FromRequestParts<Arc<AppState>> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer ").map(ToOwned::to_owned))
            .or_else(|| {
                parts
                    .headers
                    .get(axum::http::header::COOKIE)
                    .and_then(|value| value.to_str().ok())
                    .and_then(extract_session_cookie)
            })
            .ok_or(AppError::Auth)?;

        Ok(AuthUser(authenticate_token(&token, &state.config.jwt_secret)?))
    }
}

pub fn authenticate_token(token: &str, secret: &str) -> Result<Uuid, AppError> {
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|_| AppError::Auth)?;
    Ok(token_data.claims.sub)
}

pub fn create_token(user_id: Uuid, secret: &str) -> Result<String, AppError> {
    let now = chrono::Utc::now().timestamp() as usize;
    let claims = Claims {
        sub: user_id,
        exp: now + 86400 * 7, // 7 days
        iat: now,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| AppError::Internal(format!("Failed to create token: {e}")))
}

pub fn build_session_cookie(token: &str, secure: bool) -> String {
    let mut cookie = format!(
        "{SESSION_COOKIE_NAME}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}",
        86400 * 7
    );
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

pub fn clear_session_cookie(secure: bool) -> String {
    let mut cookie = format!(
        "{SESSION_COOKIE_NAME}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT"
    );
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

fn extract_session_cookie(header: &str) -> Option<String> {
    header.split(';').find_map(|segment| {
        let (name, value) = segment.trim().split_once('=')?;
        (name == SESSION_COOKIE_NAME).then(|| value.to_string())
    })
}
