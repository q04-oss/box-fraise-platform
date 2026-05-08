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
    http::middleware::user_rate_limit::RateLimitWindow,
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
#[utoipa::path(
    post, path = "/api/background-checks/initiate", tag = "background-checks",
    request_body = InitiateCheckRequest,
    responses(
        (status = 201, description = "Background check initiated", body = BackgroundCheckResponse),
        (status = 400, description = "check_type or provider missing"),
        (status = 403, description = "Cooling period incomplete or required checks missing"),
        (status = 409, description = "Pending check of this type already exists"),
    ),
    security(("bearer_auth" = [])),
)]
pub async fn initiate(
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
    state.user_rate_limiter.check(
        i32::from(user_id),
        "background_checks",
        "rate_limit_background_checks_per_day",
        RateLimitWindow::Daily,
    ).await.map_err(AppError::rate_limited)?;

    // Hardening cleanup #3 — RLS-scoped per-request transaction.
    // Begin → call service with `&mut tx` → on Ok, commit; on Err, the
    // `?` operator drops `tx` and Postgres rolls back automatically.
    let mut tx = box_fraise_domain::transaction::RlsTransaction::begin(
        &state.db, i32::from(user_id),
    ).await?;
    let resp = service::initiate_check(&mut tx, &state.db, user_id, body, &state.event_bus).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(resp)))
}

/// POST /api/background-checks/webhook
///
/// Provider webhook endpoint — no authentication required.
/// The HMAC of the raw payload is stored as response_hash for integrity.
/// Always returns 200 (unknown external_check_ids are silently ignored).
#[utoipa::path(
    post, path = "/api/background-checks/webhook", tag = "background-checks",
    request_body = CheckWebhookPayload,
    responses(
        (status = 200, description = "Webhook accepted"),
        (status = 400, description = "Invalid JSON in webhook body"),
    ),
)]
pub async fn webhook(
    State(state): State<AppState>,
    body:         axum::body::Bytes,
) -> AppResult<StatusCode> {
    let payload: CheckWebhookPayload = serde_json::from_slice(&body)
        .map_err(|_| AppError::bad_request("invalid JSON in webhook body"))?;

    let hmac_key = state.cfg.hmac_shared_key
        .as_ref()
        .map(|k| k.expose_secret().to_owned())
        .unwrap_or_default();

    // Webhook bypass — stays on `&PgPool` (no `RlsTransaction`). The provider is
    // unauthenticated, so there is no JWT user id to scope RLS to. Authenticity
    // is established by the HMAC of the raw payload (`response_hash`) and the
    // route is constrained to looking up a row by its `external_check_id`.
    // RLS bypass here is by design — see service::handle_webhook docs.
    service::handle_webhook(&state.db, payload, &body, &hmac_key, &state.event_bus).await?;
    Ok(StatusCode::OK)
}

/// GET /api/background-checks/status
///
/// Return the aggregate background check status for the authenticated user.
#[utoipa::path(
    get, path = "/api/background-checks/status", tag = "background-checks",
    responses(
        (status = 200, description = "Aggregate background check status", body = BackgroundCheckStatusResponse),
    ),
    security(("bearer_auth" = [])),
)]
pub async fn status(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<BackgroundCheckStatusResponse>> {
    Ok(Json(service::get_status(&state.db, user_id).await?))
}
