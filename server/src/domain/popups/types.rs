use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct PopupRow {
    pub id:               i32,
    pub name:             String,
    pub address:          Option<String>,
    pub description:      Option<String>,
    pub capacity:         Option<i32>,
    pub entrance_fee_cents: Option<i32>,
    pub active:           bool,
    pub created_at:       NaiveDateTime,
}

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct RsvpRow {
    pub id:                       i32,
    pub user_id:                  i32,
    pub business_id:              i32,
    pub status:                   String,
    pub stripe_payment_intent_id: Option<String>,
    pub created_at:               NaiveDateTime,
}

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct NominationRow {
    pub id:          i32,
    pub popup_id:    i32,
    pub nominator_id: i32,
    pub nominee_id:  i32,
    pub created_at:  NaiveDateTime,
}

#[derive(Debug, Deserialize)]
pub struct NominateBody {
    pub message: Option<String>,
}
