#![allow(missing_docs)]
use chrono::{DateTime, Utc};
use sqlx::PgConnection;

use crate::error::{AppResult, DomainError};
use super::types::{BusinessSubscriptionRow, BUSINESS_SUBSCRIPTION_COLS};

/// Insert a new subscription row, or update the existing one when the
/// `(business_id)` UNIQUE constraint hits. Stripe sends `subscription.created`
/// once and `subscription.updated` thereafter; both funnel through here
/// so the row reflects the latest billing state regardless of which
/// event arrived first.
pub async fn upsert_subscription(
    conn:                   &mut PgConnection,
    business_id:            i32,
    tier:                   &str,
    stripe_customer_id:     Option<&str>,
    stripe_subscription_id: Option<&str>,
    current_period_start:   Option<DateTime<Utc>>,
    current_period_end:     Option<DateTime<Utc>>,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO business_subscriptions \
         (business_id, tier, stripe_customer_id, stripe_subscription_id, \
          current_period_start, current_period_end, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, now()) \
         ON CONFLICT (business_id) DO UPDATE SET \
             tier                   = EXCLUDED.tier, \
             stripe_customer_id     = EXCLUDED.stripe_customer_id, \
             stripe_subscription_id = EXCLUDED.stripe_subscription_id, \
             current_period_start   = EXCLUDED.current_period_start, \
             current_period_end     = EXCLUDED.current_period_end, \
             updated_at             = now()"
    )
    .bind(business_id)
    .bind(tier)
    .bind(stripe_customer_id)
    .bind(stripe_subscription_id)
    .bind(current_period_start)
    .bind(current_period_end)
    .execute(conn)
    .await
    .map_err(DomainError::Db)?;
    Ok(())
}

/// Mark a subscription cancelled by setting `cancelled_at = now()`.
/// Identified by `stripe_subscription_id` because that's the only
/// stable handle the webhook payload carries — `business_id` is on
/// the row but the `customer.subscription.deleted` event identifies
/// the subscription, not the business.
pub async fn cancel_subscription(
    conn:                   &mut PgConnection,
    stripe_subscription_id: &str,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE business_subscriptions \
         SET cancelled_at = now(), updated_at = now() \
         WHERE stripe_subscription_id = $1"
    )
    .bind(stripe_subscription_id)
    .execute(conn)
    .await
    .map_err(DomainError::Db)?;
    Ok(())
}

pub async fn get_subscription_by_business(
    conn:        &mut PgConnection,
    business_id: i32,
) -> AppResult<Option<BusinessSubscriptionRow>> {
    sqlx::query_as(&format!(
        "SELECT {BUSINESS_SUBSCRIPTION_COLS} FROM business_subscriptions \
         WHERE business_id = $1"
    ))
    .bind(business_id)
    .fetch_optional(conn)
    .await
    .map_err(DomainError::Db)
}

/// List every subscription row for the admin dashboard. Migrated from
/// `platform_configuration::repository::list_business_subscriptions` —
/// the caller in `platform_configuration::routes::list_subscriptions`
/// was updated to point here so the admin endpoint shape stays stable.
pub async fn list_all(conn: &mut PgConnection) -> AppResult<Vec<BusinessSubscriptionRow>> {
    sqlx::query_as(&format!(
        "SELECT {BUSINESS_SUBSCRIPTION_COLS} FROM business_subscriptions \
         ORDER BY id"
    ))
    .fetch_all(conn)
    .await
    .map_err(DomainError::Db)
}
