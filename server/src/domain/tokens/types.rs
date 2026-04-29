use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use crate::types::UserId;

// ── Evening tokens ────────────────────────────────────────────────────────────

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct EveningTokenRow {
    pub id:           i32,
    pub user_id_1:    UserId,
    pub user_id_2:    UserId,
    pub booking_id:   Option<i32>,
    pub minted_at:    NaiveDateTime,
}

// ── Content tokens ────────────────────────────────────────────────────────────

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct ContentTokenRow {
    pub id:          i32,
    pub creator_id:  UserId,
    pub owner_id:    UserId,
    pub archetype:   Option<String>,
    pub power:       Option<i32>,
    pub rarity:      Option<String>,
    pub created_at:  NaiveDateTime,
}

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct TradeOfferRow {
    pub id:          i32,
    pub token_id:    i32,
    pub from_user_id: UserId,
    pub to_user_id:  UserId,
    pub status:      String,
    pub created_at:  NaiveDateTime,
}

#[derive(Debug, Deserialize)]
pub struct TradeOfferBody {
    pub token_id:   i32,
    pub to_user_id: UserId,
}

// ── Portrait tokens ───────────────────────────────────────────────────────────

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct PortraitTokenRow {
    pub id:           i32,
    pub owner_id:     UserId,
    pub creator_id:   UserId,
    pub media_url:    Option<String>,
    pub created_at:   NaiveDateTime,
}

#[derive(Debug, Deserialize)]
pub struct MintPortraitBody {
    pub media_url:    String,
    pub subject_id:   UserId,
}
