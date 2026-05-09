#![allow(missing_docs)]
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// One row of `whisked_menu_items`.
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize, utoipa::ToSchema)]
pub struct WhiskedMenuItemRow {
    pub id:          i32,
    pub name:        String,
    pub description: Option<String>,
    pub price_cents: i32,
    pub category:    String,
    pub available:   bool,
    pub sort_order:  i32,
    pub created_at:  DateTime<Utc>,
    pub updated_at:  DateTime<Utc>,
}

pub const WHISKED_MENU_COLS: &str =
    "id, name, description, price_cents, category, available, sort_order, created_at, updated_at";
