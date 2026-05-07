
use axum::{
    extract::{Path, Query, State},
    routing::{delete, get},
    Json, Router,
};

use crate::{
    app::AppState,
    error::AppResult,
    http::extractors::auth::RequireUser,
    types::UserId,
};
use super::{service, types::*};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/users/search",              get(search))
        .route("/api/users/{id}/public-profile", get(public_profile))
        // Hardening §9 — GDPR right to erasure + right to portability.
        .route("/api/users/me",                  delete(erase_me))
        .route("/api/users/me/export",           get(export_me))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

pub async fn search(
    State(state): State<AppState>,
    RequireUser(_): RequireUser,
    Query(q): Query<SearchQuery>,
) -> AppResult<Json<Vec<UserSearchResult>>> {
    let trimmed = q.q.trim();
    if trimmed.is_empty() || trimmed.len() > 50 {
        return Err(crate::error::AppError::bad_request("q must be 1-50 characters"));
    }
    Ok(Json(service::search_users(&state.db, trimmed).await?))
}

pub async fn public_profile(
    State(state): State<AppState>,
    Path(user_id): Path<UserId>,
) -> AppResult<Json<PublicProfile>> {
    Ok(Json(service::get_public_profile(&state.db, user_id).await?))
}

/// `DELETE /api/users/me` — request erasure. Returns the per-row anonymisation
/// summary plus the retention rules that left a trace behind.
pub async fn erase_me(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<ErasureResponse>> {
    Ok(Json(service::request_erasure(&state.db, i32::from(user_id)).await?))
}

/// `GET /api/users/me/export` — full export of the caller's data
/// (GDPR Article 20).
pub async fn export_me(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<UserDataExport>> {
    Ok(Json(service::export_my_data(&state.db, i32::from(user_id)).await?))
}
