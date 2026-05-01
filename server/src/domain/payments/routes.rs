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
    // Metadata is only used for routing — all business data is resolved from DB.
    let payment_type = pi["metadata"]["type"].as_str().unwrap_or("");

    match payment_type {
        "order" | "" => complete_order(state, pi_id).await,
        "rsvp" => complete_rsvp(state, pi_id).await,
        "membership" => complete_membership(state, pi_id).await,
        "tip" => complete_tip(state, pi_id).await,
        "portal_access" => complete_portal_access(state, pi_id).await,
        // Venue drink orders: push to Square POS + credit loyalty steep.
        "venue_order" => {
            crate::domain::venue_drinks::service::complete_venue_order(state, pi_id).await
        }
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

async fn complete_tip(state: &AppState, pi_id: &str) {
    #[derive(sqlx::FromRow)]
    struct TipPayment {
        id: i32,
        business_id: i32,
        amount_cents: i64,
    }

    // Resolve the tip from the DB — never from metadata.
    let tip_result: Result<Option<TipPayment>, _> = sqlx::query_as(
        "UPDATE tip_payments SET status = 'processing'
         WHERE stripe_payment_intent_id = $1 AND status = 'pending'
         RETURNING id, business_id, amount_cents",
    )
    .bind(pi_id)
    .fetch_optional(&state.db)
    .await;

    let tip = match tip_result {
        Ok(Some(t)) => t,
        Ok(None) => {
            tracing::warn!(pi_id, "no pending tip_payment found for webhook pi");
            return;
        }
        Err(e) => {
            tracing::error!(pi_id, error = %e, "complete_tip: tip_payments lookup failed");
            return;
        }
    };

    // Find the currently placed worker at this business.
    let placed: Option<(i32,)> = sqlx::query_as(
        "SELECT u.id FROM employment_contracts ec
         JOIN users u ON u.id = ec.user_id
         WHERE ec.business_id = $1 AND ec.status = 'active'
         ORDER BY ec.created_at DESC LIMIT 1",
    )
    .bind(tip.business_id)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

    let Some((worker_id,)) = placed else {
        tracing::warn!(
            pi_id,
            business_id = tip.business_id,
            "no active placed user to receive tip"
        );
        // Mark complete so we don't retry; audit the undelivered state.
        let _ = sqlx::query(
            "UPDATE tip_payments SET status = 'undeliverable'
             WHERE stripe_payment_intent_id = $1",
        )
        .bind(pi_id)
        .execute(&state.db)
        .await;
        audit::write(
            &state.db,
            None,
            Some(tip.business_id),
            "payment.tip_undeliverable",
            serde_json::json!({
                "pi_id":        pi_id,
                "amount_cents": tip.amount_cents,
                "outcome":      "no_placed_user",
            }),
            None,
        )
        .await;
        return;
    };

    if let Err(e) = sqlx::query(
        "INSERT INTO earnings_ledger (user_id, amount_cents, type, stripe_payment_intent_id)
         VALUES ($1, $2, 'tip', $3)",
    )
    .bind(worker_id)
    .bind(tip.amount_cents)
    .bind(pi_id)
    .execute(&state.db)
    .await
    {
        tracing::error!(pi_id, worker_id, error = %e, "earnings_ledger INSERT failed for tip — earning record lost");
    }

    if let Err(e) = sqlx::query(
        "UPDATE tip_payments SET status = 'completed'
         WHERE stripe_payment_intent_id = $1",
    )
    .bind(pi_id)
    .execute(&state.db)
    .await
    {
        tracing::error!(pi_id, error = %e, "tip_payments status UPDATE failed — tip will appear pending");
    }

    tracing::info!(
        pi_id,
        worker_id,
        amount = tip.amount_cents,
        "tip credited via webhook"
    );
    audit::write(
        &state.db,
        Some(worker_id),
        Some(tip.business_id),
        "payment.tip_credited",
        serde_json::json!({
            "pi_id":        pi_id,
            "amount_cents": tip.amount_cents,
            "outcome":      "credited",
        }),
        None,
    )
    .await;

    if let Some(key) = state
        .cfg
        .resend_api_key
        .as_ref()
        .map(|k| k.expose_secret().to_owned())
    {
        let http = state.http.clone();
        let db = state.db.clone();
        let amt = tip.amount_cents as i32;
        let pi = pi_id.to_owned();
        tokio::spawn(async move {
            match sqlx::query_scalar::<_, Option<String>>("SELECT email FROM users WHERE id = $1")
                .bind(worker_id)
                .fetch_optional(&db)
                .await
            {
                Ok(Some(Some(addr))) => {
                    if let Err(e) = resend::send_tip_received(&http, &key, &addr, amt).await {
                        tracing::error!(worker_id, pi_id = pi, error = %e, "tip received email delivery failed");
                        audit::write(
                            &db, Some(worker_id), None,
                            "email.tip_notification_failed",
                            serde_json::json!({ "pi_id": pi, "amount_cents": amt, "error": e.to_string() }),
                            None,
                        ).await;
                    }
                }
                Ok(_) => {}
                Err(e) => tracing::error!(worker_id, error = %e, "worker email lookup failed for tip notification"),
            }
        });
    }
}

// ── portal_access ─────────────────────────────────────────────────────────────

async fn complete_portal_access(state: &AppState, pi_id: &str) {
    #[derive(sqlx::FromRow)]
    struct PortalRow {
        buyer_id: i32,
        owner_id: i32,
        amount_cents: i64,
    }

    let result: Result<Option<PortalRow>, _> = sqlx::query_as(
        "UPDATE portal_access SET status = 'active'
         WHERE stripe_payment_intent_id = $1 AND status = 'pending'
         RETURNING buyer_id, owner_id, amount_cents",
    )
    .bind(pi_id)
    .fetch_optional(&state.db)
    .await;

    match result {
        Ok(Some(row)) => {
            tracing::info!(
                pi_id,
                buyer = row.buyer_id,
                owner = row.owner_id,
                "portal access activated via webhook"
            );
            audit::write(
                &state.db,
                Some(row.buyer_id),
                None,
                "payment.portal_access_activated",
                serde_json::json!({
                    "pi_id":        pi_id,
                    "buyer_id":     row.buyer_id,
                    "owner_id":     row.owner_id,
                    "amount_cents": row.amount_cents,
                    "outcome":      "activated",
                }),
                None,
            )
            .await;
        }
        Ok(None) => tracing::warn!(pi_id, "no pending portal_access found for webhook pi"),
        Err(e) => tracing::error!(pi_id, error = %e, "complete_portal_access failed"),
    }
}

// ── identity.verification_session.verified ────────────────────────────────────

async fn handle_identity_verified(state: &AppState, event: &serde_json::Value) {
    let session = &event["data"]["object"];
    let session_id = session["id"].as_str().unwrap_or_default();

    let verified_name = session["verified_outputs"]["name"].as_str();
    let verified_dob = session["verified_outputs"]["dob"]
        .as_str()
        .or_else(|| session["verified_outputs"]["date_of_birth"].as_str());

    // Resolve user_id from the stored session record — not from metadata.
    // Sessions are stored in identity_verification_sessions at creation time
    // by the verification initiation endpoint.
    let user_id: Option<i32> = sqlx::query_scalar(
        "UPDATE identity_verification_sessions
         SET status = 'verified'
         WHERE stripe_session_id = $1 AND status = 'pending'
         RETURNING user_id",
    )
    .bind(session_id)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

    let Some(uid) = user_id else {
        tracing::warn!(
            session_id,
            "no pending identity_verification_session found for webhook"
        );
        return;
    };

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
        Ok(_) => {
            tracing::info!(uid, session_id, "identity verified via webhook");
            audit::write(
                &state.db,
                Some(uid),
                None,
                "identity.verified",
                serde_json::json!({
                    "session_id": session_id,
                    "outcome":    "verified",
                }),
                None,
            )
            .await;
        }
        Err(e) => tracing::error!(uid, session_id, error = %e, "handle_identity_verified failed"),
    }
}
