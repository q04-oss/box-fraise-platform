use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use crate::types::{StripeCustomerId, UserId};

// ── Database row ──────────────────────────────────────────────────────────────

/// Subset of `users` columns used across the auth domain.
/// Extend with additional columns as other domains require them.
#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct UserRow {
    pub id:                        UserId,
    pub apple_user_id:             Option<String>,
    pub email:                     String,
    pub display_name:              Option<String>,
    pub push_token:                Option<String>,
    pub user_code:                 Option<String>,
    pub verified:                  bool,
    pub banned:                    bool,
    pub table_verified:            bool,
    pub is_dorotka:                bool,
    pub stripe_customer_id:        Option<StripeCustomerId>,
    pub social_time_bank_seconds:  i32,
    pub identity_verified:         bool,
    pub portrait_url:              Option<String>,
    pub password_hash:             Option<String>,
    pub created_at:                NaiveDateTime,
}

/// Columns to SELECT when fetching a full UserRow.
pub const USER_COLS: &str =
    "id, apple_user_id, email, display_name, push_token, user_code, \
     verified, banned, table_verified, is_dorotka, stripe_customer_id, \
     social_time_bank_seconds, identity_verified, \
     portrait_url, password_hash, created_at";

// ── Device row ────────────────────────────────────────────────────────────────

#[derive(Debug, sqlx::FromRow)]
pub struct DeviceRow {
    pub id:          i32,
    pub role:        String,
    pub user_id:     Option<crate::types::UserId>,
    pub business_id: Option<i32>,
}

// ── Request bodies ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct AppleAuthBody {
    pub identity_token: String,
    pub display_name:   Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DemoAuthBody {
    pub pin: String,
}

#[derive(Debug, Deserialize)]
pub struct RegisterBody {
    pub email:        String,
    pub password:     String,
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LoginBody {
    pub email:    String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct ForgotPasswordBody {
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct ResetPasswordBody {
    pub token:    String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct PushTokenBody {
    pub push_token: String,
}

#[derive(Debug, Deserialize)]
pub struct DisplayNameBody {
    pub display_name: String,
}

#[derive(Debug, Deserialize)]
pub struct MagicLinkBody {
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct MagicLinkVerifyBody {
    pub token: String,
}

// ── Response bodies ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub user_id: UserId,
    pub token:   String,
    pub is_new:  bool,
    pub verified: bool,
}

#[derive(Debug, Serialize)]
pub struct MeResponse {
    #[serde(flatten)]
    pub user: UserRow,
}
