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

use secrecy::ExposeSecret;

use crate::{
    app::AppState,
    integrations::resend,
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
        .verify_webhook(&body, sig, state.cfg.stripe_webhook_secret.expose_secret())
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
        "rsvp"            => complete_rsvp(state, pi_id).await,
        "membership"      => complete_membership(state, pi, pi_id).await,
        "tip"             => complete_tip(state, pi).await,
        "portal_access"   => complete_portal_access(state, pi).await,
        "tournament_entry"=> complete_tournament_entry(state, pi).await,
        "portrait_purchase"=> complete_portrait_purchase(state, pi).await,
        _            => {
            tracing::info!(pi_id, payment_type, "unhandled payment_intent.succeeded type");
        }
    }
}

async fn complete_order(state: &AppState, pi_id: &str) {
    #[derive(sqlx::FromRow)]
    struct OrderInfo {
        id:           i32,
        variety_name: String,
        total_cents:  i64,
        email:        Option<String>,
    }

    let result: Result<Option<OrderInfo>, _> = sqlx::query_as(
        "UPDATE orders SET status = 'paid'
         WHERE stripe_payment_intent_id = $1 AND status = 'queued'
         RETURNING
             orders.id,
             (SELECT name FROM catalog_varieties WHERE id = orders.variety_id) AS variety_name,
             orders.total_cents,
             (SELECT email FROM users WHERE id = orders.user_id) AS email",
    )
    .bind(pi_id)
    .fetch_optional(&state.db)
    .await;

    match result {
        Ok(Some(info)) => {
            tracing::info!(pi_id, order_id = info.id, "order marked paid via webhook");
            if let (Some(email), Some(key)) = (info.email, state.cfg.resend_api_key.as_ref().map(|k| k.expose_secret().to_owned())) {
                let http    = state.http.clone();
                let variety = info.variety_name.clone();
                let total   = info.total_cents as i32;
                let oid     = info.id;
                tokio::spawn(async move {
                    let _ = resend::send_order_confirmation(&http, &key, &email, oid, &variety, total).await;
                });
            }
        }
        Ok(None)  => tracing::warn!(pi_id, "no order found for webhook pi"),
        Err(e)    => tracing::error!(pi_id, error = %e, "complete_order failed"),
    }
}

async fn complete_rsvp(state: &AppState, pi_id: &str) {
    #[derive(sqlx::FromRow)]
    struct RsvpInfo {
        event_name: String,
        email:      Option<String>,
    }

    let result: Result<Option<RsvpInfo>, _> = sqlx::query_as(
        "UPDATE popup_rsvps SET status = 'confirmed'
         WHERE stripe_payment_intent_id = $1 AND status = 'pending'
         RETURNING
             (SELECT name FROM popup_events WHERE id = popup_rsvps.event_id) AS event_name,
             (SELECT email FROM users WHERE id = popup_rsvps.user_id) AS email",
    )
    .bind(pi_id)
    .fetch_optional(&state.db)
    .await;

    match result {
        Ok(Some(info)) => {
            tracing::info!(pi_id, "RSVP confirmed via webhook");
            if let (Some(email), Some(key)) = (info.email, state.cfg.resend_api_key.as_ref().map(|k| k.expose_secret().to_owned())) {
                let http  = state.http.clone();
                let event = info.event_name.clone();
                tokio::spawn(async move {
                    let _ = resend::send_rsvp_confirmed(&http, &key, &email, &event).await;
                });
            }
        }
        Ok(None)  => tracing::warn!(pi_id, "no RSVP found for webhook pi"),
        Err(e)    => tracing::error!(pi_id, error = %e, "complete_rsvp failed"),
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

            if let Some(key) = state.cfg.resend_api_key.as_ref().map(|k| k.expose_secret().to_owned()) {
                let http = state.http.clone();
                let db   = state.db.clone();
                tokio::spawn(async move {
                    let email: Option<String> = sqlx::query_scalar(
                        "SELECT email FROM users WHERE id = $1",
                    )
                    .bind(worker_id)
                    .fetch_optional(&db)
                    .await
                    .unwrap_or(None)
                    .flatten();

                    if let Some(addr) = email {
                        let _ = resend::send_tip_received(&http, &key, &addr, amount).await;
                    }
                });
            }
        }
    }
}

// ── portal_access ─────────────────────────────────────────────────────────────

