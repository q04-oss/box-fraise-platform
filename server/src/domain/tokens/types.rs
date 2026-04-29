use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

// ── Evening tokens ────────────────────────────────────────────────────────────

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct EveningTokenRow {
    pub id:           i32,
    pub user_id_1:    i32,
    pub user_id_2:    i32,
    pub booking_id:   Option<i32>,
    pub minted_at:    NaiveDateTime,
}

// ── Content tokens ────────────────────────────────────────────────────────────

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct ContentTokenRow {
    pub id:          i32,
    pub creator_id:  i32,
    pub owner_id:    i32,
    pub archetype:   Option<String>,
    pub power:       Option<i32>,
    pub rarity:      Option<String>,
    pub created_at:  NaiveDateTime,
}

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct TradeOfferRow {
    pub id:          i32,
    pub token_id:    i32,
    pub from_user_id: i32,
    pub to_user_id:  i32,
    pub status:      String,
    pub created_at:  NaiveDateTime,
}

#[derive(Debug, Deserialize)]
pub struct TradeOfferBody {
    pub token_id:   i32,
    pub to_user_id: i32,
}

// ── Portrait tokens ───────────────────────────────────────────────────────────

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct PortraitTokenRow {
    pub id:           i32,
    pub owner_id:     i32,
    pub creator_id:   i32,
    pub media_url:    Option<String>,
    pub created_at:   NaiveDateTime,
}

#[derive(Debug, Deserialize)]
pub struct MintPortraitBody {
    pub media_url:    String,
    pub subject_id:   i32,
}
