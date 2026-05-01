use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};

use super::{repository, types::*};
use crate::{
    app::AppState,
    error::{AppError, AppResult},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/businesses", get(list))
        .route("/api/businesses/{id}", get(find))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn list(State(state): State<AppState>) -> AppResult<Json<Vec<BusinessRow>>> {
    Ok(Json(repository::list(&state.db).await?))
}

async fn find(State(state): State<AppState>, Path(id): Path<i32>) -> AppResult<Json<BusinessRow>> {
    repository::find(&state.db, id)
        .await?
        .ok_or(AppError::NotFound)
        .map(Json)
}
