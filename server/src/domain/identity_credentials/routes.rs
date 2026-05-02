use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
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
        .route("/api/identity/verify",            post(initiate_verification))
        .route("/api/identity/webhook/stripe",    post(stripe_webhook))
        .route("/api/identity/cooling/app-open",  post(app_open))
        .route("/api/identity/cooling/status",    get(cooling_status))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// POST /api/identity/verify
///
/// Record a successful Stripe Identity verification. Called by the iOS app
/// after Stripe confirms identity on the client. Starts the 7-day cooling period.
async fn initiate_verification(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body):        AppJson<InitiateVerificationRequest>,
) -> AppResult<(StatusCode, Json<IdentityCredentialResponse>)> {
    if body.stripe_session_id.trim().is_empty() {
        return Err(AppError::bad_request("stripe_session_id is required"));
    }
    let resp = service::initiate_verification(&state.db, user_id, body, &state.event_bus).await?;
    Ok((StatusCode::CREATED, Json(resp)))
}

/// POST /api/identity/webhook/stripe
///
/// Stripe Identity webhook endpoint. No authentication — validated by HMAC
/// signature in the `Stripe-Signature` header.
async fn stripe_webhook(
    State(state): State<AppState>,
    headers:      HeaderMap,
    body:         axum::body::Bytes,
) -> AppResult<StatusCode> {
    let sig = headers
        .get("stripe-signature")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::bad_request("missing Stripe-Signature header"))?;

    let secret = state.cfg.stripe_webhook_secret.expose_secret();

    service::handle_stripe_webhook(&state.db, &body, sig, secret).await?;
    Ok(StatusCode::OK)
}

/// POST /api/identity/cooling/app-open
///
/// Record a cooling-period app open. Idempotent within the same calendar day.
async fn app_open(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body):        AppJson<RecordAppOpenRequest>,
) -> AppResult<Json<CoolingStatusResponse>> {
    let resp = service::record_app_open(&state.db, user_id, body, &state.event_bus).await?;
    Ok(Json(resp))
}

/// GET /api/identity/cooling/status
///
/// Return the current cooling period status. Returns 404 if the user has
/// not yet initiated identity verification.
async fn cooling_status(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<CoolingStatusResponse>> {
    let resp = service::get_cooling_status(&state.db, user_id).await?;
    Ok(Json(resp))
}
