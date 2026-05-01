use axum::{
    extract::State,
    routing::get,
    Json, Router,
};

use super::{repository, types::*};
use crate::{app::AppState, error::AppResult};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/businesses", get(list))
}

async fn list(State(state): State<AppState>) -> AppResult<Json<Vec<BusinessRow>>> {
    Ok(Json(repository::list(&state.db).await?))
}
