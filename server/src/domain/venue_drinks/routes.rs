use axum::{
    extract::{ConnectInfo, Path, State},
    routing::{get, post},
    Json, Router,
};
use std::net::SocketAddr;

use crate::{
    app::AppState,
    error::AppResult,
    http::extractors::{
        auth::{RequireStaff, RequireUser},
        json::AppJson,
    },
};
use super::{service, types::*};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/businesses/{id}/drinks",         get(menu))
        .route("/api/businesses/{id}/stripe-connect", post(stripe_connect))
        .route("/api/venue-orders",                   post(create_order))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn menu(
    State(state):      State<AppState>,
    Path(business_id): Path<i32>,
) -> AppResult<Json<Vec<DrinkRow>>> {
    Ok(Json(service::get_menu(&state, business_id).await?))
}

async fn stripe_connect(
    State(state):         State<AppState>,
    Path(business_id):    Path<i32>,
    RequireStaff(claims): RequireStaff,
    ConnectInfo(addr):    ConnectInfo<SocketAddr>,
) -> AppResult<Json<ConnectOnboardingResponse>> {
    if claims.business_id != business_id {
        return Err(crate::error::AppError::Forbidden);
    }
    Ok(Json(service::onboard_stripe_connect(
        &state, claims.user_id, business_id, Some(addr.ip())
    ).await?))
}

async fn create_order(
    State(state):      State<AppState>,
    RequireUser(uid):  RequireUser,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    AppJson(body):     AppJson<CreateVenueOrderBody>,
) -> AppResult<Json<VenueOrderResponse>> {
    Ok(Json(service::create_order(&state, uid, body, Some(addr.ip())).await?))
}
