use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

// ── Stored row ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct MessageRow {
    pub id:                    i32,
    pub sender_id:             i32,
    pub recipient_id:          i32,
    pub body:                  String,
    pub read:                  bool,
    pub order_id:              Option<i32>,
    #[sqlx(rename = "type")]
    pub message_type:          String,
    pub metadata:              Option<serde_json::Value>,
    pub encrypted:             bool,
    pub ephemeral_key:         Option<String>,
    pub sender_identity_key:   Option<String>,
    pub one_time_pre_key_id:   Option<i32>,
    pub created_at:            NaiveDateTime,
}

// ── Request bodies ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SendMessageBody {
    pub recipient_id:        i32,
    pub body:                String,
    /// Set when the body is E2E ciphertext.
    pub encrypted:           Option<bool>,
    pub ephemeral_key:       Option<String>,
    pub sender_identity_key: Option<String>,
    pub one_time_pre_key_id: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct ThreadQuery {
    pub before: Option<i32>,
    pub limit:  Option<i64>,
}

// ── Response bodies ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ConversationSummary {
    pub peer_id:      i32,
    pub peer_name:    Option<String>,
    pub last_message: Option<MessageRow>,
    pub unread_count: i64,
}
