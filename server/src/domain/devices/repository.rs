use rand::Rng;
use sqlx::PgPool;

use crate::error::{AppError, AppResult};
use super::types::DeviceRow;

// ── Pairing tokens ────────────────────────────────────────────────────────────

pub async fn create_pair_token(pool: &PgPool, user_id: i32) -> AppResult<String> {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let token: String = (0..8)
        .map(|_| CHARSET[rand::thread_rng().gen_range(0..CHARSET.len())] as char)
        .collect();

    let expires_at = chrono::Utc::now().naive_utc() + chrono::Duration::minutes(5);

    sqlx::query(
        "INSERT INTO device_pairing_tokens (token, user_id, expires_at)
         VALUES ($1, $2, $3)",
    )
    .bind(&token)
    .bind(user_id)
    .bind(expires_at)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;

    Ok(token)
}

/// Atomically consume a pairing token. Returns the owning user_id on success.
pub async fn consume_pair_token(pool: &PgPool, token: &str) -> AppResult<Option<i32>> {
    let row: Option<(i32,)> = sqlx::query_as(
        "DELETE FROM device_pairing_tokens
         WHERE UPPER(token) = UPPER($1) AND expires_at > NOW()
         RETURNING user_id",
    )
    .bind(token)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)?;

    Ok(row.map(|(id,)| id))
}

// ── Device registration ───────────────────────────────────────────────────────

pub async fn insert_device(
    pool:           &PgPool,
    user_id:        i32,
    device_address: &str,
) -> AppResult<DeviceRow> {
    sqlx::query_as(
        "INSERT INTO devices (device_address, user_id, role)
         VALUES (LOWER($1), $2, 'user')
         ON CONFLICT (device_address) DO UPDATE
         SET user_id = EXCLUDED.user_id
         RETURNING id, device_address, user_id, role, created_at",
    )
    .bind(device_address)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(AppError::Db)
}

pub async fn find_device(pool: &PgPool, address: &str) -> AppResult<Option<DeviceRow>> {
    sqlx::query_as(
        "SELECT id, device_address, user_id, role, created_at
         FROM devices
         WHERE LOWER(device_address) = LOWER($1)",
    )
    .bind(address)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)
}

pub async fn list_devices(pool: &PgPool, user_id: i32) -> AppResult<Vec<DeviceRow>> {
    sqlx::query_as(
        "SELECT id, device_address, user_id, role, created_at
         FROM devices
         WHERE user_id = $1
         ORDER BY created_at DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)
}

pub async fn set_role(pool: &PgPool, address: &str, role: &str) -> AppResult<()> {
    let result = sqlx::query(
        "UPDATE devices SET role = $1 WHERE LOWER(device_address) = LOWER($2)",
    )
    .bind(role)
    .bind(address)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(())
}

// ── App Attest ────────────────────────────────────────────────────────────────

pub async fn upsert_attestation(
    pool:        &PgPool,
    user_id:     i32,
    key_id:      &str,
    attestation: &str,
    hmac_key:    &str,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO device_attestations (key_id, attestation, user_id, hmac_key)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (key_id) DO UPDATE
         SET hmac_key    = COALESCE(EXCLUDED.hmac_key, device_attestations.hmac_key),
             attestation = EXCLUDED.attestation,
             user_id     = EXCLUDED.user_id",
    )
    .bind(key_id)
    .bind(attestation)
    .bind(user_id)
    .bind(hmac_key)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;
    Ok(())
}
