use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};

use crate::{
    app::AppState,
    error::AppResult,
    http::extractors::{auth::RequireUser, json::AppJson},
    types::UserId,
};
use super::{
    service,
    types::*,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/keys/challenge",           post(challenge))
        .route("/api/keys/register",            post(register))
        .route("/api/keys/one-time",            post(upload_otpks))
        .route("/api/keys/one-time/count",      get(otpk_count))
        .route("/api/keys/bundle/{user_id}",     get(bundle_by_id))
        .route("/api/keys/bundle/by-code/{code}", get(bundle_by_code))
}

// â”€â”€ Handlers â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

async fn challenge(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<ChallengeResponse>> {
    let challenge = service::issue_challenge(&state, user_id).await?;
    Ok(Json(ChallengeResponse { challenge }))
}

async fn register(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body): AppJson<RegisterKeysBody>,
) -> AppResult<Json<serde_json::Value>> {
    service::register_keys(&state, user_id, body).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn upload_otpks(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body): AppJson<UploadOtpkBody>,
) -> AppResult<Json<serde_json::Value>> {
    let pairs = body
        .one_time_pre_keys
        .into_iter()
        .map(|k| (k.key_id, k.public_key))
        .collect();
    service::upload_otpks(&state, user_id, pairs).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn otpk_count(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<OtpkCountResponse>> {
    let count = service::otpk_count(&state, user_id).await?;
    Ok(Json(OtpkCountResponse { count }))
}

async fn bundle_by_id(
    State(state): State<AppState>,
    RequireUser(_): RequireUser,
    Path(target_id): Path<UserId>,
) -> AppResult<Json<KeyBundleResponse>> {
    Ok(Json(service::fetch_bundle(&state, target_id).await?))
}

async fn bundle_by_code(
    State(state): State<AppState>,
    RequireUser(_): RequireUser,
    Path(code): Path<String>,
) -> AppResult<Json<KeyBundleResponse>> {
    Ok(Json(service::fetch_bundle_by_code(&state, &code).await?))
}
