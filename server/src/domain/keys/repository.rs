use sqlx::PgPool;

use crate::error::{AppError, AppResult};
use super::types::{OtpkRow, UserKeysRow};

// ── Challenges ────────────────────────────────────────────────────────────────

/// Issue a fresh challenge and persist it.
pub async fn create_challenge(pool: &PgPool, user_id: i32) -> AppResult<String> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use rand::RngCore;

    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let challenge = STANDARD.encode(bytes);

    let expires_at = chrono::Utc::now().naive_utc() + chrono::Duration::minutes(5);

    sqlx::query(
        "INSERT INTO key_challenges (user_id, challenge, expires_at)
         VALUES ($1, $2, $3)",
    )
    .bind(user_id)
    .bind(&challenge)
    .bind(expires_at)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;

    Ok(challenge)
}

/// Atomically consume the most recent valid challenge for the user.
/// Returns the challenge string on success, None if none is available.
pub async fn consume_challenge(pool: &PgPool, user_id: i32) -> AppResult<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as(
        "UPDATE key_challenges
         SET used = true
         WHERE id = (
             SELECT id FROM key_challenges
             WHERE user_id = $1
               AND used = false
               AND expires_at > NOW()
             ORDER BY created_at DESC
             LIMIT 1
         )
         RETURNING challenge",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)?;

    Ok(row.map(|(c,)| c))
}

// ── Key material ──────────────────────────────────────────────────────────────

pub async fn upsert_user_keys(
    pool:                 &PgPool,
    user_id:              i32,
    identity_key:         &str,
    identity_signing_key: Option<&str>,
    signed_pre_key:       &str,
    signed_pre_key_sig:   &str,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO user_keys
             (user_id, identity_key, identity_signing_key, signed_pre_key, signed_pre_key_sig)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (user_id) DO UPDATE
         SET identity_key         = EXCLUDED.identity_key,
             identity_signing_key = COALESCE(EXCLUDED.identity_signing_key, user_keys.identity_signing_key),
             signed_pre_key       = EXCLUDED.signed_pre_key,
             signed_pre_key_sig   = EXCLUDED.signed_pre_key_sig,
             updated_at           = NOW()",
    )
    .bind(user_id)
    .bind(identity_key)
    .bind(identity_signing_key)
    .bind(signed_pre_key)
    .bind(signed_pre_key_sig)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;
    Ok(())
}

pub async fn find_user_keys(pool: &PgPool, user_id: i32) -> AppResult<Option<UserKeysRow>> {
    sqlx::query_as(
        "SELECT user_id, identity_key, identity_signing_key, signed_pre_key, signed_pre_key_sig
         FROM user_keys
         WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)
}

// ── One-time pre-keys ─────────────────────────────────────────────────────────

pub async fn insert_otpks(
    pool:    &PgPool,
    user_id: i32,
    keys:    &[(i32, String)], // (key_id, public_key)
) -> AppResult<()> {
    // Batch insert — one statement per key to keep it simple and avoid
    // unnest/array gymnastics. Volume is low (≤ 100 keys per upload).
    for (key_id, public_key) in keys {
        sqlx::query(
            "INSERT INTO one_time_pre_keys (user_id, key_id, public_key)
             VALUES ($1, $2, $3)
             ON CONFLICT DO NOTHING",
        )
        .bind(user_id)
        .bind(key_id)
        .bind(public_key)
        .execute(pool)
        .await
        .map_err(AppError::Db)?;
    }
    Ok(())
}

pub async fn count_otpks(pool: &PgPool, user_id: i32) -> AppResult<i64> {
    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM one_time_pre_keys WHERE user_id = $1 AND used = false")
            .bind(user_id)
            .fetch_one(pool)
            .await
            .map_err(AppError::Db)?;
    Ok(count)
}

/// Atomically claim the oldest unused OPK for a user. Returns None if the
/// user has no remaining OPKs — the caller should still serve the bundle
/// without one (the receiver falls back to the signed pre-key).
pub async fn claim_otpk(pool: &PgPool, user_id: i32) -> AppResult<Option<OtpkRow>> {
    sqlx::query_as(
        "UPDATE one_time_pre_keys
         SET used = true
         WHERE id = (
             SELECT id FROM one_time_pre_keys
             WHERE user_id = $1 AND used = false
             ORDER BY key_id ASC
             LIMIT 1
         )
         RETURNING key_id, public_key",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)
}

// ── Bundle lookup ─────────────────────────────────────────────────────────────

/// Find a user_id by their 6-character user_code.
pub async fn user_id_by_code(pool: &PgPool, code: &str) -> AppResult<Option<i32>> {
    let row: Option<(i32,)> =
        sqlx::query_as("SELECT id FROM users WHERE UPPER(user_code) = UPPER($1)")
            .bind(code)
            .fetch_optional(pool)
            .await
            .map_err(AppError::Db)?;
    Ok(row.map(|(id,)| id))
}
