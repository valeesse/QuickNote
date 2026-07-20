mod app;
mod auth_limits;
mod collab;
mod config;
mod db;
mod error;
mod middleware;
mod migrations;
mod models;
mod routes;

use config::Config;
use db::DbPool;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::sync::Mutex;

pub struct AppState {
    pub db: DbPool,
    pub config: Config,
    pub event_tx: broadcast::Sender<models::SyncEvent>,
    pub http: reqwest::Client,
    pub auth_limiter: Mutex<auth_limits::AuthRateLimiter>,
    pub collab_hub: collab::CollabHub,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let config = Config::from_env().expect("Failed to load config");
    let db = DbPool::connect(&config.database_url)
        .await
        .expect("Failed to connect to database");

    db.run_migrations().await.expect("Failed to run migrations");

    let (event_tx, _) = broadcast::channel(256);

    let state = Arc::new(AppState {
        db,
        config,
        event_tx,
        http: reqwest::Client::new(),
        auth_limiter: Mutex::new(auth_limits::AuthRateLimiter::default()),
        collab_hub: collab::CollabHub::default(),
    });

    let app = app::build_router(state);

    let addr = "0.0.0.0:3000";
    tracing::info!("Server listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
