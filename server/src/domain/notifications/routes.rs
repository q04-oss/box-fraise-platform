#![deny(clippy::disallowed_methods)] // Grade A item 5 — raw SQL belongs in repository.rs (clippy.toml)
//! `GET /api/notifications/stream` — authenticated SSE feed.

use std::convert::Infallible;

use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    routing::get,
    Router,
};
use futures::Stream;
use tokio_stream::{wrappers::BroadcastStream, StreamExt};

use crate::{app::AppState, http::extractors::auth::RequireUser};

pub fn router() -> Router<AppState> {
    Router::new().route("/api/notifications/stream", get(stream))
}

/// `GET /api/notifications/stream`
///
/// Subscribes the authenticated user to the broadcast channel on
/// `AppState`. Each delivered event is filtered against the user's id —
/// the JSON body is sent only when `target_user_id` matches the caller or
/// when the event is a broadcast (target_user_id == 0). Lagged receivers
/// (slow clients) silently skip the dropped events; the stream itself
/// stays alive because we map `Lagged` errors to `None` in `filter_map`.
#[utoipa::path(
    get, path = "/api/notifications/stream", tag = "notifications",
    responses(
        (status = 200, description = "Server-Sent Events stream", content_type = "text/event-stream"),
    ),
    security(("bearer_auth" = [])),
)]
pub async fn stream(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let me = i32::from(user_id);
    let rx = state.event_tx.subscribe();
    let s  = BroadcastStream::new(rx).filter_map(move |result| match result {
        Ok(event) => {
            let target = event.target_user_id();
            if target == me || target == 0 {
                let data = serde_json::to_string(&event).unwrap_or_default();
                Some(Ok(Event::default().data(data)))
            } else {
                None
            }
        }
        // Lagged: client too slow — drop the dropped events, keep the stream alive.
        Err(_) => None,
    });

    Sse::new(s).keep_alive(KeepAlive::default())
}