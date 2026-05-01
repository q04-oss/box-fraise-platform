use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use crate::types::{StripeCustomerId, UserId};

// ── Database row ──────────────────────────────────────────────────────────────

/// Subset of `users` columns used across the auth domain.
/// Extend with additional columns as other domains require them.
#[allow(missing_docs)] // Database row — field names are identical to column names.
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

// ── Request bodies ────────────────────────────────────────────────────────────

/// Request body for `POST /api/auth/apple`.
#[derive(Debug, Deserialize)]
pub struct AppleAuthBody {
    /// Apple identity token (JWT) returned by Sign in with Apple.
    pub identity_token: String,
    /// Display name provided by the user on first sign-in (optional).
    pub display_name:   Option<String>,
}

/// Request body for `PATCH /api/auth/push-token`.
#[derive(Debug, Deserialize)]
pub struct PushTokenBody {
    /// Expo push token to register for this device.
    pub push_token: String,
}

/// Request body for `PATCH /api/auth/display-name`.
#[derive(Debug, Deserialize)]
pub struct DisplayNameBody {
    /// New display name (1–50 characters, trimmed).
    pub display_name: String,
}

/// Request body for `POST /api/auth/magic-link`.
#[derive(Debug, Deserialize)]
pub struct MagicLinkBody {
    /// Email address to send the magic link to.
    pub email: String,
}

/// Request body for `POST /api/auth/magic-link/verify`.
#[derive(Debug, Deserialize)]
pub struct MagicLinkVerifyBody {
    /// Single-use token extracted from the magic link URL.
    pub token: String,
}

// ── Response bodies ───────────────────────────────────────────────────────────

/// Response returned on successful authentication (Apple or magic link).
#[derive(Debug, Serialize)]
pub struct AuthResponse {
    /// Authenticated user's identifier.
    pub user_id: UserId,
    /// Signed JWT for subsequent authenticated requests.
    pub token:   String,
    /// `true` if this sign-in created a new account.
    pub is_new:  bool,
    /// `true` if the user's email has been verified.
    pub verified: bool,
}

/// Response returned by `GET /api/auth/me`.
#[derive(Debug, Serialize)]
pub struct MeResponse {
    /// The authenticated user's full profile row.
    #[serde(flatten)]
    pub user: UserRow,
}
