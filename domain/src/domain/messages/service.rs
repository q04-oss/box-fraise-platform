use sqlx::PgPool;

use crate::{
    error::{AppError, AppResult},
    types::{MessageId, UserId},
};
use super::{
    repository,
    types::{ConversationSummary, MessageRow, SendMessageBody},
};

pub async fn list_conversations(
    pool:    &PgPool,
    user_id: UserId,
) -> AppResult<Vec<ConversationSummary>> {
    repository::list_conversations(pool, user_id).await
}

pub async fn archive_conversation(
    pool:    &PgPool,
    user_id: UserId,
    peer_id: UserId,
) -> AppResult<()> {
    repository::archive(pool, user_id, peer_id).await
}

pub async fn get_thread(
    pool:      &PgPool,
    user_id:   UserId,
    peer_id:   UserId,
    before_id: Option<MessageId>,
    limit:     i64,
) -> AppResult<Vec<MessageRow>> {
    repository::thread(pool, user_id, peer_id, before_id, limit).await
}

/// Validates send permission, persists the message, and fires a push
/// notification to the recipient. The push is best-effort — failure is
/// logged but never blocks the response.
pub async fn send_message(
    pool:    &PgPool,
    http:    &reqwest::Client,
    user_id: UserId,
    body:    SendMessageBody,
) -> AppResult<MessageRow> {
    if !repository::can_message(pool, user_id, body.recipient_id).await? {
        return Err(AppError::Forbidden);
    }

    let message = repository::insert(
        pool,
        user_id,
        body.recipient_id,
        &body.body,
        body.encrypted.unwrap_or(false),
        body.ephemeral_key.as_deref(),
        body.sender_identity_key.as_deref(),
        body.one_time_pre_key_id,
    )
    .await?;

    // Fire-and-forget — push failure must never fail the message send.
    let db   = pool.clone();
    let http = http.clone();
    let rid  = body.recipient_id;
    tokio::spawn(async move {
        match sqlx::query_as::<_, (Option<String>,)>(
            "SELECT push_token FROM users WHERE id = $1",
        )
        .bind(rid)
        .fetch_optional(&db)
        .await
        {
            Ok(Some((Some(token),))) => {
                use box_fraise_integrations::expo_push::{send, PushMessage};
                if let Err(e) = send(&http, PushMessage { to: &token, body: "New message", ..Default::default() }).await {
                    tracing::error!(recipient_id = i32::from(rid), error = %e, "push notification failed");
                }
            }
            Ok(_)  => {}
            Err(e) => tracing::error!(recipient_id = i32::from(rid), error = %e, "push token lookup failed"),
        }
    });

    Ok(message)
}
