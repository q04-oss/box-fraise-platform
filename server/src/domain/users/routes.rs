
use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;

use crate::{
    app::AppState,
    error::AppResult,
    http::extractors::{auth::RequireUser, json::AppJson},
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
        // Hardening §10 — admin dispute tooling.
        .route("/api/admin/users/{id}/ban",      post(ban_user))
        .route("/api/admin/users/{id}/unban",    post(unban_user))
}

#[derive(Deserialize)]
struct BanRequest {
    reason: String,
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

/// `POST /api/admin/users/:id/ban` — Hardening §10. platform_admin only.
pub async fn ban_user(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(target):         Path<i32>,
    AppJson(body):        AppJson<BanRequest>,
) -> AppResult<Json<serde_json::Value>> {
    service::admin_ban_user(&state.db, i32::from(user_id), target, body.reason).await?;
    Ok(Json(json!({ "user_id": target, "is_banned": true })))
}

/// `POST /api/admin/users/:id/unban`.
pub async fn unban_user(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(target):         Path<i32>,
) -> AppResult<Json<serde_json::Value>> {
    service::admin_unban_user(&state.db, i32::from(user_id), target).await?;
    Ok(Json(json!({ "user_id": target, "is_banned": false })))
}
