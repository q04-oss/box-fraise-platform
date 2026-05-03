#![allow(missing_docs)]
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── Database rows ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct OrderRow {
    pub id:                       i32,
    pub user_id:                  i32,
    pub business_id:              i32,
    pub visit_id:                 Option<i32>,
    pub stripe_payment_intent_id: Option<String>,
    pub variety_description:      Option<String>,
    pub box_count:                i32,
    pub amount_cents:             i32,
    pub status:                   String,
    pub collected_via_box_id:     Option<i32>,
    pub pickup_deadline:          Option<DateTime<Utc>>,
    pub updated_at:               DateTime<Utc>,
    pub created_at:               DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct VisitBoxRow {
    pub id:                      i32,
    pub visit_id:                i32,
    pub assigned_order_id:       Option<i32>,
    pub nfc_chip_uid:            String,
    pub quantity:                i32,
    pub chain_of_custody_hash:   Option<String>,
    pub pack_signature:          Option<String>,
    pub delivery_signature:      Option<String>,
    pub activated_at:            Option<DateTime<Utc>>,
    pub expires_at:              Option<DateTime<Utc>>,
    pub tapped_by_user_id:       Option<i32>,
    pub tapped_at:               Option<DateTime<Utc>>,
    pub collection_confirmed_at: Option<DateTime<Utc>>,
    pub clone_detected:          bool,
    pub clone_detected_at:       Option<DateTime<Utc>>,
    pub clone_alert_sent_at:     Option<DateTime<Utc>>,
    pub returned_at:             Option<DateTime<Utc>>,
    pub disposal_reason:         Option<String>,
    pub is_gift:                 bool,
    pub gift_reason:             Option<String>,
    pub covered_by:              Option<String>,
    pub created_at:              DateTime<Utc>,
}

// ── Column lists ──────────────────────────────────────────────────────────────

pub const ORDER_COLS: &str =
    "id, user_id, business_id, visit_id, stripe_payment_intent_id, \
     variety_description, box_count, amount_cents, status, \
     collected_via_box_id, pickup_deadline, updated_at, created_at";

pub const VISIT_BOX_COLS: &str =
    "id, visit_id, assigned_order_id, nfc_chip_uid, quantity, \
     chain_of_custody_hash, pack_signature, delivery_signature, \
     activated_at, expires_at, tapped_by_user_id, tapped_at, \
     collection_confirmed_at, clone_detected, clone_detected_at, \
     clone_alert_sent_at, returned_at, disposal_reason, \
     is_gift, gift_reason, covered_by, created_at";

// ── Request bodies ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateOrderRequest {
    pub business_id:         i32,
    pub variety_description: Option<String>,
    /// Must be >= 1.
    pub box_count:           i32,
    /// Must be > 0 (in cents).
    pub amount_cents:        i32,
}

#[derive(Debug, Deserialize)]
pub struct ActivateBoxRequest {
    pub nfc_chip_uid:       String,
    pub delivery_signature: String,
    pub expires_at:         DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CollectOrderRequest {
    pub nfc_chip_uid: String,
}

// ── Response bodies ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct OrderResponse {
    pub id:                   i32,
    pub business_id:          i32,
    pub variety_description:  Option<String>,
    pub box_count:            i32,
    pub amount_cents:         i32,
    pub status:               String,
    pub pickup_deadline:      Option<DateTime<Utc>>,
    pub collected_via_box_id: Option<i32>,
    pub created_at:           DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct VisitBoxResponse {
    pub id:          i32,
    pub nfc_chip_uid: String,
    pub quantity:    i32,
    pub activated_at: Option<DateTime<Utc>>,
    pub expires_at:  Option<DateTime<Utc>>,
    pub tapped_at:   Option<DateTime<Utc>>,
    pub is_gift:     bool,
    pub gift_reason: Option<String>,
}
