mod config;
mod db;
mod error;
mod middleware;
mod models;
mod routes;

use axum::{
    routing::{delete, get, patch, post},
    Router,
};
use config::Config;
use db::DbPool;
use std::sync::Arc;
use tokio::sync::broadcast;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

pub struct AppState {
    pub db: DbPool,
    pub config: Config,
    pub event_tx: broadcast::Sender<models::SyncEvent>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
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
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/api/health", get(|| async { "ok" }))
        // Auth
        .route("/api/auth/register", post(routes::auth::register))
        .route("/api/auth/login", post(routes::auth::login))
        .route("/api/auth/refresh", post(routes::auth::refresh))
        // Notes (protected)
        .route("/api/notes", get(routes::notes::list_notes).post(routes::notes::create_note))
        .route(
            "/api/notes/{id}",
            get(routes::notes::get_note)
                .put(routes::notes::update_note)
                .delete(routes::notes::delete_note),
        )
        .route("/api/notes/{id}/restore", post(routes::notes::restore_note))
        .route("/api/notes/{id}/pin", patch(routes::notes::toggle_pin))
        .route("/api/notes/search", get(routes::notes::search_notes))
        // Clipboard (protected)
        .route(
            "/api/clipboard",
            get(routes::clipboard::list_items).post(routes::clipboard::capture),
        )
        .route(
            "/api/clipboard/{id}",
            delete(routes::clipboard::delete_item),
        )
        // Sync (protected)
        .route("/api/sync/pull", post(routes::sync::pull))
        .route("/api/sync/push", post(routes::sync::push))
        .route("/api/events", get(routes::sync::events_sse))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = "0.0.0.0:3000";
    tracing::info!("Server listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
