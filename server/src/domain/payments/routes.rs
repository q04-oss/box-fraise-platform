/// Stripe webhook handler.
///
/// All inbound webhook events are verified against the STRIPE_WEBHOOK_SECRET
/// before any database work begins. Each event type is dispatched to a handler
/// that fails independently — one failing order must not block an unrelated
/// membership payment in the same batch.
use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::post,
    Router,
};

use crate::{
    app::AppState,
    error::AppError,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/api/stripe/webhook", post(webhook))
}

async fn webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    // Signature verification must happen before any body parsing.
    let sig = match headers.get("stripe-signature").and_then(|v| v.to_str().ok()) {
        Some(s) => s,
        None    => return StatusCode::BAD_REQUEST,
    };

    let event: serde_json::Value = match state
        .stripe()
        .verify_webhook(&body, sig, &state.cfg.stripe_webhook_secret)
    {
        Ok(e)  => e,
        Err(_) => return StatusCode::UNAUTHORIZED,
    };

    let event_type = event["type"].as_str().unwrap_or("");

    match event_type {
        "payment_intent.succeeded" => {
            handle_pi_succeeded(&state, &event).await;
        }
        "identity.verification_session.verified" => {
            handle_identity_verified(&state, &event).await;
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
    let payment_type = pi["metadata"]["type"].as_str().unwrap_or("");

    match payment_type {
        "order" | "" => complete_order(state, pi_id).await,
        "rsvp"       => complete_rsvp(state, pi_id).await,
        "membership"  => complete_membership(state, pi, pi_id).await,
        "tip"        => complete_tip(state, pi).await,
        _            => {
            tracing::info!(pi_id, payment_type, "unhandled payment_intent.succeeded type");
        }
    }
}

async fn complete_order(state: &AppState, pi_id: &str) {
    let result = sqlx::query(
        "UPDATE orders SET status = 'paid'
         WHERE stripe_payment_intent_id = $1 AND status = 'queued'",
    )
    .bind(pi_id)
    .execute(&state.db)
    .await;

    match result {
        Ok(r) if r.rows_affected() > 0 => {
            tracing::info!(pi_id, "order marked paid via webhook");
            // TODO: push notification + send_order_queued email
        }
        Ok(_)  => tracing::warn!(pi_id, "no order found for webhook pi"),
        Err(e) => tracing::error!(pi_id, error = %e, "complete_order failed"),
    }
}

async fn complete_rsvp(state: &AppState, pi_id: &str) {
    let result = sqlx::query(
        "UPDATE popup_rsvps SET status = 'confirmed'
         WHERE stripe_payment_intent_id = $1 AND status = 'pending'",
    )
    .bind(pi_id)
    .execute(&state.db)
    .await;

    match result {
        Ok(r) if r.rows_affected() > 0 => {
            tracing::info!(pi_id, "RSVP confirmed via webhook");
            // TODO: send_rsvp_confirmed email
        }
        Ok(_)  => tracing::warn!(pi_id, "no RSVP found for webhook pi"),
        Err(e) => tracing::error!(pi_id, error = %e, "complete_rsvp failed"),
    }
}

async fn complete_membership(state: &AppState, pi: &serde_json::Value, pi_id: &str) {
    let tier    = pi["metadata"]["tier"].as_str().unwrap_or("maison");
    let user_id = pi["metadata"]["user_id"]
        .as_str()
        .and_then(|s| s.parse::<i32>().ok());

    if let Some(uid) = user_id {
        let renews_at = chrono::Utc::now()
            .checked_add_signed(chrono::Duration::days(365))
            .unwrap()
            .naive_utc();

        let result = sqlx::query(
            "INSERT INTO memberships (user_id, tier, status, started_at, renews_at)
             VALUES ($1, $2, 'active', NOW(), $3)
             ON CONFLICT (user_id) DO UPDATE
             SET tier = EXCLUDED.tier, status = 'active',
                 started_at = NOW(), renews_at = EXCLUDED.renews_at,
                 renewal_notified = false",
        )
        .bind(uid)
        .bind(tier)
        .bind(renews_at)
        .execute(&state.db)
        .await;

        match result {
            Ok(_)  => tracing::info!(pi_id, uid, tier, "membership activated"),
            Err(e) => tracing::error!(pi_id, error = %e, "complete_membership failed"),
        }
    }
}

async fn complete_tip(state: &AppState, pi: &serde_json::Value) {
    let business_id = pi["metadata"]["business_id"]
        .as_str()
        .and_then(|s| s.parse::<i32>().ok());

    if let Some(biz_id) = business_id {
        // Look up placed user to deliver the earnings.
        let placed: Option<(i32,)> = sqlx::query_as(
            "SELECT u.id FROM employment_contracts ec
             JOIN users u ON u.id = ec.user_id
             WHERE ec.business_id = $1 AND ec.status = 'active'
             ORDER BY ec.created_at DESC LIMIT 1",
        )
        .bind(biz_id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);

        if let Some((worker_id,)) = placed {
            let amount = pi["amount"].as_i64().unwrap_or(0) as i32;
            let _ = sqlx::query(
                "INSERT INTO earnings_ledger (user_id, amount_cents, type)
                 VALUES ($1, $2, 'tip')",
            )
            .bind(worker_id)
            .bind(amount)
            .execute(&state.db)
            .await;

            tracing::info!(worker_id, amount, "tip credited via webhook");
            // TODO: send_tip_received email
        }
    }
}

// ── identity.verification_session.verified ────────────────────────────────────

async fn handle_identity_verified(state: &AppState, event: &serde_json::Value) {
    let session  = &event["data"]["object"];
    let user_id: Option<i32> = session["metadata"]["user_id"]
        .as_str()
        .and_then(|s| s.parse().ok());

    let verified_name = session["verified_outputs"]["name"].as_str();
    let verified_dob  = session["verified_outputs"]["dob"]
        .as_str()
        .or_else(|| session["verified_outputs"]["date_of_birth"].as_str());

    if let Some(uid) = user_id {
        let expires = chrono::Utc::now()
            .checked_add_signed(chrono::Duration::days(730)) // 2 years
            .unwrap()
            .naive_utc();

        let result = sqlx::query(
            "UPDATE users
             SET identity_verified = true,
                 identity_verified_at = NOW(),
                 identity_verified_expires_at = $2,
                 id_verified_name = $3,
                 id_verified_dob  = $4
             WHERE id = $1",
        )
        .bind(uid)
        .bind(expires)
        .bind(verified_name)
        .bind(verified_dob)
        .execute(&state.db)
        .await;

        match result {
            Ok(_)  => tracing::info!(uid, "identity verified via webhook"),
            Err(e) => tracing::error!(uid, error = %e, "handle_identity_verified failed"),
        }
    }
}
