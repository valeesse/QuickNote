use crate::error::AppError;
use crate::middleware::{create_token, AuthUser};
use crate::models::{AuthRequest, AuthResponse, User, UserResponse};
use crate::AppState;
use argon2::{
    password_hash::{rand_core::OsRng, SaltString},
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
};
use axum::{extract::State, Json};
use std::sync::Arc;
use uuid::Uuid;

pub async fn register(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AuthRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    if !is_valid_email(&req.email) || req.password.len() < 10 {
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
    let email = req.email.trim().to_lowercase();

    sqlx::query("INSERT INTO users (id, email, password_hash) VALUES ($1, $2, $3)")
        .bind(user_id)
        .bind(&email)
        .bind(&password_hash)
        .execute(state.db.inner())
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(ref db_err) = e {
                if db_err.constraint() == Some("users_email_key") {
                    return AppError::BadRequest("Email already registered".into());
                }
            }
            AppError::Db(e)
        })?;

    let token = create_token(user_id, &state.config.jwt_secret)?;
    Ok(Json(AuthResponse {
        token,
        user: UserResponse { id: user_id, email },
    }))
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
    Json(req): Json<AuthRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    let email = req.email.trim().to_lowercase();

    let user: User =
        sqlx::query_as("SELECT id, email, password_hash, created_at FROM users WHERE email = $1")
            .bind(&email)
            .fetch_optional(state.db.inner())
            .await?
            .ok_or(AppError::Auth)?;

    let parsed_hash =
        PasswordHash::new(&user.password_hash).map_err(|e| AppError::Internal(e.to_string()))?;
    Argon2::default()
        .verify_password(req.password.as_bytes(), &parsed_hash)
        .map_err(|_| AppError::Auth)?;

    let token = create_token(user.id, &state.config.jwt_secret)?;
    Ok(Json(AuthResponse {
        token,
        user: UserResponse {
            id: user.id,
            email: user.email,
        },
    }))
}

pub async fn refresh(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    let token = create_token(user_id, &state.config.jwt_secret)?;
    Ok(Json(serde_json::json!({ "token": token })))
}
