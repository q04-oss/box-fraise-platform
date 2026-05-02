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
        .route("/api/businesses",     post(create))
        .route("/api/businesses/me",  get(list_mine))
        .route("/api/businesses/{id}", get(get_one))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// POST /api/businesses
///
/// Register a new partner business. The requesting user must be attested
/// (BFIP Section 12). Returns 403 if not attested, 422 if validation fails.
async fn create(
    State(state):           State<AppState>,
    RequireUser(user_id):   RequireUser,
    AppJson(body):          AppJson<CreateBusinessRequest>,
) -> AppResult<(StatusCode, Json<BusinessResponse>)> {
    let name = body.name.trim();
    if name.is_empty() || name.chars().count() > 100 {
        return Err(AppError::bad_request("name must be 1–100 characters"));
    }
    if body.address.trim().is_empty() {
        return Err(AppError::unprocessable("address is required"));
    }

    let resp = service::create_business(
        &state.db, user_id, body, &state.event_bus,
    ).await?;

    Ok((StatusCode::CREATED, Json(resp)))
}

/// GET /api/businesses/me
///
/// List all businesses where the authenticated user is the primary holder.
async fn list_mine(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<Vec<BusinessResponse>>> {
    Ok(Json(service::list_my_businesses(&state.db, user_id).await?))
}

/// GET /api/businesses/:id
///
/// Fetch a single business by ID. Returns 404 if not found.
async fn get_one(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(business_id):    Path<i32>,
) -> AppResult<Json<BusinessResponse>> {
    Ok(Json(service::get_business(&state.db, business_id, user_id).await?))
}
