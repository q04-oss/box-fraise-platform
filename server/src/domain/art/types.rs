use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct ArtworkRow {
    pub id:          i32,
    pub pitch_id:    Option<i32>,
    pub title:       String,
    pub media_url:   Option<String>,
    pub description: Option<String>,
    pub status:      String,
    pub created_at:  NaiveDateTime,
}

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct AuctionRow {
    pub id:           i32,
    pub artwork_id:   i32,
    pub reserve_cents: Option<i64>,
    pub starts_at:    Option<NaiveDateTime>,
    pub ends_at:      Option<NaiveDateTime>,
    pub status:       String,
}

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct BidRow {
    pub id:                       i32,
    pub auction_id:               i32,
    pub user_id:                  i32,
    pub amount_cents:             i64,
    pub stripe_payment_intent_id: Option<String>,
    pub created_at:               NaiveDateTime,
}

#[derive(Debug, Deserialize)]
pub struct PitchBody {
    pub title:       String,
    pub description: Option<String>,
    pub media_url:   Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BidBody {
    /// Bid amount in cents. Must exceed current highest bid.
    pub amount_cents: i64,
}
