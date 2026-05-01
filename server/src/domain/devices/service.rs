use base64::{engine::general_purpose::STANDARD, Engine};

use crate::{
    app::AppState,
    error::{AppError, AppResult},
    types::UserId,
};
use super::repository;

// ── Role management ───────────────────────────────────────────────────────────

const VALID_ROLES: &[&str] = &["user", "employee", "chocolatier"];

pub async fn update_role(
    state:           &AppState,
    requesting_user: UserId,
    address:         &str,
    new_role:        &str,
) -> AppResult<()> {
    if !VALID_ROLES.contains(&new_role) {
        return Err(AppError::bad_request("invalid role"));
    }

    // Only admins (is_dorotka) may elevate a device beyond 'user'.
    if new_role != "user" {
        let is_admin: bool = sqlx::query_scalar(
            "SELECT is_dorotka FROM users WHERE id = $1",
        )
        .bind(requesting_user)
        .fetch_optional(&state.db)
        .await
        .map_err(AppError::Db)?
        .unwrap_or(false);

        if !is_admin {
            return Err(AppError::Forbidden);
        }
    }

    repository::set_role(&state.db, address, new_role).await
}

// ── App Attest ────────────────────────────────────────────────────────────────

pub async fn store_attestation(
    state:        &AppState,
    user_id:      UserId,
    key_id:       &str,
    attestation:  &str,
    hmac_key_b64: &str,
    challenge:    Option<&str>,
) -> AppResult<()> {
    // Validate HMAC key: must be exactly 32 bytes (256-bit).
    let key_bytes = STANDARD
        .decode(hmac_key_b64)
        .map_err(|_| AppError::bad_request("hmac_key must be base64"))?;
    if key_bytes.len() != 32 {
        return Err(AppError::bad_request("hmac_key must be 32 bytes"));
    }

    // If a challenge was provided, consume it from the DB — verifies it was server-issued.
    if let Some(ch) = challenge {
        let consumed = repository::consume_attest_challenge(&state.db, ch).await?;
        if !consumed {
            return Err(AppError::bad_request(
                "invalid or expired challenge — call GET /api/devices/attest-challenge first",
            ));
        }
    }

    // Parse the attestation object and extract the leaf certificate's public key.
    // This verifies the authData rpIdHash matches our App ID, and extracts the
    // EC public key for future per-request assertion verification.
    let rp_id = state.cfg.apple_client_id.as_deref().unwrap_or("com.boxfraise.app");
    let challenge_bytes = challenge.and_then(|c| base64::engine::general_purpose::STANDARD.decode(c).ok());
    let attest_data = crate::auth::apple_attest::parse_attestation(
        attestation,
        key_id,
        challenge_bytes.as_deref(),
        rp_id,
    )?;

    let public_key_b64 = STANDARD.encode(&attest_data.public_key_der);

    repository::upsert_attestation(
        &state.db, user_id, key_id, attestation, hmac_key_b64, &public_key_b64,
    )
    .await
}
