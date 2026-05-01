use sqlx::PgPool;

use crate::{error::{DomainError, AppResult}, types::{KeyId, MessageId, UserId}};
use super::types::{ConversationSummary, MessageRow};

const MSG_COLS: &str =
    "id, sender_id, recipient_id, body, read, order_id, type, metadata,
     encrypted, ephemeral_key, sender_identity_key, one_time_pre_key_id, created_at";

// ── Permission check ──────────────────────────────────────────────────────────

/// Returns true if `from` is permitted to send a message to `to`.
///
/// Rules:
///   1. Shop accounts can always receive messages.
///   2. Both users must be verified.
pub async fn can_message(pool: &PgPool, from: UserId, to: UserId) -> AppResult<bool> {
    // Rule 1: shop account.
    let is_shop: bool = sqlx::query_scalar(
        "SELECT COALESCE((SELECT is_shop FROM users WHERE id = $1), false)",
    )
    .bind(to)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)?;
    if is_shop { return Ok(true); }

    // Rule 2: both verified.
    sqlx::query_scalar(
        "SELECT (
             (SELECT verified FROM users WHERE id = $1) = true AND
             (SELECT verified FROM users WHERE id = $2) = true
         )",
    )
    .bind(from)
    .bind(to)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

// ── Conversations ─────────────────────────────────────────────────────────────

pub async fn list_conversations(
    pool:    &PgPool,
    user_id: UserId,
) -> AppResult<Vec<ConversationSummary>> {
    // Find all peers the user has exchanged messages with (excluding archived).
    let peers: Vec<(UserId, Option<String>)> = sqlx::query_as(
        "SELECT DISTINCT
             CASE WHEN sender_id = $1 THEN recipient_id ELSE sender_id END AS peer_id,
             (SELECT display_name FROM users u WHERE u.id =
              CASE WHEN m.sender_id = $1 THEN m.recipient_id ELSE m.sender_id END)
         FROM messages m
         WHERE (sender_id = $1 OR recipient_id = $1)
           AND NOT (
               archived_by IS NOT NULL AND archived_by = $1
           )
         ORDER BY peer_id",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(DomainError::Db)?;

    let mut summaries = Vec::with_capacity(peers.len());
    for (peer_id, peer_name) in peers {
        let last_message: Option<MessageRow> = sqlx::query_as(&format!(
            "SELECT {MSG_COLS} FROM messages
             WHERE (sender_id = $1 AND recipient_id = $2)
                OR (sender_id = $2 AND recipient_id = $1)
             ORDER BY created_at DESC LIMIT 1"
        ))
        .bind(user_id)
        .bind(peer_id)
        .fetch_optional(pool)
        .await
        .map_err(DomainError::Db)?;

        let unread_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM messages
             WHERE sender_id = $1 AND recipient_id = $2 AND read = false",
        )
        .bind(peer_id)
        .bind(user_id)
        .fetch_one(pool)
        .await
        .map_err(DomainError::Db)?;

        summaries.push(ConversationSummary {
            peer_id,
            peer_name,
            last_message,
            unread_count,
        });
    }

    Ok(summaries)
}

// ── Thread ────────────────────────────────────────────────────────────────────

pub async fn thread(
    pool:      &PgPool,
    user_id:   UserId,
    peer_id:   UserId,
    before_id: Option<MessageId>,
    limit:     i64,
) -> AppResult<Vec<MessageRow>> {
    // Mark messages from the peer as read.
    let _ = sqlx::query(
        "UPDATE messages SET read = true
         WHERE sender_id = $1 AND recipient_id = $2 AND read = false",
    )
    .bind(peer_id)
    .bind(user_id)
    .execute(pool)
    .await;

    if let Some(before) = before_id {
        sqlx::query_as(&format!(
            "SELECT {MSG_COLS} FROM messages
             WHERE ((sender_id = $1 AND recipient_id = $2)
                 OR (sender_id = $2 AND recipient_id = $1))
               AND id < $3
             ORDER BY created_at DESC
             LIMIT $4"
        ))
        .bind(user_id)
        .bind(peer_id)
        .bind(before)
        .bind(limit)
        .fetch_all(pool)
        .await
        .map_err(DomainError::Db)
    } else {
        sqlx::query_as(&format!(
            "SELECT {MSG_COLS} FROM messages
             WHERE (sender_id = $1 AND recipient_id = $2)
                OR (sender_id = $2 AND recipient_id = $1)
             ORDER BY created_at DESC
             LIMIT $3"
        ))
        .bind(user_id)
        .bind(peer_id)
        .bind(limit)
        .fetch_all(pool)
        .await
        .map_err(DomainError::Db)
    }
}

// ── Send ──────────────────────────────────────────────────────────────────────

pub async fn insert(
    pool:                &PgPool,
    sender_id:           UserId,
    recipient_id:        UserId,
    body:                &str,
    encrypted:           bool,
    ephemeral_key:       Option<&str>,
    sender_identity_key: Option<&str>,
    one_time_pre_key_id: Option<KeyId>,
) -> AppResult<MessageRow> {
    sqlx::query_as(&format!(
        "INSERT INTO messages
             (sender_id, recipient_id, body, type, encrypted,
              ephemeral_key, sender_identity_key, one_time_pre_key_id)
         VALUES ($1, $2, $3, 'text', $4, $5, $6, $7)
         RETURNING {MSG_COLS}"
    ))
    .bind(sender_id)
    .bind(recipient_id)
    .bind(body)
    .bind(encrypted)
    .bind(ephemeral_key)
    .bind(sender_identity_key)
    .bind(one_time_pre_key_id)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

// ── Archive ───────────────────────────────────────────────────────────────────

pub async fn archive(pool: &PgPool, user_id: UserId, peer_id: UserId) -> AppResult<()> {
    sqlx::query(
        "UPDATE messages
         SET archived_by = $1
         WHERE (sender_id = $1 AND recipient_id = $2)
            OR (sender_id = $2 AND recipient_id = $1)",
    )
    .bind(user_id)
    .bind(peer_id)
    .execute(pool)
    .await
    .map_err(DomainError::Db)?;
    Ok(())
}
