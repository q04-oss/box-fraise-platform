use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};

use crate::{
    app::AppState,
    error::AppResult,
    http::extractors::auth::RequireUser,
};
use super::{repository, types::*};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/campaigns",                get(list))
        .route("/api/campaigns/{id}",            get(find))
        .route("/api/campaigns/{id}/signup",     post(signup).delete(cancel))
}

async fn list(State(state): State<AppState>) -> AppResult<Json<Vec<CampaignRow>>> {
    Ok(Json(repository::list_upcoming(&state.db).await?))
}

async fn find(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> AppResult<Json<CampaignRow>> {
    repository::find(&state.db, id)
        .await?
        .ok_or(crate::error::AppError::NotFound)
        .map(Json)
}

async fn signup(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(campaign_id): Path<i32>,
) -> AppResult<Json<SignupRow>> {
    Ok(Json(repository::signup(&state.db, user_id, campaign_id).await?))
}

async fn cancel(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(campaign_id): Path<i32>,
) -> AppResult<Json<serde_json::Value>> {
    repository::cancel(&state.db, user_id, campaign_id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
