use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};

use crate::{
    app::AppState,
    error::{AppError, AppResult},
    http::extractors::{auth::RequireUser, json::AppJson},
};
use super::{service, types::*};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/beacons",                          post(create))
        .route("/api/beacons/business/{business_id}",   get(list))
        .route("/api/beacons/{id}/daily-uuid",          get(daily_uuid))
        .route("/api/beacons/{id}/rotate-key",          post(rotate_key))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// POST /api/beacons
///
/// Register a new beacon at a business location.
/// The requesting user must be attested and the primary holder of the business.
async fn create(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body):        AppJson<CreateBeaconRequest>,
) -> AppResult<(StatusCode, Json<BeaconResponse>)> {
    let resp = service::create_beacon(&state.db, user_id, body, &state.event_bus).await?;
    Ok((StatusCode::CREATED, Json(resp)))
}

/// GET /api/beacons/business/:business_id
///
/// List all active beacons for a business.
/// Only the business owner or a platform admin may call this endpoint.
async fn list(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(business_id):    Path<i32>,
) -> AppResult<Json<Vec<BeaconResponse>>> {
    Ok(Json(service::list_beacons(&state.db, business_id, user_id).await?))
}

/// GET /api/beacons/:id/daily-uuid
///
/// Return today's HMAC-derived UUID for a beacon (UTC day).
/// Only the business owner or a platform admin may call this endpoint.
async fn daily_uuid(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(beacon_id):      Path<i32>,
) -> AppResult<Json<DailyUuidResponse>> {
    Ok(Json(service::get_daily_uuid(&state.db, beacon_id, user_id).await?))
}

/// POST /api/beacons/:id/rotate-key
///
/// Rotate the secret key for a beacon. The old key is preserved as
/// `previous_secret_key` for a 24-hour grace period. Returns the updated beacon.
async fn rotate_key(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(beacon_id):      Path<i32>,
) -> AppResult<Json<BeaconResponse>> {
    Ok(Json(service::rotate_key(&state.db, beacon_id, user_id, &state.event_bus).await?))
}
