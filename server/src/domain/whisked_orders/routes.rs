#![deny(clippy::disallowed_methods)] // Grade A item 5 — raw SQL belongs in repository.rs (clippy.toml)
//! Whisked order routes.
//!
//! Customer surface (`POST /api/whisked/orders`, `GET …/:id`, `GET …/:id/pickup-code`)
//! requires a regular authenticated user. Staff surface (`PATCH …/:id/status`,
//! `POST …/:id/validate`, `GET /api/whisked/business/:business_id/orders`)
//! also requires authentication; ownership is enforced inside the service
//! against `businesses.primary_holder_id`.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, patch, post},
    Json, Router,
};

use box_fraise_domain::transaction::RlsTransaction;
use secrecy::ExposeSecret;

use crate::{
    app::AppState,
    error::AppResult,
    http::extractors::{auth::RequireUser, json::AppJson},
};
use super::{service, types::*};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/whisked/orders",                       post(place))
        .route("/api/whisked/orders/{id}",                  get(get_one))
        .route("/api/whisked/orders/{id}/pickup-code",      get(pickup_code))
        .route("/api/whisked/orders/{id}/validate",         post(validate))
        .route("/api/whisked/orders/{id}/status",           patch(update_status))
        .route("/api/whisked/business/{business_id}/orders", get(list_for_business))
}

// ── Customer surface ─────────────────────────────────────────────────────────

/// POST /api/whisked/orders — place a new order. Returns the order with its
/// freshly-minted pickup code; the iOS client surfaces the code on the
/// Order tab once the staff dashboard advances status to `ready`.
#[utoipa::path(
    post, path = "/api/whisked/orders", tag = "whisked",
    request_body = PlaceOrderRequest,
    responses(
        (status = 201, description = "Order placed", body = WhiskedOrderResponse),
        (status = 400, description = "Invalid request"),
        (status = 404, description = "Business not found"),
    ),
    security(("bearer_auth" = [])),
)]
pub async fn place(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body):        AppJson<PlaceOrderRequest>,
) -> AppResult<(StatusCode, Json<WhiskedOrderResponse>)> {
    let mut tx = RlsTransaction::begin(&state.db, i32::from(user_id)).await?;
    let resp = service::place_order(
        &mut tx,
        &state.db,
        &state.http,
        i32::from(user_id),
        body,
        state.cfg.stripe_secret_key.expose_secret(),
    ).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(resp)))
}

/// GET /api/whisked/orders/:id — fetch a single order with line items.
#[utoipa::path(
    get, path = "/api/whisked/orders/{id}", tag = "whisked",
    params(("id" = i32, Path, description = "Order id")),
    responses(
        (status = 200, description = "Order detail", body = WhiskedOrderResponse),
        (status = 403, description = "Not the order's owner"),
        (status = 404, description = "Order not found"),
    ),
    security(("bearer_auth" = [])),
)]
pub async fn get_one(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(order_id):       Path<i32>,
) -> AppResult<Json<WhiskedOrderResponse>> {
    let mut tx = RlsTransaction::begin(&state.db, i32::from(user_id)).await?;
    let resp = service::get_order(&mut tx, &state.db, i32::from(user_id), order_id).await?;
    tx.commit().await?;
    Ok(Json(resp))
}

/// GET /api/whisked/orders/:id/pickup-code — convenience accessor returning
/// just the pickup code. Same authorization as `GET …/:id`.
#[utoipa::path(
    get, path = "/api/whisked/orders/{id}/pickup-code", tag = "whisked",
    params(("id" = i32, Path, description = "Order id")),
    responses(
        (status = 200, description = "Pickup code", body = PickupCodeResponse),
        (status = 403, description = "Not the order's owner"),
        (status = 404, description = "Order not found"),
    ),
    security(("bearer_auth" = [])),
)]
pub async fn pickup_code(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(order_id):       Path<i32>,
) -> AppResult<Json<PickupCodeResponse>> {
    let mut tx = RlsTransaction::begin(&state.db, i32::from(user_id)).await?;
    let detail = service::get_order(&mut tx, &state.db, i32::from(user_id), order_id).await?;
    tx.commit().await?;
    Ok(Json(PickupCodeResponse { pickup_code: detail.pickup_code }))
}

// ── Staff surface ────────────────────────────────────────────────────────────

/// POST /api/whisked/orders/:id/validate — atomically consume the pickup
/// code and mark the order collected. Caller must own the business.
#[utoipa::path(
    post, path = "/api/whisked/orders/{id}/validate", tag = "whisked",
    params(("id" = i32, Path, description = "Order id")),
    request_body = ValidatePickupRequest,
    responses(
        (status = 200, description = "Order collected", body = WhiskedOrderRow),
        (status = 400, description = "Order is not ready for pickup"),
        (status = 403, description = "Wrong code or not the business owner"),
        (status = 409, description = "Pickup code already used"),
    ),
    security(("bearer_auth" = [])),
)]
pub async fn validate(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(order_id):       Path<i32>,
    AppJson(body):        AppJson<ValidatePickupRequest>,
) -> AppResult<Json<WhiskedOrderRow>> {
    let row = service::validate_pickup_request(
        &state.db, i32::from(user_id), order_id, body,
    ).await?;
    Ok(Json(row))
}

/// PATCH /api/whisked/orders/:id/status — advance the prep timeline.
/// Allowed transitions: `pending → preparing` and `preparing → ready`.
#[utoipa::path(
    patch, path = "/api/whisked/orders/{id}/status", tag = "whisked",
    params(("id" = i32, Path, description = "Order id")),
    request_body = UpdateOrderStatusRequest,
    responses(
        (status = 200, description = "Status updated", body = WhiskedOrderRow),
        (status = 400, description = "Invalid transition"),
        (status = 403, description = "Not the business owner"),
    ),
    security(("bearer_auth" = [])),
)]
pub async fn update_status(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(order_id):       Path<i32>,
    AppJson(body):        AppJson<UpdateOrderStatusRequest>,
) -> AppResult<Json<WhiskedOrderRow>> {
    let row = service::update_order_status(
        &state.db, i32::from(user_id), order_id, &body.status,
    ).await?;
    Ok(Json(row))
}

/// GET /api/whisked/business/:business_id/orders — every in-flight order
/// for a business (pending / preparing / ready), newest first.
#[utoipa::path(
    get, path = "/api/whisked/business/{business_id}/orders", tag = "whisked",
    params(("business_id" = i32, Path, description = "Business id")),
    responses(
        (status = 200, description = "Active orders", body = [WhiskedOrderResponse]),
        (status = 403, description = "Not the business owner"),
    ),
    security(("bearer_auth" = [])),
)]
pub async fn list_for_business(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(business_id):    Path<i32>,
) -> AppResult<Json<Vec<WhiskedOrderResponse>>> {
    let rows = service::list_active_for_business(
        &state.db, i32::from(user_id), business_id,
    ).await?;
    Ok(Json(rows))
}
