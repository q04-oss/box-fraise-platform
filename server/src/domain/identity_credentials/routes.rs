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
#[utoipa::path(
    post, path = "/api/identity/verify", tag = "identity",
    request_body = InitiateVerificationRequest,
    responses(
        (status = 201, description = "Verification recorded; cooling period started"),
        (status = 400, description = "stripe_session_id is required"),
    ),
    security(("bearer_auth" = [])),
)]
pub async fn initiate_verification(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body):        AppJson<InitiateVerificationRequest>,
) -> AppResult<(StatusCode, Json<IdentityCredentialResponse>)> {
    if body.stripe_session_id.trim().is_empty() {
        return Err(AppError::bad_request("stripe_session_id is required"));
    }
    // Hardening cleanup #3 — RLS-scoped per-request transaction.
    let mut tx = box_fraise_domain::transaction::RlsTransaction::begin(
        &state.db, i32::from(user_id),
    ).await?;
    let resp = service::initiate_verification(&mut tx, &state.db, user_id, body, &state.event_bus).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(resp)))
}

/// POST /api/identity/webhook/stripe
///
/// Stripe Identity webhook endpoint. No authentication — validated by HMAC
/// signature in the `Stripe-Signature` header.
#[utoipa::path(
    post, path = "/api/identity/webhook/stripe", tag = "identity",
    request_body(
        content = String,
        content_type = "application/json",
        description = "Raw Stripe webhook payload (signature verified server-side via STRIPE_WEBHOOK_SECRET)",
    ),
    responses(
        (status = 200, description = "Webhook accepted"),
        (status = 400, description = "Missing Stripe-Signature header"),
        (status = 401, description = "Stripe signature invalid"),
    ),
)]
pub async fn stripe_webhook(
    State(state): State<AppState>,
    headers:      HeaderMap,
    body:         axum::body::Bytes,
) -> AppResult<StatusCode> {
    let sig = headers
        .get("stripe-signature")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::bad_request("missing Stripe-Signature header"))?;

    let secret = state.cfg.stripe_webhook_secret.expose_secret();

    // Hardening cleanup #3 — webhook stays on `&PgPool`. There is no user
    // JWT on this endpoint (the request is authenticated by HMAC, not by a
    // JWT identifying a user), so there is no `user_id` to scope an
    // `RlsTransaction` to. The service runs as a service-account caller.
    service::handle_stripe_webhook(&state.db, &body, sig, secret).await?;
    Ok(StatusCode::OK)
}

/// POST /api/identity/cooling/app-open
///
/// Record a cooling-period app open. Idempotent within the same calendar day.
#[utoipa::path(
    post, path = "/api/identity/cooling/app-open", tag = "identity",
    request_body = RecordAppOpenRequest,
    responses((status = 200, description = "App open recorded; returns updated cooling status")),
    security(("bearer_auth" = [])),
)]
pub async fn app_open(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body):        AppJson<RecordAppOpenRequest>,
) -> AppResult<Json<CoolingStatusResponse>> {
    // Hardening cleanup #3 — RLS-scoped per-request transaction.
    let mut tx = box_fraise_domain::transaction::RlsTransaction::begin(
        &state.db, i32::from(user_id),
    ).await?;
    let resp = service::record_app_open(&mut tx, &state.db, user_id, body, &state.event_bus).await?;
    tx.commit().await?;
    Ok(Json(resp))
}

/// GET /api/identity/cooling/status
///
/// Return the current cooling period status. Returns 404 if the user has
/// not yet initiated identity verification.
#[utoipa::path(
    get, path = "/api/identity/cooling/status", tag = "identity",
    responses(
        (status = 200, description = "Current cooling status"),
        (status = 404, description = "User has not initiated identity verification"),
    ),
    security(("bearer_auth" = [])),
)]
pub async fn cooling_status(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<CoolingStatusResponse>> {
    let resp = service::get_cooling_status(&state.db, user_id).await?;
    Ok(Json(resp))
}
