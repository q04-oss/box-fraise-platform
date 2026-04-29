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

pub async fn find(pool: &PgPool, id: i32) -> AppResult<Option<BusinessRow>> {
    sqlx::query_as::<_, BusinessRow>(
        "SELECT id, name, business_type, address, description, hours,
                instagram, latitude, longitude, active, walk_in,
                capacity, entrance_fee_cents, created_at
         FROM businesses WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)
}

/// Return the push_token of the currently placed user at this business.
pub async fn placed_user_push_token(
    pool:        &PgPool,
    business_id: i32,
) -> AppResult<Option<(i32, Option<String>)>> {
    #[derive(sqlx::FromRow)]
    struct Row { id: i32, push_token: Option<String> }

    let row: Option<Row> = sqlx::query_as(
        "SELECT u.id, u.push_token
         FROM employment_contracts ec
         JOIN users u ON u.id = ec.user_id
         WHERE ec.business_id = $1
           AND ec.status = 'active'
         ORDER BY ec.created_at DESC
         LIMIT 1",
    )
    .bind(business_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)?;

    Ok(row.map(|r| (r.id, r.push_token)))
}
