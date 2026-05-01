use chrono::NaiveDateTime;
use serde::Serialize;

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct BusinessRow {
    pub id:            i32,
    pub name:          String,
    pub business_type: Option<String>,
    pub address:       Option<String>,
    pub description:   Option<String>,
    pub hours:         Option<String>,
    pub instagram:     Option<String>,
    pub latitude:      Option<f64>,
    pub longitude:     Option<f64>,
    pub active:        bool,
    pub walk_in:       Option<bool>,
    pub capacity:      Option<i32>,
    pub entrance_fee_cents: Option<i32>,
    pub created_at:    NaiveDateTime,
}

