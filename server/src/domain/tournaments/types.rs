use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct TournamentRow {
    pub id:           i32,
    pub name:         String,
    pub entry_fee_cents: i64,
    pub status:       String, // "open" | "active" | "completed"
    pub max_players:  Option<i32>,
    pub starts_at:    Option<NaiveDateTime>,
    pub created_at:   NaiveDateTime,
}

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct TournamentEntryRow {
    pub id:            i32,
    pub tournament_id: i32,
    pub user_id:       i32,
    pub deck_json:     Option<serde_json::Value>,
    pub status:        String, // "registered" | "active" | "eliminated" | "winner"
    pub created_at:    NaiveDateTime,
}

#[derive(Debug, Deserialize)]
pub struct EnterTournamentBody {
    pub deck_json: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct RegisterDeckBody {
    pub deck_json: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct PlayCardBody {
    pub card_id:   String,
    pub target_id: Option<String>,
}

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct TournamentPlayRow {
    pub id:            i32,
    pub tournament_id: i32,
    pub round:         i32,
    pub player_id:     i32,
    pub card_id:       String,
    pub target_id:     Option<String>,
    pub played_at:     NaiveDateTime,
}
