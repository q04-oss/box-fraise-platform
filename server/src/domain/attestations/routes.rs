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
async fn initiate(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body):        AppJson<InitiateAttestationRequest>,
) -> AppResult<(StatusCode, Json<VisitAttestationRow>)> {
    let resp = service::initiate_attestation(&state.db, user_id, body, &state.event_bus).await?;
    Ok((StatusCode::CREATED, Json(resp)))
}

/// GET /api/attestations
///
/// List all attestations for the authenticated user (as the person being attested).
async fn list_mine(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<Vec<VisitAttestationRow>>> {
    Ok(Json(service::list_my_attestations(&state.db, user_id).await?))
}

/// GET /api/attestations/pending
///
/// List attestations in `co_sign_pending` status assigned to the requesting reviewer.
async fn list_pending(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<Vec<VisitAttestationRow>>> {
    Ok(Json(service::list_pending_for_reviewer(&state.db, user_id).await?))
}

/// POST /api/attestations/:id/staff-sign
///
/// Record the delivery staff's signature — transitions attestation to `co_sign_pending`.
async fn staff_sign(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(attestation_id): Path<i32>,
    AppJson(body):        AppJson<StaffSignAttestationRequest>,
) -> AppResult<Json<VisitAttestationRow>> {
    Ok(Json(
        service::staff_sign(&state.db, attestation_id, user_id, body, &state.event_bus).await?,
    ))
}

/// POST /api/attestations/:id/reviewer-sign
///
/// Record a reviewer co-signature. Approves the attestation when both reviewers have signed.
async fn reviewer_sign(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(attestation_id): Path<i32>,
    AppJson(body):        AppJson<ReviewerSignAttestationRequest>,
) -> AppResult<Json<VisitAttestationRow>> {
    Ok(Json(
        service::reviewer_sign(&state.db, attestation_id, user_id, body, &state.event_bus).await?,
    ))
}

/// POST /api/attestations/:id/reject
///
/// Reject an attestation. Only an assigned reviewer may reject.
/// Returns the user to `presence_confirmed` status.
async fn reject(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(attestation_id): Path<i32>,
    AppJson(body):        AppJson<RejectAttestationRequest>,
) -> AppResult<Json<VisitAttestationRow>> {
    Ok(Json(
        service::reject_attestation(&state.db, attestation_id, user_id, body, &state.event_bus)
            .await?,
    ))
}
