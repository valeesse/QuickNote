use super::*;

pub async fn events_sse(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.event_tx.subscribe();
    let stream =
        tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(move |result| match result {
            Ok(event) if event.user_id == user_id => {
                Some(Ok(Event::default().json_data(event).unwrap_or_default()))
            }
            _ => None,
        });
    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}
