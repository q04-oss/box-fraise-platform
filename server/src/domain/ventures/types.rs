use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct VentureRow {
    pub id:          i32,
    pub name:        String,
    pub description: Option<String>,
    pub ceo_type:    String,   // "human" | "dorotka"
    pub ceo_user_id: Option<i32>,
    pub status:      String,
    pub fraise_cut:  f64,
    pub created_at:  NaiveDateTime,
}

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct VentureMemberRow {
    pub user_id:     i32,
    pub venture_id:  i32,
    pub role:        String,   // "owner" | "worker" | "contractor"
    pub joined_at:   NaiveDateTime,
    pub display_name: Option<String>,
}

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct VenturePostRow {
    pub id:         i32,
    pub venture_id: i32,
    pub author_id:  i32,
    pub body:       String,
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Deserialize)]
pub struct CreateVentureBody {
    pub name:        String,
    pub description: Option<String>,
    pub ceo_type:    Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PostBody {
    pub body: String,
}
