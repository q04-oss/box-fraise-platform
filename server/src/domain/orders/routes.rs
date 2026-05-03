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
        .route("/api/orders",                            post(create).get(list_mine))
        .route("/api/orders/collect",                    post(collect))
        .route("/api/orders/{id}/cancel",                post(cancel))
        .route("/api/staff/visits/{visit_id}/boxes/activate", post(activate_box))
        .route("/api/staff/visits/{visit_id}/boxes",     get(list_boxes))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// POST /api/orders
///
/// Create a new strawberry order for the authenticated user.
async fn create(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body):        AppJson<CreateOrderRequest>,
) -> AppResult<(StatusCode, Json<OrderResponse>)> {
    let resp = service::create_order(&state.db, user_id, body, &state.event_bus).await?;
    Ok((StatusCode::CREATED, Json(resp)))
}

/// GET /api/orders/me
///
/// Return all orders for the authenticated user.
async fn list_mine(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<Vec<OrderResponse>>> {
    Ok(Json(service::get_my_orders(&state.db, user_id).await?))
}

/// POST /api/orders/collect
///
/// Collect an order by tapping an NFC box chip.
/// Returns 409 if the box has already been tapped (clone detected).
async fn collect(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body):        AppJson<CollectOrderRequest>,
) -> AppResult<Json<OrderResponse>> {
    Ok(Json(
        service::collect_order(&state.db, user_id, body, &state.event_bus).await?,
    ))
}

/// POST /api/orders/:id/cancel
///
/// Cancel a pending or paid order. Returns 403 if the user does not own the order,
/// 409 if the order is already collected.
async fn cancel(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(order_id):       Path<i32>,
) -> AppResult<Json<OrderResponse>> {
    Ok(Json(
        service::cancel_order(&state.db, order_id, user_id).await?,
    ))
}

/// POST /api/staff/visits/:visit_id/boxes/activate
///
/// Activate an NFC box chip during a staff visit. Requires `delivery_staff` role.
async fn activate_box(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(visit_id):       Path<i32>,
    AppJson(body):        AppJson<ActivateBoxRequest>,
) -> AppResult<(StatusCode, Json<VisitBoxResponse>)> {
    let resp = service::activate_box(&state.db, visit_id, user_id, body).await?;
    Ok((StatusCode::CREATED, Json(resp)))
}

/// GET /api/staff/visits/:visit_id/boxes
///
/// List all NFC boxes for a staff visit. Requires `delivery_staff` role.
async fn list_boxes(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(visit_id):       Path<i32>,
) -> AppResult<Json<Vec<VisitBoxResponse>>> {
    Ok(Json(
        service::list_boxes_for_visit(&state.db, visit_id, user_id).await?,
    ))
}
