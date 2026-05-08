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
    http::middleware::user_rate_limit::RateLimitWindow,
};
use super::{service, types::*};

// ── Router ────────────────────────────────────────────────────────────────────

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/attestations",                    post(initiate).get(list_mine))
        .route("/api/attestations/pending",            get(list_pending))
        .route("/api/attestations/{id}/staff-sign",    post(staff_sign))
        .route("/api/attestations/{id}/reviewer-sign", post(reviewer_sign))
        .route("/api/attestations/{id}/reject",        post(reject))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// POST /api/attestations
///
/// Initiate a staff attestation for a presence-confirmed user during a visit.
/// Requesting user must be the assigned delivery staff for the visit.
#[utoipa::path(
    post, path = "/api/attestations", tag = "attestations",
    request_body = InitiateAttestationRequest,
    responses((status = 201, description = "Attestation initiated", body = VisitAttestationRow)),
    security(("bearer_auth" = [])),
)]
pub async fn initiate(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body):        AppJson<InitiateAttestationRequest>,
) -> AppResult<(StatusCode, Json<VisitAttestationRow>)> {
    state.user_rate_limiter.check(
        i32::from(user_id),
        "attestations",
        "rate_limit_attestations_per_hour",
        RateLimitWindow::Hourly,
    ).await.map_err(AppError::rate_limited)?;

    let mut tx = box_fraise_domain::transaction::RlsTransaction::begin(
        &state.db, i32::from(user_id),
    ).await?;
    let resp = service::initiate_attestation(&mut tx, &state.db, user_id, body, &state.event_bus).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(resp)))
}

/// GET /api/attestations
///
/// List all attestations for the authenticated user (as the person being attested).
#[utoipa::path(
    get, path = "/api/attestations", tag = "attestations",
    responses((status = 200, description = "Attestations for the authenticated user", body = [VisitAttestationRow])),
    security(("bearer_auth" = [])),
)]
pub async fn list_mine(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<Vec<VisitAttestationRow>>> {
    Ok(Json(service::list_my_attestations(&state.db, user_id).await?))
}

/// GET /api/attestations/pending
///
/// List attestations in `co_sign_pending` status assigned to the requesting reviewer.
#[utoipa::path(
    get, path = "/api/attestations/pending", tag = "attestations",
    responses((status = 200, description = "Pending attestations awaiting reviewer signature", body = [VisitAttestationRow])),
    security(("bearer_auth" = [])),
)]
pub async fn list_pending(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<Vec<VisitAttestationRow>>> {
    Ok(Json(service::list_pending_for_reviewer(&state.db, user_id).await?))
}

/// POST /api/attestations/:id/staff-sign
///
/// Record the delivery staff's signature — transitions attestation to `co_sign_pending`.
#[utoipa::path(
    post, path = "/api/attestations/{id}/staff-sign", tag = "attestations",
    params(("id" = i32, Path, description = "Attestation id")),
    request_body = StaffSignAttestationRequest,
    responses((status = 200, description = "Staff signature recorded", body = VisitAttestationRow)),
    security(("bearer_auth" = [])),
)]
pub async fn staff_sign(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(attestation_id): Path<i32>,
    AppJson(body):        AppJson<StaffSignAttestationRequest>,
) -> AppResult<Json<VisitAttestationRow>> {
    let mut tx = box_fraise_domain::transaction::RlsTransaction::begin(
        &state.db, i32::from(user_id),
    ).await?;
    let resp = service::staff_sign(&mut tx, &state.db, attestation_id, user_id, body, &state.event_bus).await?;
    tx.commit().await?;
    Ok(Json(resp))
}

/// POST /api/attestations/:id/reviewer-sign
///
/// Record a reviewer co-signature. Approves the attestation when both reviewers have signed.
#[utoipa::path(
    post, path = "/api/attestations/{id}/reviewer-sign", tag = "attestations",
    params(("id" = i32, Path, description = "Attestation id")),
    request_body = ReviewerSignAttestationRequest,
    responses((status = 200, description = "Reviewer signature recorded", body = VisitAttestationRow)),
    security(("bearer_auth" = [])),
)]
pub async fn reviewer_sign(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(attestation_id): Path<i32>,
    AppJson(body):        AppJson<ReviewerSignAttestationRequest>,
) -> AppResult<Json<VisitAttestationRow>> {
    let mut tx = box_fraise_domain::transaction::RlsTransaction::begin(
        &state.db, i32::from(user_id),
    ).await?;
    let resp = service::reviewer_sign(&mut tx, &state.db, attestation_id, user_id, body, &state.event_bus).await?;
    tx.commit().await?;
    Ok(Json(resp))
}

/// POST /api/attestations/:id/reject
///
/// Reject an attestation. Only an assigned reviewer may reject.
/// Returns the user to `presence_confirmed` status.
#[utoipa::path(
    post, path = "/api/attestations/{id}/reject", tag = "attestations",
    params(("id" = i32, Path, description = "Attestation id")),
    request_body = RejectAttestationRequest,
    responses((status = 200, description = "Attestation rejected", body = VisitAttestationRow)),
    security(("bearer_auth" = [])),
)]
pub async fn reject(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(attestation_id): Path<i32>,
    AppJson(body):        AppJson<RejectAttestationRequest>,
) -> AppResult<Json<VisitAttestationRow>> {
    let mut tx = box_fraise_domain::transaction::RlsTransaction::begin(
        &state.db, i32::from(user_id),
    ).await?;
    let resp = service::reject_attestation(&mut tx, &state.db, attestation_id, user_id, body, &state.event_bus).await?;
    tx.commit().await?;
    Ok(Json(resp))
}
