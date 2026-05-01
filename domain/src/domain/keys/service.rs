use base64::{engine::general_purpose::STANDARD, Engine};
use ring::signature::{self, UnparsedPublicKey};
use sqlx::PgPool;

use crate::error::{AppError, AppResult};
use crate::types::{KeyId, UserId};
use super::{
    repository,
    types::{KeyBundleResponse, OtpkResponse, RegisterKeysBody},
};

// ── Challenge ─────────────────────────────────────────────────────────────────

pub async fn issue_challenge(pool: &PgPool, user_id: UserId) -> AppResult<String> {
    repository::create_challenge(pool, user_id).await
}

// ── Key registration ──────────────────────────────────────────────────────────

pub async fn register_keys(
    pool:    &PgPool,
    user_id: UserId,
    body:    RegisterKeysBody,
) -> AppResult<()> {
    match (&body.identity_signing_key, &body.challenge_sig) {
        (Some(signing_key_b64), Some(sig_b64)) => {
            let challenge = repository::consume_challenge(pool, user_id)
                .await?
                .ok_or_else(|| AppError::bad_request("no valid challenge found — request one first"))?;

            verify_ed25519(challenge.as_bytes(), sig_b64, signing_key_b64)?;
        }
        (Some(_), None) => {
            return Err(AppError::bad_request(
                "challenge_sig is required when identity_signing_key is provided",
            ));
        }
        _ => {}
    }

    repository::upsert_user_keys(
        pool,
        user_id,
        &body.identity_key,
        body.identity_signing_key.as_deref(),
        &body.signed_pre_key,
        &body.signed_pre_key_sig,
    )
    .await?;

    if !body.one_time_pre_keys.is_empty() {
        let pairs: Vec<(KeyId, String)> = body
            .one_time_pre_keys
            .into_iter()
            .map(|k| (k.key_id, k.public_key))
            .collect();
        repository::insert_otpks(pool, user_id, &pairs).await?;
    }

    Ok(())
}

// ── OPK management ────────────────────────────────────────────────────────────

pub async fn upload_otpks(pool: &PgPool, user_id: UserId, keys: Vec<(KeyId, String)>) -> AppResult<()> {
    repository::insert_otpks(pool, user_id, &keys).await
}

pub async fn otpk_count(pool: &PgPool, user_id: UserId) -> AppResult<i64> {
    repository::count_otpks(pool, user_id).await
}

// ── Key bundle (X3DH) ─────────────────────────────────────────────────────────

const KEY_REFRESH_GRACE_DAYS: i64 = 30;

pub async fn fetch_bundle(pool: &PgPool, target_id: UserId) -> AppResult<KeyBundleResponse> {
    let keys = repository::find_user_keys(pool, target_id)
        .await?
        .ok_or(AppError::NotFound)?;

    if keys.identity_signing_key.is_none() {
        let age_days = (chrono::Utc::now().naive_utc() - keys.updated_at).num_days();
        if age_days >= KEY_REFRESH_GRACE_DAYS {
            return Err(AppError::Unprocessable("keys_expired".to_string()));
        }
        return Err(AppError::Conflict("keys_need_refresh".to_string()));
    }

    let otpk = repository::claim_otpk(pool, target_id).await?.map(|r| OtpkResponse {
        key_id:     r.key_id,
        public_key: r.public_key,
    });

    Ok(KeyBundleResponse {
        user_id:              target_id,
        identity_key:         keys.identity_key,
        identity_signing_key: keys.identity_signing_key,
        signed_pre_key:       keys.signed_pre_key,
        signed_pre_key_sig:   keys.signed_pre_key_sig,
        one_time_pre_key:     otpk,
    })
}

pub async fn fetch_bundle_by_code(pool: &PgPool, code: &str) -> AppResult<KeyBundleResponse> {
    let target_id = repository::user_id_by_code(pool, code)
        .await?
        .ok_or(AppError::NotFound)?;

    fetch_bundle(pool, target_id).await
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn verify_ed25519(message: &[u8], sig_b64: &str, pubkey_b64: &str) -> AppResult<()> {
    let sig = STANDARD
        .decode(sig_b64)
        .map_err(|_| AppError::bad_request("invalid signature encoding"))?;

    let key = STANDARD
        .decode(pubkey_b64)
        .map_err(|_| AppError::bad_request("invalid signing key encoding"))?;

    UnparsedPublicKey::new(&signature::ED25519, &key)
        .verify(message, &sig)
        .map_err(|_| AppError::bad_request("signature verification failed"))
}
