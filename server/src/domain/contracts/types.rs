use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct ContractRow {
    pub id:          i32,
    pub business_id: i32,
    pub user_id:     i32,
    pub status:      String,
    pub note:        Option<String>,
    pub start_date:  Option<NaiveDateTime>,
    pub end_date:    Option<NaiveDateTime>,
    pub created_at:  NaiveDateTime,
}

#[derive(Debug, Deserialize)]
pub struct RespondBody {
    /// Optional note from the user (e.g. decline reason).
    pub note: Option<String>,
}
