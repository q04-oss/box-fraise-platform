use axum::{
    extract::{Path, Query, State},
    routing::get,
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
        .route("/api/varieties",                    get(varieties))
        .route("/api/varieties/passport",           get(passport))
        .route("/api/locations",                    get(locations))
        .route("/api/locations/:id/batch-status",   get(batch_status))
        .route("/api/slots",                        get(slots))
        .route("/api/time-slots",                   get(time_slots))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn varieties(State(state): State<AppState>) -> AppResult<Json<Vec<VarietyRow>>> {
    Ok(Json(repository::list_varieties(&state.db).await?))
}

async fn passport(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<Vec<VarietyRow>>> {
    Ok(Json(repository::user_passport(&state.db, user_id).await?))
}

async fn locations(State(state): State<AppState>) -> AppResult<Json<Vec<LocationRow>>> {
    Ok(Json(repository::list_locations(&state.db).await?))
}

async fn batch_status(
    State(state): State<AppState>,
    Path(location_id): Path<i32>,
) -> AppResult<Json<Vec<BatchStatusEntry>>> {
    Ok(Json(repository::batch_status(&state.db, location_id).await?))
}

async fn slots(
    State(state): State<AppState>,
    Query(q): Query<SlotsQuery>,
) -> AppResult<Json<Vec<TimeSlotRow>>> {
    Ok(Json(
        repository::get_or_generate_slots(&state.db, q.location_id, &q.date).await?,
    ))
}

async fn time_slots(
    State(state): State<AppState>,
    Query(q): Query<TimeSlotsQuery>,
) -> AppResult<Json<Vec<TimeSlotRow>>> {
    Ok(Json(
        repository::available_slots(&state.db, q.location_id, q.date.as_deref()).await?,
    ))
}
