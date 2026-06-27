mod auth_limits;
mod collab;
mod config;
mod db;
mod error;
mod middleware;
mod models;
mod routes;

use axum::{
    extract::DefaultBodyLimit,
    http::{HeaderValue, Method},
    routing::{delete, get, patch, post},
    Router,
};
use config::Config;
use db::DbPool;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::sync::Mutex;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

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

    let cors = CorsLayer::new()
        .allow_origin(
            state
                .config
                .allowed_origin
                .parse::<HeaderValue>()
                .expect("Invalid ALLOWED_ORIGIN"),
        )
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
        ])
        .allow_headers([
            axum::http::header::AUTHORIZATION,
            axum::http::header::CONTENT_TYPE,
        ])
        .allow_credentials(true);

    let app = Router::new()
        .route("/api/health", get(|| async { "ok" }))
        // Auth
        .route("/api/auth/register", post(routes::auth::register))
        .route("/api/auth/login", post(routes::auth::login))
        .route("/api/auth/refresh", post(routes::auth::refresh))
        .route("/api/auth/me", get(routes::auth::me))
        .route("/api/auth/logout", post(routes::auth::logout))
        .route(
            "/api/account/summary",
            get(routes::billing::account_summary),
        )
        .route(
            "/api/billing/checkout",
            post(routes::billing::create_checkout),
        )
        .route("/api/billing/portal", post(routes::billing::billing_portal))
        .route(
            "/api/billing/webhooks/lemonsqueezy",
            post(routes::billing::lemonsqueezy_webhook),
        )
        // Notes (protected)
        .route(
            "/api/notes",
            get(routes::notes::list_notes).post(routes::notes::create_note),
        )
        .route("/api/notes/reorder", post(routes::notes::reorder_notes))
        .route("/api/notes/trash", get(routes::notes::list_deleted_notes))
        .route("/api/notes/search", get(routes::notes::search_notes))
        .route("/api/tags", get(routes::notes::list_tags))
        .route(
            "/api/notes/{id}",
            get(routes::notes::get_note)
                .put(routes::notes::update_note)
                .delete(routes::notes::delete_note),
        )
        .route("/api/notes/{id}/restore", post(routes::notes::restore_note))
        .route("/api/notes/{id}/tags", axum::routing::put(routes::notes::update_note_tags))
        .route("/api/notes/{id}/pin", patch(routes::notes::toggle_pin))
        .route("/api/notes/{id}/purge", delete(routes::notes::purge_note))
        .route(
            "/api/notes/{id}/versions",
            get(routes::notes::list_versions).delete(routes::notes::clear_versions),
        )
        .route(
            "/api/notes/{id}/versions/{vid}/restore",
            post(routes::notes::restore_version),
        )
        .route(
            "/api/notes/versions/{vid}",
            delete(routes::notes::delete_version),
        )
        .route(
            "/api/notes/versions/{vid}/pin",
            patch(routes::notes::toggle_version_pin),
        )
        // Clipboard (protected)
        .route(
            "/api/clipboard",
            get(routes::clipboard::list_items).post(routes::clipboard::capture),
        )
        .route(
            "/api/clipboard/{id}",
            delete(routes::clipboard::delete_item),
        )
        .route(
            "/api/clipboard/{id}/pin",
            patch(routes::clipboard::toggle_pin),
        )
        // Sync (protected)
        .route("/api/sync/pull", post(routes::sync::pull))
        .route("/api/sync/push", post(routes::sync::push))
        .route("/api/events", get(routes::sync::events_sse))
        .route(
            "/api/collab/notes/{id}/ws",
            get(collab::websocket::note_socket),
        )
        .route(
            "/api/attachments/{id}",
            get(routes::attachments::download).put(routes::attachments::upload),
        )
        .layer(cors)
        .layer(DefaultBodyLimit::max(21 * 1024 * 1024))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = "0.0.0.0:3000";
    tracing::info!("Server listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
