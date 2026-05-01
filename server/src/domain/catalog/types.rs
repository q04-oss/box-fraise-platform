use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

// ── Varieties ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct VarietyRow {
    pub id:               i32,
    pub name:             String,
    pub description:      Option<String>,
    pub farm_source:      Option<String>,
    pub price_cents:      i32,
    pub stock:            i32,
    pub harvest_date:     Option<NaiveDate>,
    pub location_id:      Option<i32>,
    pub image_url:        Option<String>,
    pub active:           bool,
    pub variety_type:     Option<String>,
    pub social_tier:      Option<String>,
    pub time_credits_days: Option<i32>,
}

// ── Locations ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct LocationRow {
    pub id:          i32,
    pub name:        String,
    pub address:     Option<String>,
    pub active:      bool,
    pub walk_in:     Option<bool>,
    pub beacon_uuid: Option<String>,
    pub business_id: Option<i32>,
}

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct BatchStatusEntry {
    pub variety_id: i32,
    pub name:       String,
    pub queued:     i64,
}

// ── Time slots ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct TimeSlotRow {
    pub id:           i32,
    pub location_id:  i32,
    pub date:         NaiveDate,
    pub time:         String,
    pub capacity:     i32,
    pub booked_count: i32,
}

#[derive(Debug, Deserialize)]
pub struct SlotsQuery {
    pub location_id: i32,
    pub date:        String, // "YYYY-MM-DD"
}

#[derive(Debug, Deserialize)]
pub struct TimeSlotsQuery {
    pub location_id: Option<i32>,
    pub date:        Option<String>,
}
