/// Audit event writer — append-only log of security-relevant platform actions.
///
/// Every call inserts a row into `audit_events`. The table has a DB trigger
/// that rejects UPDATE and DELETE, so the audit trail cannot be scrubbed after
/// the fact. Callers do not check the return value: audit failures are logged
/// but never surface as user-facing errors — a failed audit write must not
/// block a legitimate payment or stamp.
///
/// # Event kinds (non-exhaustive)
///
///   auth.staff_login              — staff member authenticated
///   square_oauth.connected        — business connected Square account
///   square_oauth.token_refreshed  — OAuth token refreshed automatically
///   loyalty.balance_read          — user fetched their loyalty balance
///   loyalty.qr_token_issued       — QR stamp token generated for user
///   loyalty.steep_earned          — stamp recorded against a business
///   loyalty.reward_redeemed       — reward redeemed
///   venue_order.payment_intent_created  — Stripe PaymentIntent created
///   venue_order.payment_confirmed       — payment_intent.succeeded received
///   venue_order.square_push_succeeded   — order pushed to Square POS
///   venue_order.square_push_failed      — Square push failed after payment
use serde_json::Value;
use sqlx::PgPool;

/// Write an audit event. Errors are logged and swallowed — audit failures
/// never propagate to the caller.
pub async fn write(
    pool:        &PgPool,
    actor_id:    Option<i32>,
    business_id: Option<i32>,
    event_kind:  &str,
    metadata:    Value,
    ip_address:  Option<std::net::IpAddr>,
) {
    let ip_str = ip_address.map(|ip| ip.to_string());

    let result = sqlx::query(
        "INSERT INTO audit_events (actor_id, business_id, event_kind, metadata, ip_address)
         VALUES ($1, $2, $3, $4, $5::inet)"
    )
    .bind(actor_id)
    .bind(business_id)
    .bind(event_kind)
    .bind(&metadata)
    .bind(ip_str.as_deref())
    .execute(pool)
    .await;

    if let Err(e) = result {
        // Audit failure is an operational alert, not a user error.
        // The failed event kind is logged so on-call can reconstruct what was missed.
        tracing::error!(
            event_kind = event_kind,
            error      = %e,
            "audit write failed — event not recorded"
        );
    }
}
