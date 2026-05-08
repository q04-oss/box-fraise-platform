#![deny(clippy::disallowed_methods)] // Grade A item 5 — raw SQL belongs in repository.rs (clippy.toml)
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
use box_fraise_domain::transaction::RlsTransaction;
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
#[utoipa::path(
    post, path = "/api/beacons", tag = "beacons",
    request_body = CreateBeaconRequest,
    responses(
        (status = 201, description = "Beacon created"),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Caller is not the primary holder of the business or is not attested"),
    ),
    security(("bearer_auth" = [])),
)]
pub async fn create(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body):        AppJson<CreateBeaconRequest>,
) -> AppResult<(StatusCode, Json<BeaconResponse>)> {
    let mut tx = RlsTransaction::begin(&state.db, i32::from(user_id)).await?;
    let resp = service::create_beacon(&mut tx, &state.db, user_id, body, &state.event_bus).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(resp)))
}

/// GET /api/beacons/business/:business_id
///
/// List all active beacons for a business.
/// Only the business owner or a platform admin may call this endpoint.
#[utoipa::path(
    get, path = "/api/beacons/business/{business_id}", tag = "beacons",
    params(("business_id" = i32, Path, description = "Business ID to list beacons for")),
    responses(
        (status = 200, description = "List of active beacons for the business"),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Caller is not the business owner or a platform admin"),
    ),
    security(("bearer_auth" = [])),
)]
pub async fn list(
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
#[utoipa::path(
    get, path = "/api/beacons/{id}/daily-uuid", tag = "beacons",
    params(("id" = i32, Path, description = "Beacon ID")),
    responses(
        (status = 200, description = "Today's HMAC-derived UUID for the beacon"),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Caller is not the business owner or a platform admin"),
        (status = 404, description = "Beacon not found"),
    ),
    security(("bearer_auth" = [])),
)]
pub async fn daily_uuid(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(beacon_id):      Path<i32>,
) -> AppResult<Json<DailyUuidResponse>> {
    let mut tx = RlsTransaction::begin(&state.db, i32::from(user_id)).await?;
    let resp = service::get_daily_uuid(&mut tx, &state.db, beacon_id, user_id).await?;
    tx.commit().await?;
    Ok(Json(resp))
}

/// POST /api/beacons/:id/rotate-key
///
/// Rotate the secret key for a beacon. The old key is preserved as
/// `previous_secret_key` for a 24-hour grace period. Returns the updated beacon.
#[utoipa::path(
    post, path = "/api/beacons/{id}/rotate-key", tag = "beacons",
    params(("id" = i32, Path, description = "Beacon ID")),
    responses(
        (status = 200, description = "Beacon key rotated; returns updated beacon"),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Caller is not the business owner or a platform admin"),
        (status = 404, description = "Beacon not found"),
    ),
    security(("bearer_auth" = [])),
)]
pub async fn rotate_key(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(beacon_id):      Path<i32>,
) -> AppResult<Json<BeaconResponse>> {
    let mut tx = RlsTransaction::begin(&state.db, i32::from(user_id)).await?;
    let resp = service::rotate_key(&mut tx, &state.db, beacon_id, user_id, &state.event_bus).await?;
    tx.commit().await?;
    Ok(Json(resp))
}