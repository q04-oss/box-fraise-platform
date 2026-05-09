use sqlx::PgPool;

use crate::error::{AppResult, DomainError};
use super::{repository, types::WhiskedMenuItemRow};

/// List every available menu item. Public surface — no auth context
/// required, runs against a pool connection (no RLS scoping).
pub async fn list_menu(pool: &PgPool) -> AppResult<Vec<WhiskedMenuItemRow>> {
    let mut conn = pool.acquire().await.map_err(DomainError::Db)?;
    repository::list_available(&mut conn).await
}
