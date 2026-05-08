#![deny(clippy::disallowed_methods)] // Grade A item 5 — raw SQL belongs in repository.rs (clippy.toml)
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
#[utoipa::path(
    post, path = "/api/businesses", tag = "businesses",
    request_body = CreateBusinessRequest,
    responses(
        (status = 200, description = "Business registered"),
        (status = 400, description = "name must be 1–100 characters"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "User is not attested"),
        (status = 422, description = "address is required"),
    ),
    security(("bearer_auth" = [])),
)]
pub async fn create(
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

    // Hardening cleanup #3 — RLS-scoped per-request transaction.
    // Begin → call service with `&mut tx` → on Ok, commit; on Err, the
    // `?` operator drops `tx` and Postgres rolls back automatically.
    let mut tx = box_fraise_domain::transaction::RlsTransaction::begin(
        &state.db, i32::from(user_id),
    ).await?;
    let resp = service::create_business(
        &mut tx, &state.db, user_id, body, &state.event_bus,
    ).await?;
    tx.commit().await?;

    Ok((StatusCode::CREATED, Json(resp)))
}

/// GET /api/businesses/me
///
/// List all businesses where the authenticated user is the primary holder.
#[utoipa::path(
    get, path = "/api/businesses/me", tag = "businesses",
    responses(
        (status = 200, description = "Businesses owned by the authenticated user"),
        (status = 401, description = "Unauthorized"),
    ),
    security(("bearer_auth" = [])),
)]
pub async fn list_mine(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<Vec<BusinessResponse>>> {
    Ok(Json(service::list_my_businesses(&state.db, user_id).await?))
}

/// GET /api/businesses/:id
///
/// Fetch a single business by ID. Returns 404 if not found.
#[utoipa::path(
    get, path = "/api/businesses/{id}", tag = "businesses",
    params(("id" = i32, Path, description = "Business database ID")),
    responses(
        (status = 200, description = "Business detail"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Business not found"),
    ),
    security(("bearer_auth" = [])),
)]
pub async fn get_one(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(business_id):    Path<i32>,
) -> AppResult<Json<BusinessResponse>> {
    Ok(Json(service::get_business(&state.db, business_id, user_id).await?))
}