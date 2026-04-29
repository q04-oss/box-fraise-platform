use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, patch, post},
    Json, Router,
};

use crate::{
    app::AppState,
    error::{AppError, AppResult},
    http::extractors::{auth::RequireUser, json::AppJson},
};
use super::{repository, types::*};

pub fn router() -> Router<AppState> {
    Router::new()
        // Search & profiles
        .route("/api/users/search",                  get(search))
        .route("/api/users/{id}/public-profile",      get(public_profile))
        .route("/api/users/{id}/follow-status",       get(follow_status))
        .route("/api/users/{id}/followers",           get(followers))
        .route("/api/users/{id}/followers-list",      get(followers_list))
        .route("/api/users/{id}/following",           get(following))
        // Social actions
        .route("/api/users/{id}/follow",              post(follow).delete(unfollow))
        // My profile mutations
        .route("/api/users/me/wallet",               patch(wallet))
        .route("/api/users/me/social-access",        get(social_access))
        .route("/api/users/me/stats",                get(stats))
        // Notifications
        .route("/api/notifications",                 get(list_notifications))
        .route("/api/notifications/read-all",        post(read_all))
        .route("/api/notifications/{id}/read",        patch(mark_read))
        // Feed
        .route("/api/feed",                          get(feed))
}

// â”€â”€ Search â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

async fn search(
    State(state): State<AppState>,
    RequireUser(_): RequireUser,
    Query(q): Query<SearchQuery>,
) -> AppResult<Json<Vec<UserSearchResult>>> {
    let trimmed = q.q.trim();
    if trimmed.is_empty() || trimmed.len() > 50 {
        return Err(AppError::bad_request("q must be 1â€“50 characters"));
    }
    Ok(Json(repository::search(&state.db, trimmed).await?))
}

// â”€â”€ Public profile â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

async fn public_profile(
    State(state): State<AppState>,
    Path(user_id): Path<i32>,
) -> AppResult<Json<PublicProfile>> {
    repository::public_profile(&state.db, user_id)
        .await?
        .ok_or(AppError::NotFound)
        .map(Json)
}

// â”€â”€ Follow status â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

async fn follow_status(
    State(state): State<AppState>,
    RequireUser(me): RequireUser,
    Path(target_id): Path<i32>,
) -> AppResult<Json<serde_json::Value>> {
    let following = repository::follow_status(&state.db, me, target_id).await?;
    Ok(Json(serde_json::json!({ "following": following })))
}

// â”€â”€ Followers / following lists â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

async fn followers(
    State(state): State<AppState>,
    Path(user_id): Path<i32>,
) -> AppResult<Json<serde_json::Value>> {
    let count = repository::follower_count(&state.db, user_id).await?;
    Ok(Json(serde_json::json!({ "count": count })))
}

async fn followers_list(
    State(state): State<AppState>,
    Path(user_id): Path<i32>,
) -> AppResult<Json<Vec<UserSearchResult>>> {
    Ok(Json(repository::list_followers(&state.db, user_id).await?))
}

async fn following(
    State(state): State<AppState>,
    Path(user_id): Path<i32>,
) -> AppResult<Json<Vec<UserSearchResult>>> {
    Ok(Json(repository::list_following(&state.db, user_id).await?))
}

// â”€â”€ Follow / unfollow â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

async fn follow(
    State(state): State<AppState>,
    RequireUser(me): RequireUser,
    Path(target_id): Path<i32>,
) -> AppResult<Json<serde_json::Value>> {
    if me == target_id {
        return Err(AppError::bad_request("cannot follow yourself"));
    }
    repository::follow(&state.db, me, target_id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn unfollow(
    State(state): State<AppState>,
    RequireUser(me): RequireUser,
    Path(target_id): Path<i32>,
) -> AppResult<Json<serde_json::Value>> {
    repository::unfollow(&state.db, me, target_id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

// â”€â”€ My profile mutations â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

async fn wallet(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body): AppJson<WalletBody>,
) -> AppResult<Json<serde_json::Value>> {
    let addr = body.eth_address.trim().to_lowercase();
    if !is_valid_eth_address(&addr) {
        return Err(AppError::bad_request("invalid Ethereum address"));
    }
    repository::set_wallet(&state.db, user_id, &addr).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn social_access(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<SocialAccess>> {
    repository::social_access(&state.db, user_id)
        .await?
        .ok_or(AppError::NotFound)
        .map(Json)
}

async fn stats(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<UserStats>> {
    Ok(Json(repository::stats(&state.db, user_id).await?))
}

// â”€â”€ Notifications â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

async fn list_notifications(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<Vec<NotificationRow>>> {
    Ok(Json(repository::list_notifications(&state.db, user_id).await?))
}

async fn mark_read(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(notif_id): Path<i32>,
) -> AppResult<Json<serde_json::Value>> {
    repository::mark_read(&state.db, user_id, notif_id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn read_all(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<serde_json::Value>> {
    repository::mark_all_read(&state.db, user_id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

// â”€â”€ Feed â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

async fn feed(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<Vec<FeedItem>>> {
    Ok(Json(repository::feed(&state.db, user_id).await?))
}

// â”€â”€ Helpers â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Validate an Ethereum address: 0x-prefixed, 42 chars total, 40 hex digits.
fn is_valid_eth_address(addr: &str) -> bool {
    addr.starts_with("0x")
        && addr.len() == 42
        && addr[2..].chars().all(|c| c.is_ascii_hexdigit())
}
