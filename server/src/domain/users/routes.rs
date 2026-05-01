use axum::{
    extract::{Path, Query, State},
    routing::get,
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
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn search(
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

async fn public_profile(
    State(state): State<AppState>,
    Path(user_id): Path<UserId>,
) -> AppResult<Json<PublicProfile>> {
    Ok(Json(service::get_public_profile(&state.db, user_id).await?))
}
