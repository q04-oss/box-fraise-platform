use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use secrecy::ExposeSecret;

use crate::{
    app::AppState,
    error::{AppError, AppResult},
    http::extractors::{auth::RequireUser, json::AppJson},
};
use super::{service, types::*};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/background-checks/initiate", post(initiate))
        .route("/api/background-checks/webhook",  post(webhook))
        .route("/api/background-checks/status",   get(status))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// POST /api/background-checks/initiate
///
/// Initiate a background check for the authenticated user.
/// Requires identity_confirmed status and completed cooling period.
/// Returns 403 if cooling is not complete or required checks are missing.
/// Returns 409 if a pending check of this type already exists.
async fn initiate(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body):        AppJson<InitiateCheckRequest>,
) -> AppResult<(StatusCode, Json<BackgroundCheckResponse>)> {
    if body.check_type.trim().is_empty() {
        return Err(AppError::bad_request("check_type is required"));
    }
    if body.provider.trim().is_empty() {
        return Err(AppError::bad_request("provider is required"));
    }
    let resp = service::initiate_check(&state.db, user_id, body, &state.event_bus).await?;
    Ok((StatusCode::CREATED, Json(resp)))
}

/// POST /api/background-checks/webhook
///
/// Provider webhook endpoint — no authentication required.
/// The HMAC of the raw payload is stored as response_hash for integrity.
/// Always returns 200 (unknown external_check_ids are silently ignored).
async fn webhook(
    State(state): State<AppState>,
    body:         axum::body::Bytes,
) -> AppResult<StatusCode> {
    let payload: CheckWebhookPayload = serde_json::from_slice(&body)
        .map_err(|_| AppError::bad_request("invalid JSON in webhook body"))?;

    let hmac_key = state.cfg.hmac_shared_key
        .as_ref()
        .map(|k| k.expose_secret().to_owned())
        .unwrap_or_default();

    service::handle_webhook(&state.db, payload, &body, &hmac_key, &state.event_bus).await?;
    Ok(StatusCode::OK)
}

/// GET /api/background-checks/status
///
/// Return the aggregate background check status for the authenticated user.
async fn status(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<BackgroundCheckStatusResponse>> {
    Ok(Json(service::get_status(&state.db, user_id).await?))
}
