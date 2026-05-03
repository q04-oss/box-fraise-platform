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

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/staff/roles",                          post(grant_role))
        .route("/api/staff/roles/me",                       get(my_roles))
        .route("/api/staff/visits",                         post(schedule_visit).get(list_visits))
        .route("/api/staff/visits/{id}/arrive",              post(arrive))
        .route("/api/staff/visits/{id}/complete",            post(complete))
        .route("/api/staff/visits/{id}/quality-assessment",  post(quality_assessment))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// POST /api/staff/roles
///
/// Grant a staff role to a user. Requires platform_admin.
async fn grant_role(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body):        AppJson<GrantRoleRequest>,
) -> AppResult<(StatusCode, Json<StaffRoleResponse>)> {
    let resp = service::grant_staff_role(&state.db, user_id, body, &state.event_bus).await?;
    Ok((StatusCode::CREATED, Json(resp)))
}

/// GET /api/staff/roles/me
///
/// List the authenticated user's own active staff roles.
async fn my_roles(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<Vec<StaffRoleResponse>>> {
    Ok(Json(service::get_my_roles(&state.db, user_id).await?))
}

/// POST /api/staff/visits
///
/// Schedule a new staff visit. Requires delivery_staff or platform_admin.
async fn schedule_visit(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body):        AppJson<ScheduleVisitRequest>,
) -> AppResult<(StatusCode, Json<StaffVisitResponse>)> {
    let resp = service::schedule_visit(&state.db, user_id, body, &state.event_bus).await?;
    Ok((StatusCode::CREATED, Json(resp)))
}

/// GET /api/staff/visits
///
/// List visits. Platform admins see all; delivery staff see only their own.
async fn list_visits(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<Vec<StaffVisitResponse>>> {
    Ok(Json(service::list_visits(&state.db, user_id).await?))
}

/// POST /api/staff/visits/:id/arrive
///
/// Record arrival at a scheduled visit — sets status to in_progress.
/// Only the assigned staff member may call this.
async fn arrive(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(visit_id):       Path<i32>,
    AppJson(body):        AppJson<ArriveAtVisitRequest>,
) -> AppResult<Json<StaffVisitResponse>> {
    Ok(Json(service::arrive_at_visit(&state.db, visit_id, user_id, body).await?))
}

/// POST /api/staff/visits/:id/complete
///
/// Mark a visit completed with box count and evidence.
/// Only the assigned staff member or a platform_admin may call this.
async fn complete(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(visit_id):       Path<i32>,
    AppJson(body):        AppJson<CompleteVisitRequest>,
) -> AppResult<Json<StaffVisitResponse>> {
    Ok(Json(service::complete_visit(&state.db, visit_id, user_id, body, &state.event_bus).await?))
}

/// POST /api/staff/visits/:id/quality-assessment
///
/// Submit a quality assessment for a business during a staff visit.
/// Returns 201 on success.
async fn quality_assessment(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(visit_id):       Path<i32>,
    AppJson(body):        AppJson<QualityAssessmentRequest>,
) -> AppResult<(StatusCode, Json<QualityAssessmentRow>)> {
    let resp = service::submit_quality_assessment(
        &state.db, visit_id, user_id, body, &state.event_bus,
    ).await?;
    Ok((StatusCode::CREATED, Json(resp)))
}
