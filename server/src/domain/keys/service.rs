use base64::{engine::general_purpose::STANDARD, Engine};
use ring::signature::{self, UnparsedPublicKey};

use crate::{
    app::AppState,
    error::{AppError, AppResult},
};
use super::{
    repository,
    types::{KeyBundleResponse, OtpkResponse, RegisterKeysBody},
};

// ── Challenge ─────────────────────────────────────────────────────────────────

pub async fn issue_challenge(state: &AppState, user_id: i32) -> AppResult<String> {
    repository::create_challenge(&state.db, user_id).await
}

// ── Key registration ──────────────────────────────────────────────────────────

pub async fn register_keys(
    state:   &AppState,
    user_id: i32,
    body:    RegisterKeysBody,
) -> AppResult<()> {
    // Verify proof-of-possession if the client provided a signing key + signature.
    // Degrade gracefully for older clients that don't support PoP yet.
    if let Some(signing_key_b64) = &body.identity_signing_key {
        if let Some(sig_b64) = &body.challenge_sig {
            let challenge = repository::consume_challenge(&state.db, user_id)
                .await?
                .ok_or_else(|| AppError::bad_request("no valid challenge found"))?;

            verify_ed25519(challenge.as_bytes(), sig_b64, signing_key_b64)?;
        } else {
            tracing::warn!(
                user_id,
                "identity_signing_key provided without challenge_sig — PoP skipped"
            );
        }
    }

    // Persist key material.
    repository::upsert_user_keys(
        &state.db,
        user_id,
        &body.identity_key,
        body.identity_signing_key.as_deref(),
        &body.signed_pre_key,
        &body.signed_pre_key_sig,
    )
    .await?;

    // Persist any accompanying one-time pre-keys.
    if !body.one_time_pre_keys.is_empty() {
        let pairs: Vec<(i32, String)> = body
            .one_time_pre_keys
            .into_iter()
            .map(|k| (k.key_id, k.public_key))
            .collect();
        repository::insert_otpks(&state.db, user_id, &pairs).await?;
    }

    Ok(())
}

// ── OPK management ────────────────────────────────────────────────────────────

pub async fn upload_otpks(
    state:   &AppState,
    user_id: i32,
    keys:    Vec<(i32, String)>,
) -> AppResult<()> {
    repository::insert_otpks(&state.db, user_id, &keys).await
}

pub async fn otpk_count(state: &AppState, user_id: i32) -> AppResult<i64> {
    repository::count_otpks(&state.db, user_id).await
}

// ── Key bundle (X3DH) ─────────────────────────────────────────────────────────

pub async fn fetch_bundle(state: &AppState, target_id: i32) -> AppResult<KeyBundleResponse> {
    let keys = repository::find_user_keys(&state.db, target_id)
        .await?
        .ok_or(AppError::NotFound)?;

    let otpk = repository::claim_otpk(&state.db, target_id).await?.map(|r| OtpkResponse {
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

pub async fn fetch_bundle_by_code(
    state: &AppState,
    code:  &str,
) -> AppResult<KeyBundleResponse> {
    let target_id = repository::user_id_by_code(&state.db, code)
        .await?
        .ok_or(AppError::NotFound)?;

    fetch_bundle(state, target_id).await
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Verify an Ed25519 signature.
/// `message` is the raw bytes being verified (the challenge string as UTF-8).
/// `sig_b64` and `pubkey_b64` are standard base64.
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
