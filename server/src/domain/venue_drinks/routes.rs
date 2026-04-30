use axum::{
    body::Bytes,
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use secrecy::ExposeSecret;
use std::net::SocketAddr;

use crate::{
    app::AppState,
    error::AppResult,
    http::extractors::{
        auth::{RequireStaff, RequireUser},
        json::AppJson,
    },
    integrations::square,
};
use super::{service, types::*};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/businesses/{id}/drinks",         get(menu))
        .route("/api/businesses/{id}/stripe-connect", post(stripe_connect))
        .route("/api/venue-orders",                   post(create_order))
        .route("/api/webhooks/square/orders",         post(square_order_webhook))
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

// ── Square order.updated webhook ──────────────────────────────────────────────

async fn square_order_webhook(
    State(state): State<AppState>,
    headers:      HeaderMap,
    body:         Bytes,
) -> StatusCode {
    // Reject immediately if Square order webhooks are not configured.
    let (signing_key, notification_url) = match (
        state.cfg.square_order_webhook_signing_key.as_ref(),
        state.cfg.square_order_notification_url.as_deref(),
    ) {
        (Some(k), Some(u)) => (k.expose_secret().to_owned(), u.to_owned()),
        _ => {
            tracing::warn!("square order webhook received but SQUARE_ORDER_WEBHOOK_SIGNING_KEY not configured");
            return StatusCode::SERVICE_UNAVAILABLE;
        }
    };

    // Validate signature before touching the body.
    let sig = match headers
        .get("x-square-hmacsha256-signature")
        .and_then(|v| v.to_str().ok())
    {
        Some(s) => s.to_owned(),
        None    => return StatusCode::UNAUTHORIZED,
    };

    if !square::validate_webhook(&signing_key, &notification_url, &body, &sig) {
        tracing::warn!("square order webhook signature validation failed");
        return StatusCode::UNAUTHORIZED;
    }

    let event: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v)  => v,
        Err(_) => return StatusCode::BAD_REQUEST,
    };

    // Only handle order.updated where state transitions to COMPLETED.
    let event_type = event["type"].as_str().unwrap_or("");
    if event_type != "order.updated" {
        return StatusCode::OK; // acknowledged, not handled
    }

    let state_val = event["data"]["object"]["order_updated"]["state"].as_str().unwrap_or("");
    if state_val != "COMPLETED" {
        return StatusCode::OK; // OPEN or CANCELED — nothing to do
    }

    let square_order_id = match event["data"]["object"]["order_updated"]["order_id"].as_str() {
        Some(id) => id.to_owned(),
        None     => return StatusCode::BAD_REQUEST,
    };

    // Fire-and-forget — always return 200 so Square doesn't retry endlessly.
    // Failures are logged inside complete_order_from_square.
    tokio::spawn(async move {
        service::complete_order_from_square(&state, &square_order_id).await;
    });

    StatusCode::OK
}