async fn complete_portal_access(state: &AppState, pi: &serde_json::Value) {
    let buyer_id: Option<i32> = pi["metadata"]["buyer_id"]
        .as_str()
        .and_then(|s| s.parse().ok());
    let owner_id: Option<i32> = pi["metadata"]["owner_id"]
        .as_str()
        .and_then(|s| s.parse().ok());

    let (Some(buyer), Some(owner)) = (buyer_id, owner_id) else {
        tracing::warn!("portal_access webhook missing buyer_id or owner_id");
        return;
    };

    let result = sqlx::query(
        "UPDATE portal_access SET status = 'active'
         WHERE buyer_id = $1 AND owner_id = $2 AND status = 'pending'",
    )
    .bind(buyer)
    .bind(owner)
    .execute(&state.db)
    .await;

    match result {
        Ok(r) if r.rows_affected() > 0 => {
            tracing::info!(buyer, owner, "portal access activated via webhook");
        }
        Ok(_)  => tracing::warn!(buyer, owner, "no pending portal_access found"),
        Err(e) => tracing::error!(error = %e, "complete_portal_access failed"),
    }
}

// ── tournament_entry ──────────────────────────────────────────────────────────

async fn complete_tournament_entry(state: &AppState, pi: &serde_json::Value) {
    let user_id: Option<i32> = pi["metadata"]["user_id"]
        .as_str()
        .and_then(|s| s.parse().ok());
    let tournament_id: Option<i32> = pi["metadata"]["tournament_id"]
        .as_str()
        .and_then(|s| s.parse().ok());

    let (Some(uid), Some(tid)) = (user_id, tournament_id) else {
        tracing::warn!("tournament_entry webhook missing user_id or tournament_id");
        return;
    };

    let result = sqlx::query(
        "UPDATE tournament_entries SET status = 'registered'
         WHERE tournament_id = $1 AND user_id = $2 AND status = 'pending'",
    )
    .bind(tid)
    .bind(uid)
    .execute(&state.db)
    .await;

    match result {
        Ok(r) if r.rows_affected() > 0 => {
            tracing::info!(uid, tid, "tournament entry confirmed via webhook");
        }
        Ok(_)  => tracing::warn!(uid, tid, "no pending tournament entry found"),
        Err(e) => tracing::error!(error = %e, "complete_tournament_entry failed"),
    }
}

// ── portrait_purchase ─────────────────────────────────────────────────────────

async fn complete_portrait_purchase(state: &AppState, pi: &serde_json::Value) {
    let buyer_id: Option<i32> = pi["metadata"]["buyer_id"]
        .as_str()
        .and_then(|s| s.parse().ok());
    let seller_id: Option<i32> = pi["metadata"]["seller_id"]
        .as_str()
        .and_then(|s| s.parse().ok());
    let creator_id: Option<i32> = pi["metadata"]["creator_id"]
        .as_str()
        .and_then(|s| s.parse().ok());
    let token_id: Option<i32> = pi["metadata"]["token_id"]
        .as_str()
        .and_then(|s| s.parse().ok());
    let amount = pi["amount"].as_i64().unwrap_or(0);

    let (Some(buyer), Some(seller), Some(creator), Some(token)) =
        (buyer_id, seller_id, creator_id, token_id)
    else {
        tracing::warn!("portrait_purchase webhook missing required metadata");
        return;
    };

    // Transfer ownership.
    let transfer = sqlx::query(
        "UPDATE portrait_tokens SET owner_id = $1
         WHERE id = $2 AND owner_id = $3",
    )
    .bind(buyer)
    .bind(token)
    .bind(seller)
    .execute(&state.db)
    .await;

    match transfer {
        Ok(r) if r.rows_affected() == 0 => {
            // Token already moved — idempotent, nothing to do.
            tracing::warn!(buyer, token, "portrait token already transferred or seller mismatch");
            return;
        }
        Err(e) => {
            tracing::error!(error = %e, "portrait_purchase ownership transfer failed");
            return;
        }
        Ok(_) => {}
    }

    tracing::info!(buyer, token, "portrait token transferred via webhook");

    // Royalty: 15% to creator, 85% to seller (recorded; actual payout is manual / batched).
    let creator_cut = (amount * 15) / 100;
    let seller_cut  = amount - creator_cut;

    let _ = sqlx::query(
        "INSERT INTO earnings_ledger (user_id, amount_cents, type)
         VALUES ($1, $2, 'portrait_royalty'), ($3, $4, 'portrait_sale')",
    )
    .bind(creator)
    .bind(creator_cut)
    .bind(seller)
    .bind(seller_cut)
    .execute(&state.db)
    .await;
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
