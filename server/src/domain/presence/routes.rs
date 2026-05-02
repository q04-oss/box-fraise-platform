use axum::{
    extract::State,
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
        .route("/api/presence/beacon-dwell", post(beacon_dwell))
        .route("/api/presence/nfc-tap",      post(nfc_tap))
        .route("/api/presence/status",       get(status))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// POST /api/presence/beacon-dwell
///
/// Record a BLE beacon dwell event. Validates witness HMAC, RSSI, and
/// dwell duration per BFIP Section 5. Returns the current presence threshold state.
async fn beacon_dwell(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body):        AppJson<RecordBeaconDwellRequest>,
) -> AppResult<Json<PresenceStatusResponse>> {
    if body.dwell_minutes <= 0 {
        return Err(AppError::bad_request("dwell_minutes must be positive"));
    }
    let resp = service::record_beacon_dwell(&state.db, user_id, body, &state.event_bus).await?;
    Ok(Json(resp))
}

/// POST /api/presence/nfc-tap
///
/// Record an NFC tap of a visit box. Single-use — a box may only be tapped once.
/// Returns 409 if the box has already been tapped.
async fn nfc_tap(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body):        AppJson<RecordNfcTapRequest>,
) -> AppResult<(StatusCode, Json<PresenceStatusResponse>)> {
    let resp = service::record_nfc_tap(&state.db, user_id, body, &state.event_bus).await?;
    Ok((StatusCode::OK, Json(resp)))
}

/// GET /api/presence/status
///
/// Return the current presence threshold state for the authenticated user.
/// Returns 404 if the user has not started any presence verification.
async fn status(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<PresenceStatusResponse>> {
    let resp = service::get_presence_status(&state.db, user_id).await?;
    if resp.event_count == 0 && resp.days_count == 0 {
        return Err(AppError::NotFound);
    }
    Ok(Json(resp))
}
