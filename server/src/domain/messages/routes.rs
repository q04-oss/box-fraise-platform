use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};

use crate::{
    app::AppState,
    error::{AppError, AppResult},
    http::extractors::{auth::RequireUser, json::AppJson},
};
use super::{repository, types::*};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/messages/conversations",               get(conversations))
        .route("/api/messages/conversations/:userId/archive", post(archive))
        .route("/api/messages/:userId",                     get(thread))
        .route("/api/messages",                             post(send))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn conversations(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<Vec<ConversationSummary>>> {
    Ok(Json(repository::list_conversations(&state.db, user_id).await?))
}

async fn archive(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(peer_id): Path<i32>,
) -> AppResult<Json<serde_json::Value>> {
    repository::archive(&state.db, user_id, peer_id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn thread(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(peer_id): Path<i32>,
    Query(q): Query<ThreadQuery>,
) -> AppResult<Json<Vec<MessageRow>>> {
    let limit = q.limit.unwrap_or(50).min(100);
    Ok(Json(
        repository::thread(&state.db, user_id, peer_id, q.before, limit).await?,
    ))
}

async fn send(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body): AppJson<SendMessageBody>,
) -> AppResult<Json<MessageRow>> {
    if !repository::can_message(&state.db, user_id, body.recipient_id).await? {
        return Err(AppError::Forbidden);
    }

    let message = repository::insert(
        &state.db,
        user_id,
        body.recipient_id,
        &body.body,
        body.encrypted.unwrap_or(false),
        body.ephemeral_key.as_deref(),
        body.sender_identity_key.as_deref(),
        body.one_time_pre_key_id,
    )
    .await?;

    // Fire-and-forget push to recipient.
    {
        let pool = state.db.clone();
        let http = state.http.clone();
        let rid  = body.recipient_id;
        tokio::spawn(async move {
            if let Ok(Some((token,))) = sqlx::query_as::<_, (Option<String>,)>(
                "SELECT push_token FROM users WHERE id = $1"
            )
            .bind(rid)
            .fetch_optional(&pool)
            .await {
                if let Some(t) = token {
                    let _ = crate::integrations::expo_push::send(
                        &http,
                        crate::integrations::expo_push::PushMessage {
                            to:   &t,
                            body: "New message",
                            ..Default::default()
                        },
                    ).await;
                }
            }
        });
    }

    Ok(Json(message))
}
