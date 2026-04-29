use chrono::NaiveDateTime;
use serde::Serialize;

use crate::types::UserId;

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct CampaignRow {
    pub id:           i32,
    pub title:        String,
    pub concept:      Option<String>,
    pub salon_id:     Option<i32>,
    pub date:         Option<NaiveDateTime>,
    pub spots:        Option<i32>,
    pub status:       String,
    pub created_at:   NaiveDateTime,
}

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct SignupRow {
    pub id:          i32,
    pub user_id:     UserId,
    pub campaign_id: i32,
    pub waitlist:    bool,
    pub created_at:  NaiveDateTime,
}
