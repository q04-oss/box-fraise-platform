use serde::{Deserialize, Serialize};

use crate::types::UserId;

// ── Public-safe user projection ───────────────────────────────────────────────

/// Public-facing user profile — contains no private or sensitive fields.
#[derive(Debug, Serialize)]
pub struct PublicProfile {
    /// User identifier.
    pub id:           UserId,
    /// Display name chosen by the user (may be absent).
    pub display_name: Option<String>,
    /// URL of the user's portrait image (may be absent).
    pub portrait_url: Option<String>,
    /// Whether this user has DJ privileges.
    pub is_dj:        bool,
    /// Whether the user's email has been verified.
    pub verified:     bool,
    /// Short user code for QR-based key bundle lookup.
    pub user_code:    Option<String>,
}

/// Compact user result returned by the search endpoint.
#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct UserSearchResult {
    /// User identifier.
    pub id:           UserId,
    /// Display name chosen by the user.
    pub display_name: Option<String>,
    /// URL of the user's portrait image.
    pub portrait_url: Option<String>,
    /// Whether the user's email has been verified.
    pub verified:     bool,
    /// Short user code used for QR-based key bundle lookup.
    pub user_code:    Option<String>,
}

// ── Request bodies ────────────────────────────────────────────────────────────

/// Query parameters for `GET /api/users/search`.
#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    /// Free-text search term matched against display name and email.
    pub q: String,
}
