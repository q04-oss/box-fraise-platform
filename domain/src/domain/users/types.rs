use serde::{Deserialize, Serialize};

use crate::types::UserId;

// ── Public-safe user projection ───────────────────────────────────────────────

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

// ── Request bodies ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
}
