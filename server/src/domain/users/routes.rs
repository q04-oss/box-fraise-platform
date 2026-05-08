
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

#[derive(Deserialize, utoipa::ToSchema)]
pub struct BanRequest {
    pub reason: String,
}

// ── Handlers ──────────────────────────────────────────────────────────────────

#[utoipa::path(
    get, path = "/api/users/search", tag = "users",
    params(("q" = String, Query, description = "Free-text search term (1–50 chars), matched against display name and email")),
    responses(
        (status = 200, description = "Matching users", body = [UserSearchResult]),
        (status = 400, description = "q must be 1-50 characters"),
        (status = 401, description = "Authentication required"),
    ),
    security(("bearer_auth" = [])),
)]
pub async fn search(
    State(state): State<AppState>,
    RequireUser(_user_id): RequireUser,
    Query(q): Query<SearchQuery>,
) -> AppResult<Json<Vec<UserSearchResult>>> {
    let trimmed = q.q.trim();
    if trimmed.is_empty() || trimmed.len() > 50 {
        return Err(crate::error::AppError::bad_request("q must be 1-50 characters"));
    }
    Ok(Json(service::search_users(&state.db, trimmed).await?))
}

#[utoipa::path(
    get, path = "/api/users/{id}/public-profile", tag = "users",
    params(("id" = i32, Path, description = "Target user identifier")),
    responses(
        (status = 200, description = "Public-safe profile of the requested user", body = PublicProfile),
        (status = 401, description = "Authentication required"),
        (status = 404, description = "User not found"),
    ),
    security(("bearer_auth" = [])),
)]
pub async fn public_profile(
    State(state): State<AppState>,
    Path(user_id): Path<UserId>,
) -> AppResult<Json<PublicProfile>> {
    Ok(Json(service::get_public_profile(&state.db, user_id).await?))
}

/// `DELETE /api/users/me` — request erasure. Returns the per-row anonymisation
/// summary plus the retention rules that left a trace behind.
#[utoipa::path(
    delete, path = "/api/users/me", tag = "users",
    responses(
        (status = 200, description = "Erasure completed; returns retention summary", body = ErasureResponse),
        (status = 401, description = "Authentication required"),
    ),
    security(("bearer_auth" = [])),
)]
pub async fn erase_me(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<ErasureResponse>> {
    let mut tx = box_fraise_domain::transaction::RlsTransaction::begin(
        &state.db, i32::from(user_id),
    ).await?;
    let resp = service::request_erasure(&mut tx, &state.db, i32::from(user_id)).await?;
    tx.commit().await?;
    Ok(Json(resp))
}

/// `GET /api/users/me/export` — full export of the caller's data
/// (GDPR Article 20).
#[utoipa::path(
    get, path = "/api/users/me/export", tag = "users",
    responses(
        (status = 200, description = "Full GDPR Article 20 export of the caller's data", body = UserDataExport),
        (status = 401, description = "Authentication required"),
    ),
    security(("bearer_auth" = [])),
)]
pub async fn export_me(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<UserDataExport>> {
    // Hardening cleanup #3 pilot — RLS-scoped per-request transaction.
    // Begin → call service with `&mut tx` → on Ok, commit; on Err, the
    // `?` operator drops `tx` and Postgres rolls back automatically.
    let mut tx   = box_fraise_domain::transaction::RlsTransaction::begin(
        &state.db, i32::from(user_id),
    ).await?;
    let resp = service::export_my_data(&mut tx, &state.db, i32::from(user_id)).await?;
    tx.commit().await?;
    Ok(Json(resp))
}

/// `POST /api/admin/users/:id/ban` — Hardening §10. platform_admin only.
#[utoipa::path(
    post, path = "/api/admin/users/{id}/ban", tag = "users",
    params(("id" = i32, Path, description = "Target user to ban")),
    request_body = BanRequest,
    responses(
        (status = 200, description = "User banned"),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "platform_admin only"),
        (status = 404, description = "User not found"),
    ),
    security(("bearer_auth" = [])),
)]
pub async fn ban_user(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(target):         Path<i32>,
    AppJson(body):        AppJson<BanRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let mut tx = box_fraise_domain::transaction::AdminRlsTransaction::begin(
        &state.db,
    ).await?;
    service::admin_ban_user(&mut tx, &state.db, i32::from(user_id), target, body.reason).await?;
    tx.commit().await?;
    Ok(Json(json!({ "user_id": target, "is_banned": true })))
}

/// `POST /api/admin/users/:id/unban`.
#[utoipa::path(
    post, path = "/api/admin/users/{id}/unban", tag = "users",
    params(("id" = i32, Path, description = "Target user to unban")),
    responses(
        (status = 200, description = "User unbanned"),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "platform_admin only"),
        (status = 404, description = "User not found"),
    ),
    security(("bearer_auth" = [])),
)]
pub async fn unban_user(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(target):         Path<i32>,
) -> AppResult<Json<serde_json::Value>> {
    let mut tx = box_fraise_domain::transaction::AdminRlsTransaction::begin(
        &state.db,
    ).await?;
    service::admin_unban_user(&mut tx, &state.db, i32::from(user_id), target).await?;
    tx.commit().await?;
    Ok(Json(json!({ "user_id": target, "is_banned": false })))
}
