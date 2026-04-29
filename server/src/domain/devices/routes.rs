use axum::{
    extract::{Path, State},
    routing::{get, patch, post},
    Json, Router,
};

use crate::{
    app::AppState,
    error::AppResult,
    http::extractors::{auth::{RequireDevice, RequireUser}, json::AppJson},
};
use super::{repository, service, types::*};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/devices/pair-token",      post(pair_token))
        .route("/api/devices/register",        post(register))
        .route("/api/devices/me",              get(device_me))
        .route("/api/devices/:address/role",   patch(update_role))
        .route("/api/devices",                 get(list))
        .route("/api/devices/attest",          post(attest))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn pair_token(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<PairTokenResponse>> {
    let token = service::create_pair_token(&state, user_id).await?;
    Ok(Json(PairTokenResponse { token }))
}

async fn register(
    State(state): State<AppState>,
    AppJson(body): AppJson<RegisterDeviceBody>,
) -> AppResult<Json<DeviceRow>> {
    let device = service::register_device(
        &state,
        &body.device_address,
        &body.signature,
        &body.pairing_token,
    )
    .await?;
    Ok(Json(device))
}

async fn device_me(
    State(state): State<AppState>,
    RequireDevice(info): RequireDevice,
) -> AppResult<Json<serde_json::Value>> {
    Ok(Json(serde_json::json!({
        "device_id":      info.id,
        "device_address": info.address,
        "role":           info.role,
        "user_id":        info.user_id,
    })))
}

async fn update_role(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(address): Path<String>,
    AppJson(body): AppJson<UpdateRoleBody>,
) -> AppResult<Json<serde_json::Value>> {
    service::update_role(&state, user_id, &address, &body.role).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn list(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<Vec<DeviceRow>>> {
    let devices = repository::list_devices(&state.db, user_id).await?;
    Ok(Json(devices))
}

async fn attest(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body): AppJson<AttestBody>,
) -> AppResult<Json<serde_json::Value>> {
    service::store_attestation(
        &state,
        user_id,
        &body.key_id,
        &body.attestation,
        &body.hmac_key,
    )
    .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
