#![deny(clippy::disallowed_methods)] // Grade A item 5 — raw SQL belongs in repository.rs (clippy.toml)
//! Billing webhook routes (Hardening §10 / Grade A item 3).

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::post,
    Router,
};
use secrecy::ExposeSecret;

use box_fraise_domain::domain::billing::service;
use crate::{
    app::AppState,
    error::{AppError, AppResult},
};

/// Webhook router — split out of `router()` so `app.rs` can wrap it in
/// the 60s `webhook_routes` timeout group alongside the other Stripe-
/// and provider-callback endpoints.
pub fn webhook_router() -> Router<AppState> {
    Router::new()
        .route("/api/billing/webhook", post(stripe_webhook))
}

/// POST /api/billing/webhook
///
/// Stripe sends `customer.subscription.{created,updated,deleted}` and
/// `payment_intent.succeeded` here. The service verifies the HMAC and
/// upserts / cancels rows in `business_subscriptions`. No JWT — auth
/// is the HMAC over the raw body using `STRIPE_WEBHOOK_SECRET`.
#[utoipa::path(
    post, path = "/api/billing/webhook", tag = "billing",
    request_body(
        content = String,
        content_type = "application/json",
        description = "Raw Stripe webhook payload (signature verified server-side via STRIPE_WEBHOOK_SECRET)",
    ),
    responses(
        (status = 200, description = "Webhook accepted"),
        (status = 400, description = "Missing Stripe-Signature header or invalid payload"),
        (status = 401, description = "Stripe signature invalid"),
    ),
)]
pub async fn stripe_webhook(
    State(state): State<AppState>,
    headers:      HeaderMap,
    body:         axum::body::Bytes,
) -> AppResult<StatusCode> {
    let sig = headers
        .get("stripe-signature")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::bad_request("missing Stripe-Signature header"))?;

    let secret = state.cfg.stripe_webhook_secret.expose_secret();
    service::handle_stripe_webhook(&state.db, &body, sig, secret).await?;
    Ok(StatusCode::OK)
}
