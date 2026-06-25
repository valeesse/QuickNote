use crate::error::AppError;
use crate::middleware::{build_session_cookie, clear_session_cookie, create_token, AuthUser};
use crate::models::{AuthRequest, AuthResponse, User, UserResponse};
use crate::AppState;
use argon2::{
    password_hash::{rand_core::OsRng, SaltString},
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
};
use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use std::sync::Arc;
use uuid::Uuid;

pub async fn register(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<AuthRequest>,
) -> Result<Response, AppError> {
    let email = req.email.trim().to_lowercase();
    let client_ip = client_ip(&headers);
    enforce_rate_limit(&state, &client_ip, &email).await?;

    if !is_valid_email(&req.email) || req.password.len() < 10 {
        record_failure(&state, &client_ip, &email).await;
        return Err(AppError::BadRequest(
            "A valid email and a password of at least 10 characters are required".into(),
        ));
    }

    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(req.password.as_bytes(), &salt)
        .map_err(|e| AppError::Internal(format!("Hash error: {e}")))?
        .to_string();

    let user_id = Uuid::new_v4();

    let insert_result =
        sqlx::query("INSERT INTO users (id, email, password_hash) VALUES ($1, $2, $3)")
            .bind(user_id)
            .bind(&email)
            .bind(&password_hash)
            .execute(state.db.inner())
            .await;
    if let Err(e) = insert_result {
        record_failure(&state, &client_ip, &email).await;
        return Err(if let sqlx::Error::Database(ref db_err) = e {
            if db_err.constraint() == Some("users_email_key") {
                AppError::BadRequest("Email already registered".into())
            } else {
                AppError::Db(e)
            }
        } else {
            AppError::Db(e)
        });
    }

    clear_identity_failures(&state, &email).await;

    let token = create_token(user_id, &state.config.jwt_secret)?;
    let cookie_token = token.clone();
    Ok(with_session_cookie(
        &state,
        &cookie_token,
        Json(AuthResponse {
            token,
            user: UserResponse { id: user_id, email },
        }),
    ))
}

fn is_valid_email(value: &str) -> bool {
    let value = value.trim();
    let Some((local, domain)) = value.split_once('@') else {
        return false;
    };
    !local.is_empty() && domain.contains('.') && !domain.ends_with('.')
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<AuthRequest>,
) -> Result<Response, AppError> {
    let email = req.email.trim().to_lowercase();
    let client_ip = client_ip(&headers);
    enforce_rate_limit(&state, &client_ip, &email).await?;

    let user: Option<User> =
        sqlx::query_as("SELECT id, email, password_hash, created_at FROM users WHERE email = $1")
            .bind(&email)
            .fetch_optional(state.db.inner())
            .await?;
    let Some(user) = user else {
        record_failure(&state, &client_ip, &email).await;
        return Err(AppError::Auth);
    };

    let parsed_hash =
        PasswordHash::new(&user.password_hash).map_err(|e| AppError::Internal(e.to_string()))?;
    if Argon2::default()
        .verify_password(req.password.as_bytes(), &parsed_hash)
        .is_err()
    {
        record_failure(&state, &client_ip, &email).await;
        return Err(AppError::Auth);
    }

    clear_identity_failures(&state, &email).await;

    let token = create_token(user.id, &state.config.jwt_secret)?;
    let cookie_token = token.clone();
    Ok(with_session_cookie(
        &state,
        &cookie_token,
        Json(AuthResponse {
            token,
            user: UserResponse {
                id: user.id,
                email: user.email,
            },
        }),
    ))
}

pub async fn refresh(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
) -> Result<Response, AppError> {
    let token = create_token(user_id, &state.config.jwt_secret)?;
    Ok(with_session_cookie(
        &state,
        &token,
        Json(serde_json::json!({ "token": token })),
    ))
}

pub async fn me(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
) -> Result<Json<UserResponse>, AppError> {
    let user: User =
        sqlx::query_as("SELECT id, email, password_hash, created_at FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(state.db.inner())
            .await?
            .ok_or(AppError::Auth)?;
    Ok(Json(UserResponse {
        id: user.id,
        email: user.email,
    }))
}

pub async fn logout(State(state): State<Arc<AppState>>) -> Response {
    (
        StatusCode::OK,
        [(
            header::SET_COOKIE,
            clear_session_cookie(state.config.cookie_secure),
        )],
        Json(serde_json::json!({ "ok": true })),
    )
        .into_response()
}

fn with_session_cookie(
    state: &AppState,
    token: &str,
    payload: Json<impl serde::Serialize>,
) -> Response {
    (
        [(
            header::SET_COOKIE,
            build_session_cookie(token, state.config.cookie_secure),
        )],
        payload,
    )
        .into_response()
}

fn client_ip(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|value| value.to_str().ok())
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
        .unwrap_or("unknown")
        .to_string()
}

async fn enforce_rate_limit(
    state: &AppState,
    client_ip: &str,
    identity: &str,
) -> Result<(), AppError> {
    let mut limiter = state.auth_limiter.lock().await;
    if let Some(retry_after) = limiter.check(client_ip, identity) {
        return Err(AppError::TooManyRequests(format!(
            "Too many authentication attempts. Retry in {retry_after} seconds."
        )));
    }
    Ok(())
}

async fn record_failure(state: &AppState, client_ip: &str, identity: &str) {
    let mut limiter = state.auth_limiter.lock().await;
    limiter.register_failure(client_ip, identity);
}

async fn clear_identity_failures(state: &AppState, identity: &str) {
    let mut limiter = state.auth_limiter.lock().await;
    limiter.reset_identity(identity);
}
