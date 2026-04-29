use rand::Rng;
use sqlx::PgPool;

use crate::error::{AppError, AppResult};
use super::types::{BatchStatusEntry, LocationRow, TimeSlotRow, VarietyRow};

// ── Varieties ─────────────────────────────────────────────────────────────────

pub async fn list_varieties(pool: &PgPool) -> AppResult<Vec<VarietyRow>> {
    sqlx::query_as(
        "SELECT v.id, v.name, v.description, v.farm_source, v.price_cents, v.stock,
                v.harvest_date, v.location_id, v.image_url, v.active,
                v.variety_type, v.social_tier, v.time_credits_days,
                AVG(o.rating)::float8  AS avg_rating,
                COUNT(o.rating)        AS rating_count
         FROM varieties v
         LEFT JOIN orders o ON o.variety_id = v.id AND o.rating IS NOT NULL
         WHERE v.active = true
         GROUP BY v.id
         ORDER BY v.id",
    )
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)
}

pub async fn find_variety(pool: &PgPool, id: i32) -> AppResult<Option<VarietyRow>> {
    sqlx::query_as(
        "SELECT v.id, v.name, v.description, v.farm_source, v.price_cents, v.stock,
                v.harvest_date, v.location_id, v.image_url, v.active,
                v.variety_type, v.social_tier, v.time_credits_days,
                AVG(o.rating)::float8 AS avg_rating,
                COUNT(o.rating)       AS rating_count
         FROM varieties v
         LEFT JOIN orders o ON o.variety_id = v.id AND o.rating IS NOT NULL
         WHERE v.id = $1
         GROUP BY v.id",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)
}

/// Varieties the user has collected at least once ("passport").
pub async fn user_passport(pool: &PgPool, user_id: i32) -> AppResult<Vec<VarietyRow>> {
    sqlx::query_as(
        "SELECT DISTINCT ON (v.id)
                v.id, v.name, v.description, v.farm_source, v.price_cents, v.stock,
                v.harvest_date, v.location_id, v.image_url, v.active,
                v.variety_type, v.social_tier, v.time_credits_days,
                AVG(o2.rating)::float8 AS avg_rating,
                COUNT(o2.rating)       AS rating_count
         FROM orders o
         JOIN varieties v ON v.id = o.variety_id
         LEFT JOIN orders o2 ON o2.variety_id = v.id AND o2.rating IS NOT NULL
         WHERE o.user_id = $1 AND o.status = 'collected'
         GROUP BY v.id
         ORDER BY v.id",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)
}

// ── Locations ─────────────────────────────────────────────────────────────────

pub async fn list_locations(pool: &PgPool) -> AppResult<Vec<LocationRow>> {
    sqlx::query_as(
        "SELECT id, name, address, active, walk_in, beacon_uuid, business_id
         FROM locations
         WHERE active = true
         ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)
}

pub async fn find_location(pool: &PgPool, id: i32) -> AppResult<Option<LocationRow>> {
    sqlx::query_as(
        "SELECT id, name, address, active, walk_in, beacon_uuid, business_id
         FROM locations WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)
}

pub async fn batch_status(pool: &PgPool, location_id: i32) -> AppResult<Vec<BatchStatusEntry>> {
    sqlx::query_as(
        "SELECT o.variety_id, v.name, COUNT(*)::bigint AS queued
         FROM orders o
         JOIN varieties v ON v.id = o.variety_id
         WHERE o.location_id = $1 AND o.status = 'queued'
         GROUP BY o.variety_id, v.name
         ORDER BY o.variety_id",
    )
    .bind(location_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)
}

// ── Time slots ────────────────────────────────────────────────────────────────

/// Fetch slots for a location + date, generating them on demand if none exist.
pub async fn get_or_generate_slots(
    pool:        &PgPool,
    location_id: i32,
    date:        &str,
) -> AppResult<Vec<TimeSlotRow>> {
    let existing: Vec<TimeSlotRow> = sqlx::query_as(
        "SELECT id, location_id, date, time, capacity, booked_count
         FROM time_slots
         WHERE location_id = $1 AND date = $2::date
         ORDER BY time",
    )
    .bind(location_id)
    .bind(date)
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)?;

    if !existing.is_empty() {
        return Ok(existing);
    }

    // Generate capacities before any await — ThreadRng is !Send and cannot
    // be held across an await point in a tokio task.
    let slots: Vec<(String, i32)> = {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        (9..=17u32)
            .map(|h| (format!("{h:02}:00"), rng.gen_range(2..=5)))
            .collect()
    }; // rng dropped here, before any await

    for (time, capacity) in &slots {
        // ON CONFLICT DO NOTHING handles races between concurrent slot requests.
        let _ = sqlx::query(
            "INSERT INTO time_slots (location_id, date, time, capacity, booked_count)
             VALUES ($1, $2::date, $3, $4, 0)
             ON CONFLICT DO NOTHING",
        )
        .bind(location_id)
        .bind(date)
        .bind(time)
        .bind(capacity)
        .execute(pool)
        .await;
    }

    sqlx::query_as(
        "SELECT id, location_id, date, time, capacity, booked_count
         FROM time_slots
         WHERE location_id = $1 AND date = $2::date
         ORDER BY time",
    )
    .bind(location_id)
    .bind(date)
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)
}

/// Public slot listing — only slots with available capacity.
/// NULL parameters are treated as "no filter" via IS NULL OR = $n in SQL.
pub async fn available_slots(
    pool:        &PgPool,
    location_id: Option<i32>,
    date:        Option<&str>,
) -> AppResult<Vec<TimeSlotRow>> {
    sqlx::query_as(
        "SELECT id, location_id, date, time, capacity, booked_count
         FROM time_slots
         WHERE capacity > booked_count
           AND ($1::int  IS NULL OR location_id = $1)
           AND ($2::date IS NULL OR date        = $2::date)
         ORDER BY date, time",
    )
    .bind(location_id)
    .bind(date)
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)
}
