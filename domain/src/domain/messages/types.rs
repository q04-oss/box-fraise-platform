use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use crate::types::{KeyId, MessageId, UserId};

// ── Stored row ────────────────────────────────────────────────────────────────

/// A single message row as stored in the `messages` table.
#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct MessageRow {
    /// Message identifier.
    pub id:                    MessageId,
    /// User who sent the message.
    pub sender_id:             UserId,
    /// User who received the message.
    pub recipient_id:          UserId,
    /// Message body (plaintext or base64 ciphertext when `encrypted` is true).
    pub body:                  String,
    /// Whether the recipient has read the message.
    pub read:                  bool,
    /// Associated order ID, if any.
    pub order_id:              Option<i32>,
    /// Message type tag (e.g. `"text"`, `"order"`).
    #[sqlx(rename = "type")]
    pub message_type:          String,
    /// Additional structured metadata for non-text messages.
    pub metadata:              Option<serde_json::Value>,
    /// Whether the body is E2E ciphertext.
    pub encrypted:             bool,
    /// X25519 ephemeral public key (base64) for E2E sessions.
    pub ephemeral_key:         Option<String>,
    /// Sender's X25519 identity public key (base64) for X3DH.
    pub sender_identity_key:   Option<String>,
    /// ID of the one-time pre-key consumed for this session.
    pub one_time_pre_key_id:   Option<KeyId>,
    /// When the message was created.
    pub created_at:            NaiveDateTime,
}

// ── Request bodies ────────────────────────────────────────────────────────────

/// Request body for `POST /api/messages`.
#[derive(Debug, Deserialize)]
pub struct SendMessageBody {
    /// User to send the message to.
    pub recipient_id:        UserId,
    /// Message body (plaintext or base64 ciphertext).
    pub body:                String,
    /// Set to `true` when the body is E2E ciphertext.
    pub encrypted:           Option<bool>,
    /// X25519 ephemeral public key (base64), required for E2E sessions.
    pub ephemeral_key:       Option<String>,
    /// Sender's X25519 identity public key (base64), required for X3DH.
    pub sender_identity_key: Option<String>,
    /// ID of the one-time pre-key consumed to establish this session.
    pub one_time_pre_key_id: Option<KeyId>,
}

/// Query parameters for `GET /api/messages/{userId}` thread pagination.
#[derive(Debug, Deserialize)]
pub struct ThreadQuery {
    /// Return only messages older than this message ID (cursor pagination).
    pub before: Option<MessageId>,
    /// Maximum number of messages to return (defaults to 50, capped at 100).
    pub limit:  Option<i64>,
}

// ── Response bodies ───────────────────────────────────────────────────────────

/// Summary of a single conversation, returned by the conversations list endpoint.
#[derive(Debug, Serialize)]
pub struct ConversationSummary {
    /// The other participant in the conversation.
    pub peer_id:      UserId,
    /// Display name of the other participant.
    pub peer_name:    Option<String>,
    /// The most recent message in the conversation.
    pub last_message: Option<MessageRow>,
    /// Number of unread messages from the peer.
    pub unread_count: i64,
}
