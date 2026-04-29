use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct GiftRow {
    pub id:              i32,
    pub sender_id:       i32,
    pub recipient_email: Option<String>,
    pub recipient_phone: Option<String>,
    pub gift_type:       String,
    pub amount_cents:    Option<i32>,
    pub claim_token:     String,
    pub claimed_at:      Option<NaiveDateTime>,
    pub created_at:      NaiveDateTime,
}

#[derive(Debug, Deserialize)]
pub struct SendGiftBody {
    pub recipient_email: Option<String>,
    pub recipient_phone: Option<String>,
    pub gift_type:       String,       // digital | physical | bundle
    pub amount_cents:    Option<i32>,
}
