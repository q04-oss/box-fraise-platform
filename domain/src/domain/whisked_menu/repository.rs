#![allow(missing_docs)]
use sqlx::PgConnection;

use crate::error::{AppResult, DomainError};
use super::types::{WhiskedMenuItemRow, WHISKED_MENU_COLS};

/// Available items, ordered by `sort_order` ASC.
pub async fn list_available(conn: &mut PgConnection) -> AppResult<Vec<WhiskedMenuItemRow>> {
    sqlx::query_as(&format!(
        "SELECT {WHISKED_MENU_COLS} FROM whisked_menu_items \
         WHERE available = true ORDER BY sort_order ASC, id ASC"
    ))
    .fetch_all(conn)
    .await
    .map_err(DomainError::Db)
}

/// Fetch a specific set of menu items by id. Used by the order placement
/// flow to validate that every item the customer picked is still on the
/// menu and still available; returns only the rows that exist, callers
/// must compare counts to detect missing ids.
pub async fn list_by_ids(
    conn: &mut PgConnection,
    ids: &[i32],
) -> AppResult<Vec<WhiskedMenuItemRow>> {
    sqlx::query_as(&format!(
        "SELECT {WHISKED_MENU_COLS} FROM whisked_menu_items \
         WHERE id = ANY($1) ORDER BY sort_order ASC, id ASC"
    ))
    .bind(ids)
    .fetch_all(conn)
    .await
    .map_err(DomainError::Db)
}
