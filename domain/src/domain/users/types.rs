use serde::{Deserialize, Serialize};

use crate::types::UserId;

// ── Public-safe user projection ───────────────────────────────────────────────

/// Public-facing user profile — contains no private or sensitive fields.
#[derive(Debug, Serialize)]
pub struct PublicProfile {
    /// User identifier.
    pub id:                  UserId,
    /// Display name chosen by the user (may be absent).
    pub display_name:        Option<String>,
    /// Whether the user's email has been verified.
    pub email_verified:      bool,
    /// BFIP verification status (registered / identity_confirmed / presence_confirmed / attested).
    pub verification_status: String,
    /// Whether the user holds a soultoken.
    pub soultoken_id:        Option<i32>,
}

/// Compact user result returned by the search endpoint.
#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct UserSearchResult {
    /// User identifier.
    pub id:                  UserId,
    /// Display name chosen by the user.
    pub display_name:        Option<String>,
    /// Whether the user's email has been verified.
    pub email_verified:      bool,
    /// BFIP verification status.
    pub verification_status: String,
}

// ── Request bodies ────────────────────────────────────────────────────────────

/// Query parameters for `GET /api/users/search`.
#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    /// Free-text search term matched against display name and email.
    pub q: String,
}
