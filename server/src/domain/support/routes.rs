use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};

use box_fraise_domain::domain::support::{
    service,
    types::{CancelBookingRequest, CreateBookingRequest, ResolveBookingRequest, SupportBookingResponse},
};
use crate::{
    app::AppState,
    error::AppResult,
    http::extractors::{auth::RequireUser, json::AppJson},
};

// ── Router ────────────────────────────────────────────────────────────────────

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/support/bookings",                        post(create_booking))
        .route("/api/support/bookings/me",                     get(get_my_bookings))
        .route("/api/support/bookings/{id}/cancel",            post(cancel_booking))
        .route("/api/support/bookings/{id}/attend",            post(attend_booking))
        .route("/api/support/bookings/{id}/resolve",           post(resolve_booking))
        .route("/api/staff/visits/{visit_id}/bookings",        get(list_bookings_for_visit))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// POST /api/support/bookings
///
/// Book a support slot at a scheduled or in-progress staff visit.
/// Returns 409 if the user already has an active booking at this visit.
/// Returns 422 if the visit is at capacity.
#[utoipa::path(
    post, path = "/api/support/bookings", tag = "support",
    request_body = CreateBookingRequest,
    responses(
        (status = 201, description = "Support booking created", body = SupportBookingResponse),
        (status = 409, description = "User already has an active booking at this visit"),
        (status = 422, description = "Visit is at capacity"),
    ),
    security(("bearer_auth" = [])),
)]
pub async fn create_booking(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body):        AppJson<CreateBookingRequest>,
) -> AppResult<(StatusCode, Json<SupportBookingResponse>)> {
    // Hardening cleanup #3 — RLS-scoped per-request transaction.
    let mut tx = box_fraise_domain::transaction::RlsTransaction::begin(
        &state.db, i32::from(user_id),
    ).await?;
    let resp = service::create_booking(
        &mut tx, &state.db, user_id, body, &state.event_bus,
    ).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(resp)))
}

/// GET /api/support/bookings/me
///
/// Return all support bookings for the authenticated user.
#[utoipa::path(
    get, path = "/api/support/bookings/me", tag = "support",
    responses(
        (status = 200, description = "All support bookings for the authenticated user", body = [SupportBookingResponse]),
    ),
    security(("bearer_auth" = [])),
)]
pub async fn get_my_bookings(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<Vec<SupportBookingResponse>>> {
    Ok(Json(service::get_my_bookings(&state.db, user_id).await?))
}

/// POST /api/support/bookings/:id/cancel
///
/// Cancel a support booking. Returns 403 if the caller is not the owner or a platform admin.
/// Returns 409 if the booking is not in booked status.
#[utoipa::path(
    post, path = "/api/support/bookings/{id}/cancel", tag = "support",
    params(("id" = i32, Path, description = "Support booking ID")),
    request_body = CancelBookingRequest,
    responses(
        (status = 200, description = "Booking cancelled", body = SupportBookingResponse),
        (status = 403, description = "Caller is not the owner or a platform admin"),
        (status = 409, description = "Booking is not in booked status"),
    ),
    security(("bearer_auth" = [])),
)]
pub async fn cancel_booking(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(booking_id):     Path<i32>,
    AppJson(body):        AppJson<CancelBookingRequest>,
) -> AppResult<Json<SupportBookingResponse>> {
    // Hardening cleanup #3 — RLS-scoped per-request transaction.
    let mut tx = box_fraise_domain::transaction::RlsTransaction::begin(
        &state.db, i32::from(user_id),
    ).await?;
    let resp = service::cancel_booking(
        &mut tx, &state.db, booking_id, user_id, body,
    ).await?;
    tx.commit().await?;
    Ok(Json(resp))
}

/// POST /api/support/bookings/:id/attend
///
/// Mark a booking as attended. Requires the caller to be the delivery_staff for the visit.
/// Returns 403 if the caller is not the visit's assigned staff.
#[utoipa::path(
    post, path = "/api/support/bookings/{id}/attend", tag = "support",
    params(("id" = i32, Path, description = "Support booking ID")),
    responses(
        (status = 200, description = "Booking marked as attended", body = SupportBookingResponse),
        (status = 403, description = "Caller is not the visit's assigned staff"),
    ),
    security(("bearer_auth" = [])),
)]
pub async fn attend_booking(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(booking_id):     Path<i32>,
) -> AppResult<Json<SupportBookingResponse>> {
    // Hardening cleanup #3 — RLS-scoped per-request transaction.
    let mut tx = box_fraise_domain::transaction::RlsTransaction::begin(
        &state.db, i32::from(user_id),
    ).await?;
    let resp = service::attend_booking(
        &mut tx, &state.db, booking_id, user_id,
    ).await?;
    tx.commit().await?;
    Ok(Json(resp))
}

/// POST /api/support/bookings/:id/resolve
///
/// Resolve a support booking. Requires the caller to be the delivery_staff for the visit.
/// Handles optional gift box logic including platform vs user coverage.
/// Returns 403 if the caller is not the visit's assigned staff.
#[utoipa::path(
    post, path = "/api/support/bookings/{id}/resolve", tag = "support",
    params(("id" = i32, Path, description = "Support booking ID")),
    request_body = ResolveBookingRequest,
    responses(
        (status = 200, description = "Booking resolved", body = SupportBookingResponse),
        (status = 403, description = "Caller is not the visit's assigned staff"),
    ),
    security(("bearer_auth" = [])),
)]
pub async fn resolve_booking(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(booking_id):     Path<i32>,
    AppJson(body):        AppJson<ResolveBookingRequest>,
) -> AppResult<Json<SupportBookingResponse>> {
    // Hardening cleanup #3 — RLS-scoped per-request transaction.
    let mut tx = box_fraise_domain::transaction::RlsTransaction::begin(
        &state.db, i32::from(user_id),
    ).await?;
    let resp = service::resolve_booking(
        &mut tx, &state.db, booking_id, user_id, body, &state.event_bus,
    ).await?;
    tx.commit().await?;
    Ok(Json(resp))
}

/// GET /api/staff/visits/:visit_id/bookings
///
/// List all support bookings for a staff visit. Requires delivery_staff role.
#[utoipa::path(
    get, path = "/api/staff/visits/{visit_id}/bookings", tag = "support",
    params(("visit_id" = i32, Path, description = "Staff visit ID")),
    responses(
        (status = 200, description = "All support bookings for the visit", body = [SupportBookingResponse]),
        (status = 403, description = "Caller is not the visit's assigned staff"),
    ),
    security(("bearer_auth" = [])),
)]
pub async fn list_bookings_for_visit(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(visit_id):       Path<i32>,
) -> AppResult<Json<Vec<SupportBookingResponse>>> {
    Ok(Json(
        service::list_bookings_for_visit(&state.db, visit_id, user_id).await?,
    ))
}
