use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

// ── Stored row ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct OrderRow {
    pub id:                        i32,
    pub user_id:                   Option<i32>,
    pub variety_id:                i32,
    pub location_id:               i32,
    pub time_slot_id:              Option<i32>,
    pub batch_id:                  Option<i32>,
    pub quantity:                  i32,
    pub chocolate:                 Option<String>,
    pub finish:                    Option<String>,
    pub is_gift:                   bool,
    pub total_cents:               i32,
    pub stripe_payment_intent_id:  Option<String>,
    pub status:                    String,
    pub nfc_token:                 Option<String>,
    pub rating:                    Option<i32>,
    pub created_at:                NaiveDateTime,
}

// ── Request bodies ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateOrderBody {
    pub variety_id:   i32,
    pub location_id:  i32,
    pub time_slot_id: Option<i32>,
    pub quantity:     i32,
    pub chocolate:    Option<String>,
    pub finish:       Option<String>,
    pub is_gift:      Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct PaymentIntentBody {
    pub variety_id:    i32,
    pub location_id:   i32,
    pub quantity:      i32,
    pub chocolate:     Option<String>,
    pub finish:        Option<String>,
    pub referral_code: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RateOrderBody {
    pub rating: i32,
}

#[derive(Debug, Deserialize)]
pub struct ScanCollectBody {
    pub nfc_token: String,
}

// ── Response bodies ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct CreateOrderResponse {
    pub order:         OrderRow,
    pub client_secret: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PaymentIntentResponse {
    pub client_secret:    String,
    pub total_cents:      i32,
    pub discount_applied: bool,
}
