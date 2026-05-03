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
async fn create_booking(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body):        AppJson<CreateBookingRequest>,
) -> AppResult<(StatusCode, Json<SupportBookingResponse>)> {
    let resp = service::create_booking(&state.db, user_id, body, &state.event_bus).await?;
    Ok((StatusCode::CREATED, Json(resp)))
}

/// GET /api/support/bookings/me
///
/// Return all support bookings for the authenticated user.
async fn get_my_bookings(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<Vec<SupportBookingResponse>>> {
    Ok(Json(service::get_my_bookings(&state.db, user_id).await?))
}

/// POST /api/support/bookings/:id/cancel
///
/// Cancel a support booking. Returns 403 if the caller is not the owner or a platform admin.
/// Returns 409 if the booking is not in booked status.
async fn cancel_booking(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(booking_id):     Path<i32>,
    AppJson(body):        AppJson<CancelBookingRequest>,
) -> AppResult<Json<SupportBookingResponse>> {
    Ok(Json(
        service::cancel_booking(&state.db, booking_id, user_id, body).await?,
    ))
}

/// POST /api/support/bookings/:id/attend
///
/// Mark a booking as attended. Requires the caller to be the delivery_staff for the visit.
/// Returns 403 if the caller is not the visit's assigned staff.
async fn attend_booking(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(booking_id):     Path<i32>,
) -> AppResult<Json<SupportBookingResponse>> {
    Ok(Json(
        service::attend_booking(&state.db, booking_id, user_id).await?,
    ))
}

/// POST /api/support/bookings/:id/resolve
///
/// Resolve a support booking. Requires the caller to be the delivery_staff for the visit.
/// Handles optional gift box logic including platform vs user coverage.
/// Returns 403 if the caller is not the visit's assigned staff.
async fn resolve_booking(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(booking_id):     Path<i32>,
    AppJson(body):        AppJson<ResolveBookingRequest>,
) -> AppResult<Json<SupportBookingResponse>> {
    Ok(Json(
        service::resolve_booking(&state.db, booking_id, user_id, body, &state.event_bus).await?,
    ))
}

/// GET /api/staff/visits/:visit_id/bookings
///
/// List all support bookings for a staff visit. Requires delivery_staff role.
async fn list_bookings_for_visit(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(visit_id):       Path<i32>,
) -> AppResult<Json<Vec<SupportBookingResponse>>> {
    Ok(Json(
        service::list_bookings_for_visit(&state.db, visit_id, user_id).await?,
    ))
}
