use crate::{collab, routes, AppState};
use axum::extract::DefaultBodyLimit;
use axum::http::{HeaderName, HeaderValue, Method};
use axum::routing::{delete, get, patch, post, put};
use axum::Router;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::TraceLayer;

pub fn build_router(state: Arc<AppState>) -> Router {
    let request_id = HeaderName::from_static("x-request-id");
    let router = Router::new()
        .route("/api/health", get(|| async { "ok" }))
        .route("/api/auth/register", post(routes::auth::register))
        .route("/api/auth/login", post(routes::auth::login))
        .route("/api/auth/refresh", post(routes::auth::refresh))
        .route("/api/auth/me", get(routes::auth::me))
        .route("/api/auth/logout", post(routes::auth::logout))
        .route(
            "/api/notes",
            get(routes::notes::list_notes).post(routes::notes::create_note),
        )
        .route("/api/notes/reorder", post(routes::notes::reorder_notes))
        .route("/api/notes/trash", get(routes::notes::list_deleted_notes))
        .route("/api/notes/search", get(routes::notes::search_notes))
        .route("/api/notes/page", get(routes::notes_page::list_page))
        .route("/api/tags", get(routes::notes::list_tags))
        .route(
            "/api/notes/{id}",
            get(routes::notes::get_note)
                .put(routes::notes::update_note)
                .delete(routes::notes::delete_note),
        )
        .route("/api/notes/{id}/restore", post(routes::notes::restore_note))
        .route("/api/notes/{id}/tags", put(routes::notes::update_note_tags))
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
        );
    router
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::middleware::validate_origin,
        ))
        .layer(cors(&state))
        .layer(DefaultBodyLimit::max(21 * 1024 * 1024))
        .layer(PropagateRequestIdLayer::new(request_id.clone()))
        .layer(TraceLayer::new_for_http())
        .layer(SetRequestIdLayer::new(request_id, MakeRequestUuid))
        .with_state(state)
}

fn cors(state: &AppState) -> CorsLayer {
    CorsLayer::new()
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
        .allow_credentials(true)
}
