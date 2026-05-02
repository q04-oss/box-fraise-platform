use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::types::UserId;

// ── Database row ──────────────────────────────────────────────────────────────

/// Subset of `users` columns used across the auth domain.
/// Extend with additional columns as other domains require them.
#[allow(missing_docs)] // Database row — field names are identical to column names.
#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct UserRow {
    pub id:                           UserId,
    pub email:                        String,
    pub apple_id:                     Option<String>,
    pub display_name:                 Option<String>,
    pub push_token:                   Option<String>,
    pub email_verified:               bool,
    pub is_platform_admin:            bool,
    pub is_banned:                    bool,
    pub verification_status:          String,
    pub attested_at:                  Option<DateTime<Utc>>,
    pub soultoken_id:                 Option<i32>,
    pub cleared_at:                   Option<DateTime<Utc>>,
    pub cleared_soultoken_id:         Option<i32>,
    pub platform_gift_eligible_after: Option<DateTime<Utc>>,
    pub deleted_at:                   Option<DateTime<Utc>>,
    pub last_active_at:               Option<DateTime<Utc>>,
    pub updated_at:                   DateTime<Utc>,
    pub created_at:                   DateTime<Utc>,
}

/// Columns to SELECT when fetching a full UserRow.
pub const USER_COLS: &str =
    "id, email, apple_id, display_name, push_token, \
     email_verified, is_platform_admin, is_banned, verification_status, \
     attested_at, soultoken_id, cleared_at, cleared_soultoken_id, \
     platform_gift_eligible_after, deleted_at, last_active_at, \
     updated_at, created_at";

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
