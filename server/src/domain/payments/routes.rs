/// Stripe webhook handler.
///
/// All inbound webhook events are verified against the STRIPE_WEBHOOK_SECRET
/// before any database work begins. Each event type is dispatched to a handler
/// that fails independently — one failing order must not block an unrelated
/// membership payment in the same batch.
///
/// Anchoring principle: every handler resolves business data from a DB row
/// keyed by stripe_payment_intent_id, not from Stripe metadata. Metadata is
/// only used at PI creation time to route the event type in handle_pi_succeeded.
use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode},
    routing::post,
    Router,
};

use secrecy::ExposeSecret;

use crate::{app::AppState, audit, integrations::resend, types::OrderId};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/stripe/webhook", post(webhook))
        .layer(DefaultBodyLimit::max(65_536))
}

async fn webhook(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> StatusCode {
    // Signature verification must happen before any body parsing.
    let sig = match headers
        .get("stripe-signature")
        .and_then(|v| v.to_str().ok())
    {
        Some(s) => s,
        None => return StatusCode::BAD_REQUEST,
    };

    let event: serde_json::Value = match state.stripe().verify_webhook(
        &body,
        sig,
        state.cfg.stripe_webhook_secret.expose_secret(),
    ) {
        Ok(e) => e,
        Err(_) => return StatusCode::UNAUTHORIZED,
    };

    let event_type = event["type"].as_str().unwrap_or("");

    match event_type {
        "payment_intent.succeeded" => {
            handle_pi_succeeded(&state, &event).await;
        }
        _ => {} // acknowledged but not handled
    }

    StatusCode::OK
}

// ── payment_intent.succeeded ──────────────────────────────────────────────────

async fn handle_pi_succeeded(state: &AppState, event: &serde_json::Value) {
    let pi = &event["data"]["object"];
    let pi_id = pi["id"].as_str().unwrap_or_default();

    // Route by payment type stored in metadata.
    // Metadata is only used for routing — all business data is resolved from DB.
    let payment_type = pi["metadata"]["type"].as_str().unwrap_or("");

    match payment_type {
        "order" | "" => complete_order(state, pi_id).await,
        "rsvp" => complete_rsvp(state, pi_id).await,
        "membership" => complete_membership(state, pi_id).await,
        _ => {
            tracing::info!(
                pi_id,
                payment_type,
                "unhandled payment_intent.succeeded type"
            );
        }
    }
}

async fn complete_order(state: &AppState, pi_id: &str) {
    #[derive(sqlx::FromRow)]
    struct OrderInfo {
        id: OrderId,
        variety_name: String,
        total_cents: i64,
        email: Option<String>,
    }

    let result: Result<Option<OrderInfo>, _> = sqlx::query_as(
        "UPDATE orders SET status = 'paid'
         WHERE stripe_payment_intent_id = $1 AND status = 'queued'
         RETURNING
             orders.id,
             (SELECT name FROM catalog_varieties WHERE id = orders.variety_id) AS variety_name,
             orders.total_cents,
             orders.customer_email AS email",
    )
    .bind(pi_id)
    .fetch_optional(&state.db)
    .await;

    match result {
        Ok(Some(info)) => {
            tracing::info!(pi_id, order_id = %info.id, "order marked paid via webhook");
            audit::write(
                &state.db,
                None,
                None,
                "payment.order_paid",
                serde_json::json!({
                    "pi_id":        pi_id,
                    "order_id":     i32::from(info.id),
                    "amount_cents": info.total_cents,
                    "outcome":      "paid",
                }),
                None,
            )
            .await;
            if let (Some(email), Some(key)) = (
                info.email,
                state
                    .cfg
                    .resend_api_key
                    .as_ref()
                    .map(|k| k.expose_secret().to_owned()),
            ) {
                let http = state.http.clone();
                let db = state.db.clone();
                let variety = info.variety_name.clone();
                let total = info.total_cents as i32;
                let oid = info.id;
                let pi = pi_id.to_owned();
                tokio::spawn(async move {
                    if let Err(e) = resend::send_order_confirmation(&http, &key, &email, oid, &variety, total).await {
                        tracing::error!(order_id = %oid, pi_id = pi, error = %e, "order confirmation email delivery failed");
                        audit::write(
                            &db, None, None,
                            "email.order_confirmation_failed",
                            serde_json::json!({ "order_id": i32::from(oid), "pi_id": pi, "error": e.to_string() }),
                            None,
                        ).await;
                    }
                });
            }
        }
        Ok(None) => tracing::warn!(pi_id, "no order found for webhook pi"),
        Err(e) => tracing::error!(pi_id, error = %e, "complete_order failed"),
    }
}

async fn complete_rsvp(state: &AppState, pi_id: &str) {
    #[derive(sqlx::FromRow)]
    struct RsvpInfo {
        user_id: i32,
        event_name: String,
        email: Option<String>,
    }

    let result: Result<Option<RsvpInfo>, _> = sqlx::query_as(
        "UPDATE popup_rsvps SET status = 'confirmed'
         WHERE stripe_payment_intent_id = $1 AND status = 'pending'
         RETURNING
             popup_rsvps.user_id,
             (SELECT name FROM businesses WHERE id = popup_rsvps.business_id) AS event_name,
             (SELECT email FROM users WHERE id = popup_rsvps.user_id) AS email",
    )
    .bind(pi_id)
    .fetch_optional(&state.db)
    .await;

    match result {
        Ok(Some(info)) => {
            tracing::info!(pi_id, "RSVP confirmed via webhook");
            audit::write(
                &state.db,
                Some(info.user_id),
                None,
                "payment.rsvp_confirmed",
                serde_json::json!({ "pi_id": pi_id, "outcome": "confirmed" }),
                None,
            )
            .await;
            if let (Some(email), Some(key)) = (
                info.email,
                state
                    .cfg
                    .resend_api_key
                    .as_ref()
                    .map(|k| k.expose_secret().to_owned()),
            ) {
                let http = state.http.clone();
                let db = state.db.clone();
                let event = info.event_name.clone();
                let uid = info.user_id;
                let pi = pi_id.to_owned();
                tokio::spawn(async move {
                    if let Err(e) = resend::send_rsvp_confirmed(&http, &key, &email, &event).await {
                        tracing::error!(user_id = uid, pi_id = pi, error = %e, "RSVP confirmation email delivery failed");
                        audit::write(
                            &db, Some(uid), None,
                            "email.rsvp_confirmation_failed",
                            serde_json::json!({ "pi_id": pi, "event": event, "error": e.to_string() }),
                            None,
                        ).await;
                    }
                });
            }
        }
        Ok(None) => tracing::warn!(pi_id, "no RSVP found for webhook pi"),
        Err(e) => tracing::error!(pi_id, error = %e, "complete_rsvp failed"),
    }
}

async fn complete_membership(state: &AppState, pi_id: &str) {
    #[derive(sqlx::FromRow)]
    struct PendingMembership {
        user_id: i32,
        tier: String,
        amount_cents: i32,
    }

    let renews_at = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::days(365))
        .unwrap()
        .naive_utc();

    let result: Result<Option<PendingMembership>, _> = sqlx::query_as(
        "UPDATE memberships
         SET status = 'active', started_at = NOW(), renews_at = $2
         WHERE stripe_payment_intent_id = $1 AND status = 'pending'
         RETURNING user_id, tier, amount_cents",
    )
    .bind(pi_id)
    .bind(renews_at)
    .fetch_optional(&state.db)
    .await;

    match result {
        Ok(Some(m)) => {
            tracing::info!(pi_id, uid = m.user_id, tier = %m.tier, "membership activated");
            audit::write(
                &state.db,
                Some(m.user_id),
                None,
                "payment.membership_activated",
                serde_json::json!({
                    "pi_id":        pi_id,
                    "tier":         m.tier,
                    "amount_cents": m.amount_cents,
                    "outcome":      "activated",
                }),
                None,
            )
            .await;
        }
        Ok(None) => {
            tracing::warn!(pi_id, "no pending membership found for webhook pi");
            audit::write(
                &state.db, None, None,
                "payment.membership_not_found",
                serde_json::json!({ "pi_id": pi_id, "outcome": "no_pending_row" }),
                None,
            ).await;
        }
        Err(e) => tracing::error!(pi_id, error = %e, "complete_membership failed"),
    }
}


