#![allow(missing_docs)]
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── Database rows ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize, utoipa::ToSchema)]
pub struct WhiskedOrderRow {
    pub id:                       i32,
    pub user_id:                  i32,
    pub business_id:              i32,
    pub status:                   String,
    pub total_cents:              i32,
    pub stripe_payment_intent_id: Option<String>,
    pub pickup_code:              String,
    pub pickup_code_used_at:      Option<DateTime<Utc>>,
    pub estimated_pickup_at:      Option<DateTime<Utc>>,
    pub created_at:               DateTime<Utc>,
    pub updated_at:               DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize, utoipa::ToSchema)]
pub struct WhiskedOrderItemRow {
    pub id:           i32,
    pub order_id:     i32,
    pub menu_item_id: i32,
    pub quantity:     i32,
    pub price_cents:  i32,
    pub created_at:   DateTime<Utc>,
}

pub const WHISKED_ORDER_COLS: &str =
    "id, user_id, business_id, status, total_cents, stripe_payment_intent_id, \
     pickup_code, pickup_code_used_at, estimated_pickup_at, created_at, updated_at";

pub const WHISKED_ORDER_ITEM_COLS: &str =
    "id, order_id, menu_item_id, quantity, price_cents, created_at";

// ── Request bodies ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct PlaceOrderRequest {
    pub business_id: i32,
    pub items:       Vec<OrderItemRequest>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct OrderItemRequest {
    pub menu_item_id: i32,
    pub quantity:     i32,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ValidatePickupRequest {
    pub pickup_code: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateOrderStatusRequest {
    /// Allowed transitions are `preparing` (from `pending`) and `ready` (from
    /// `preparing`). `collected` is only set via the validate-pickup path.
    pub status: String,
}

// ── Response bodies ──────────────────────────────────────────────────────────

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct WhiskedOrderResponse {
    pub order:                 WhiskedOrderRow,
    pub items:                 Vec<WhiskedOrderItemRow>,
    pub pickup_code:           String,
    /// PaymentIntent client_secret returned by Stripe at order placement.
    /// `None` if the Stripe key is unset (dev / test mode) — the iOS Stripe
    /// SDK uses this to confirm the payment without round-tripping the
    /// secret key.
    pub stripe_client_secret:  Option<String>,
    /// `users.display_name` for the customer who placed this order. `None`
    /// if the customer hasn't set one. Surfaced so the staff dashboard
    /// can show e.g. "Lara M. — W-4829" on each order card.
    pub customer_name:         Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PickupCodeResponse {
    pub pickup_code: String,
}
