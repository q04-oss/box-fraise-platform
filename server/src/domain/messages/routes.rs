use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};

use crate::{
    app::AppState,
    error::AppResult,
    http::extractors::{auth::RequireUser, json::AppJson},
    types::UserId,
};
use super::{service, types::*};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/messages/conversations",                  get(conversations))
        .route("/api/messages/conversations/{userId}/archive", post(archive))
        .route("/api/messages/{userId}",                       get(thread))
        .route("/api/messages",                                post(send))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn conversations(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<Vec<ConversationSummary>>> {
    Ok(Json(service::list_conversations(&state.db, user_id).await?))
}

async fn archive(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(peer_id): Path<UserId>,
) -> AppResult<Json<serde_json::Value>> {
    service::archive_conversation(&state.db, user_id, peer_id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn thread(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(peer_id): Path<UserId>,
    Query(q): Query<ThreadQuery>,
) -> AppResult<Json<Vec<MessageRow>>> {
    let limit = q.limit.unwrap_or(50).min(100);
    Ok(Json(service::get_thread(&state.db, user_id, peer_id, q.before, limit).await?))
}

async fn send(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body): AppJson<SendMessageBody>,
) -> AppResult<Json<MessageRow>> {
    Ok(Json(service::send_message(&state.db, &state.http, user_id, body).await?))
}
