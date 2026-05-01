use sqlx::PgPool;

use crate::error::{AppError, AppResult};
use super::types::BusinessRow;

pub async fn list(pool: &PgPool) -> AppResult<Vec<BusinessRow>> {
    sqlx::query_as::<_, BusinessRow>(
        "SELECT id, name, business_type, address, description, hours,
                instagram, latitude, longitude, active, walk_in,
                capacity, entrance_fee_cents, created_at
         FROM businesses
         WHERE active = true
         ORDER BY name",
    )
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)
}
