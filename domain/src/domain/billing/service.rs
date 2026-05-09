//! Stripe billing webhook handler.
//!
//! Stripe POSTs subscription lifecycle events to `POST /api/billing/webhook`;
//! the handler here verifies the HMAC, parses the event, and routes it to
//! the appropriate write path on `business_subscriptions`. Unknown event
//! types are logged and acknowledged (200) so Stripe doesn't retry — only
//! signature failures return an error.
//!
//! Auth model mirrors `identity_credentials::service::handle_stripe_webhook`:
//! the request carries no JWT, authenticity is established by the HMAC
//! over the raw body. The handler runs on `&PgPool` (no `RlsTransaction`)
//! because there's no `app.user_id` to scope to — this is a service-account
//! caller and the writes go to a table with no user-isolation policy.
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;

use crate::{
    audit,
    error::{AppResult, DomainError},
};
use super::{repository, types::StripeWebhookEvent};

/// Constant-time HMAC compare — same XOR-fold the rest of the codebase
/// uses (see `crypto::constant_time_eq`). Inlined here to keep this
/// module's auth path self-contained.
fn hmac_eq(a: &str, b: &str) -> bool {
    let ab = a.as_bytes();
    let bb = b.as_bytes();
    if ab.len() != bb.len() { return false; }
    ab.iter().zip(bb.iter()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Verify a `Stripe-Signature` header against the raw body and the shared
/// webhook secret. Returns `Unauthorized` on any mismatch — never echoes
/// the failure reason.
fn verify_stripe_signature(
    payload:        &[u8],
    sig_header:     &str,
    webhook_secret: &str,
) -> AppResult<()> {
    let timestamp = sig_header
        .split(',')
        .find(|p| p.starts_with("t="))
        .and_then(|p| p.strip_prefix("t="))
        .ok_or_else(|| DomainError::invalid_input("missing timestamp in Stripe-Signature"))?;

    let expected_hex = sig_header
        .split(',')
        .find(|p| p.starts_with("v1="))
        .and_then(|p| p.strip_prefix("v1="))
        .ok_or_else(|| DomainError::invalid_input("missing v1 in Stripe-Signature"))?;

    let signed = format!("{}.{}", timestamp, String::from_utf8_lossy(payload));
    let key    = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, webhook_secret.as_bytes());
    let tag    = ring::hmac::sign(&key, signed.as_bytes());
    let computed_hex = hex::encode(tag.as_ref());

    if !hmac_eq(&computed_hex, expected_hex) {
        return Err(DomainError::Unauthorized);
    }
    Ok(())
}

/// Pull `business_id` out of `metadata` and parse it as i32. The iOS app
/// (or admin tool) is expected to set `metadata[business_id]` on every
/// Stripe customer / subscription / payment_intent it creates so we can
/// route events back to the row that should be updated.
fn business_id_from_metadata(obj: &Value) -> Option<i32> {
    obj.get("metadata")?
        .get("business_id")?
        .as_str()
        .and_then(|s| s.parse::<i32>().ok())
}

/// Pull a Stripe subscription's tier from
/// `items.data[0].price.metadata.tier`. Defaults to `"standard"` when
/// absent so a misconfigured Stripe price still produces a valid row
/// (the CHECK constraint on `business_subscriptions.tier` accepts
/// `standard | professional | trusted`).
fn tier_from_subscription(obj: &Value) -> String {
    obj.get("items")
        .and_then(|i| i.get("data"))
        .and_then(|d| d.get(0))
        .and_then(|item| item.get("price"))
        .and_then(|p| p.get("metadata"))
        .and_then(|m| m.get("tier"))
        .and_then(|t| t.as_str())
        .filter(|t| ["standard", "professional", "trusted"].contains(t))
        .unwrap_or("standard")
        .to_owned()
}

fn unix_to_datetime(v: &Value) -> Option<DateTime<Utc>> {
    let secs = v.as_i64()?;
    DateTime::from_timestamp(secs, 0)
}

/// Top-level dispatch — verify signature, parse, route by `event_type`.
pub async fn handle_stripe_webhook(
    pool:             &PgPool,
    payload:          &[u8],
    stripe_signature: &str,
    webhook_secret:   &str,
) -> AppResult<()> {
    verify_stripe_signature(payload, stripe_signature, webhook_secret)?;

    let event: StripeWebhookEvent = serde_json::from_slice(payload)
        .map_err(|_| DomainError::invalid_input("invalid JSON in Stripe webhook payload"))?;

    let object = event.data.get("object")
        .ok_or_else(|| DomainError::invalid_input("missing data.object in webhook payload"))?;

    let mut conn = pool.acquire().await.map_err(DomainError::Db)?;

    match event.event_type.as_str() {
        "payment_intent.succeeded" => {
            let business_id = business_id_from_metadata(object);
            let amount      = object.get("amount").and_then(|a| a.as_i64()).unwrap_or(0);
            let intent_id   = object.get("id").and_then(|i| i.as_str()).unwrap_or("");
            audit::write(
                pool,
                business_id,
                None,
                "billing.payment_succeeded",
                serde_json::json!({
                    "business_id":       business_id,
                    "amount":            amount,
                    "payment_intent_id": intent_id,
                }),
            ).await;
        }

        "customer.subscription.created" | "customer.subscription.updated" => {
            // business_id MUST be on the subscription's metadata for the
            // row to be routable. Without it we can't upsert — log and
            // acknowledge so Stripe stops retrying.
            let Some(business_id) = business_id_from_metadata(object) else {
                tracing::warn!(
                    event_type = %event.event_type,
                    "Stripe subscription event without metadata.business_id — skipping",
                );
                return Ok(());
            };

            let tier               = tier_from_subscription(object);
            let stripe_customer_id = object.get("customer").and_then(|c| c.as_str());
            let stripe_sub_id      = object.get("id").and_then(|i| i.as_str());
            let period_start       = object.get("current_period_start").and_then(unix_to_datetime);
            let period_end         = object.get("current_period_end").and_then(unix_to_datetime);

            repository::upsert_subscription(
                &mut conn,
                business_id,
                &tier,
                stripe_customer_id,
                stripe_sub_id,
                period_start,
                period_end,
            ).await?;

            audit::write(
                pool,
                Some(business_id),
                None,
                "billing.subscription_updated",
                serde_json::json!({
                    "business_id":            business_id,
                    "tier":                   tier,
                    "stripe_subscription_id": stripe_sub_id,
                    "event_type":             event.event_type,
                }),
            ).await;
        }

        "customer.subscription.deleted" => {
            let Some(stripe_sub_id) = object.get("id").and_then(|i| i.as_str()) else {
                tracing::warn!("subscription.deleted without object.id — skipping");
                return Ok(());
            };
            repository::cancel_subscription(&mut conn, stripe_sub_id).await?;
            audit::write(
                pool,
                business_id_from_metadata(object),
                None,
                "billing.subscription_cancelled",
                serde_json::json!({
                    "stripe_subscription_id": stripe_sub_id,
                }),
            ).await;
        }

        _ => {
            // Unhandled type — log and acknowledge. Stripe will mark the
            // event delivered and stop retrying, which is correct: any
            // event we don't have a handler for shouldn't keep retrying.
            tracing::debug!(event_type = %event.event_type, "Unhandled Stripe billing event");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    /// Reproduce the same `Stripe-Signature` shape Stripe sends so the
    /// HMAC verification accepts our payload. The fixed timestamp keeps
    /// the test deterministic — the verifier doesn't enforce a maximum
    /// age, only that the timestamp is present.
    fn make_webhook_signature(secret: &str, payload: &[u8]) -> String {
        let ts  = "1700000000";
        let msg = format!("{}.{}", ts, String::from_utf8_lossy(payload));
        let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, secret.as_bytes());
        let tag = ring::hmac::sign(&key, msg.as_bytes());
        format!("t={},v1={}", ts, hex::encode(tag.as_ref()))
    }

    /// Bare-minimum business + location to satisfy the FK on
    /// `business_subscriptions.business_id`. Returns the business id.
    async fn create_test_business(pool: &PgPool) -> i32 {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let (owner_id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified, verification_status) \
             VALUES ($1, true, 'attested') RETURNING id"
        )
        .bind(&SafeEmail().fake::<String>())
        .fetch_one(pool).await.unwrap();
        let (loc_id,): (i32,) = sqlx::query_as(
            "INSERT INTO locations (name, location_type, address, timezone) \
             VALUES ('Bill Loc', 'partner_business', '1 Bill', 'America/Edmonton') \
             RETURNING id"
        )
        .fetch_one(pool).await.unwrap();
        let (biz_id,): (i32,) = sqlx::query_as(
            "INSERT INTO businesses (location_id, primary_holder_id, name, verification_status) \
             VALUES ($1, $2, 'Bill Biz', 'active') RETURNING id"
        )
        .bind(loc_id).bind(owner_id)
        .fetch_one(pool).await.unwrap();
        biz_id
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn handle_stripe_webhook_subscription_created_upserts_row(pool: PgPool) {
        let biz_id = create_test_business(&pool).await;
        let secret = "whsec_billing_test";

        let payload = serde_json::to_vec(&serde_json::json!({
            "type": "customer.subscription.created",
            "data": {
                "object": {
                    "id":                   "sub_test_001",
                    "customer":             "cus_test_001",
                    "current_period_start": 1700000000_i64,
                    "current_period_end":   1702592000_i64,
                    "metadata":             { "business_id": biz_id.to_string() },
                    "items": {
                        "data": [{
                            "price": { "metadata": { "tier": "professional" } }
                        }]
                    }
                }
            }
        })).unwrap();

        let sig = make_webhook_signature(secret, &payload);
        handle_stripe_webhook(&pool, &payload, &sig, secret).await
            .expect("valid webhook must succeed");

        let mut conn = pool.acquire().await.unwrap();
        let row = repository::get_subscription_by_business(&mut conn, biz_id).await.unwrap();
        let row = row.expect("subscription row must be created");
        assert_eq!(row.tier, "professional");
        assert_eq!(row.stripe_subscription_id.as_deref(), Some("sub_test_001"));
        assert_eq!(row.stripe_customer_id.as_deref(),     Some("cus_test_001"));
        assert!(row.cancelled_at.is_none(), "new sub must not be marked cancelled");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn handle_stripe_webhook_subscription_deleted_sets_cancelled_at(pool: PgPool) {
        let biz_id = create_test_business(&pool).await;
        let secret = "whsec_billing_test";

        // First seed a subscription via repo so we have something to cancel.
        let mut conn = pool.acquire().await.unwrap();
        repository::upsert_subscription(
            &mut conn, biz_id, "professional",
            Some("cus_seed"), Some("sub_to_cancel"),
            None, None,
        ).await.unwrap();
        drop(conn);

        let payload = serde_json::to_vec(&serde_json::json!({
            "type": "customer.subscription.deleted",
            "data": { "object": { "id": "sub_to_cancel" } }
        })).unwrap();

        let sig = make_webhook_signature(secret, &payload);
        handle_stripe_webhook(&pool, &payload, &sig, secret).await
            .expect("delete webhook must succeed");

        let mut conn = pool.acquire().await.unwrap();
        let row = repository::get_subscription_by_business(&mut conn, biz_id).await.unwrap()
            .expect("row should still exist (cancellation is soft)");
        assert!(row.cancelled_at.is_some(), "cancelled_at must be set");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn handle_stripe_webhook_rejects_invalid_signature(pool: PgPool) {
        let payload = b"{}";
        let err = handle_stripe_webhook(
            &pool, payload,
            "t=1700000000,v1=0000000000000000000000000000000000000000000000000000000000000000",
            "whsec_real_secret",
        ).await.unwrap_err();
        assert!(matches!(err, DomainError::Unauthorized));
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn handle_stripe_webhook_ignores_unknown_events(pool: PgPool) {
        let secret = "whsec_billing_test";
        let payload = serde_json::to_vec(&serde_json::json!({
            "type": "invoice.paid",
            "data": { "object": { "id": "in_test_001" } }
        })).unwrap();
        let sig = make_webhook_signature(secret, &payload);
        // Should swallow the event and return Ok without touching the DB.
        handle_stripe_webhook(&pool, &payload, &sig, secret).await
            .expect("unknown event types must be acknowledged");
    }
}
