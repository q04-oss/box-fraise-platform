use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use crate::types::UserId;

// ── Public-safe user projection ───────────────────────────────────────────────
// Never include: password_hash, push_token, stripe_*, ban_reason, or any
// internal flag not meant for external consumption.

#[derive(Debug, Serialize)]
pub struct PublicProfile {
    pub id:           UserId,
    pub display_name: Option<String>,
    pub portrait_url: Option<String>,
    pub is_dj:        bool,
    pub verified:     bool,
    pub user_code:    Option<String>,
}

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct UserSearchResult {
    pub id:           UserId,
    pub display_name: Option<String>,
    pub portrait_url: Option<String>,
    pub verified:     bool,
    pub user_code:    Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SocialAccess {
    pub social_tier:           Option<String>,
    pub social_time_bank_seconds: i32,
}

#[derive(Debug, Serialize)]
pub struct UserStats {
    pub nfc_connection_count: i64,
    pub membership_tier:      Option<String>,
}

// ── Notifications ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct NotificationRow {
    pub id:         i32,
    pub user_id:    UserId,
    #[sqlx(rename = "type")]
    pub notif_type: String,
    pub title:      Option<String>,
    pub body:       String,
    pub read:       bool,
    pub data:       Option<serde_json::Value>,
    pub created_at: NaiveDateTime,
}

// ── Request bodies ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct WalletBody {
    pub eth_address: String,
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
}

#[derive(Debug, Deserialize)]
pub struct NotificationPrefsBody {
    pub order_updates: Option<bool>,
    pub social:        Option<bool>,
    pub popup_updates: Option<bool>,
    pub marketing:     Option<bool>,
}
