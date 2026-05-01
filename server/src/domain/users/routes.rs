use axum::{
    extract::{Path, Query, State},
    routing::{get, patch, post},
    Json, Router,
};

use crate::{
    app::AppState,
    error::{AppError, AppResult},
    http::extractors::auth::RequireUser,
    types::UserId,
};
use super::{service, types::*};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/users/search",                  get(search))
        .route("/api/users/{id}/public-profile",      get(public_profile))
        .route("/api/users/me/social-access",        get(social_access))
        .route("/api/notifications",                 get(list_notifications))
        .route("/api/notifications/read-all",        post(read_all))
        .route("/api/notifications/{id}/read",        patch(mark_read))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn search(
    State(state): State<AppState>,
    RequireUser(_): RequireUser,
    Query(q): Query<SearchQuery>,
) -> AppResult<Json<Vec<UserSearchResult>>> {
    let trimmed = q.q.trim();
    if trimmed.is_empty() || trimmed.len() > 50 {
        return Err(AppError::bad_request("q must be 1-50 characters"));
    }
    Ok(Json(service::search_users(&state.db, trimmed).await?))
}

async fn public_profile(
    State(state): State<AppState>,
    Path(user_id): Path<UserId>,
) -> AppResult<Json<PublicProfile>> {
    Ok(Json(service::get_public_profile(&state.db, user_id).await?))
}

async fn social_access(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<SocialAccess>> {
    Ok(Json(service::get_social_access(&state.db, user_id).await?))
}

async fn list_notifications(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<Vec<NotificationRow>>> {
    Ok(Json(service::list_notifications(&state.db, user_id).await?))
}

async fn mark_read(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(notif_id): Path<i32>,
) -> AppResult<Json<serde_json::Value>> {
    service::mark_notification_read(&state.db, user_id, notif_id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn read_all(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<serde_json::Value>> {
    service::mark_all_notifications_read(&state.db, user_id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
