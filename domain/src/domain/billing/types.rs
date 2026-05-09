#![allow(missing_docs)]
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── Database rows ─────────────────────────────────────────────────────────────

/// One row of `business_subscriptions` (`007_billing.sql`). Migrated here
/// from `platform_configuration::types` once the dedicated billing domain
/// landed (Grade A item 3). The admin read endpoint keeps its current
/// path in `platform_configuration::routes` but pulls the row type from
/// this module — having two definitions would let the admin response
/// schema drift away from the webhook write schema.
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize, utoipa::ToSchema)]
pub struct BusinessSubscriptionRow {
    pub id:                     i32,
    pub business_id:            i32,
    pub tier:                   String,
    pub stripe_customer_id:     Option<String>,
    pub stripe_subscription_id: Option<String>,
    pub current_period_start:   Option<DateTime<Utc>>,
    pub current_period_end:     Option<DateTime<Utc>>,
    pub cancelled_at:           Option<DateTime<Utc>>,
    pub created_at:             DateTime<Utc>,
    pub updated_at:             DateTime<Utc>,
}

/// All columns of `business_subscriptions` for SELECT / RETURNING queries.
pub const BUSINESS_SUBSCRIPTION_COLS: &str =
    "id, business_id, tier, stripe_customer_id, stripe_subscription_id, \
     current_period_start, current_period_end, cancelled_at, \
     created_at, updated_at";

// ── Webhook payload ───────────────────────────────────────────────────────────

/// Top-level shape Stripe POSTs. We keep `data` as an opaque
/// `serde_json::Value` and extract the fields we care about per
/// event type — Stripe's typed model is huge and we only use a
/// handful of fields per event.
#[derive(Debug, Deserialize)]
pub struct StripeWebhookEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub data:       serde_json::Value,
}
