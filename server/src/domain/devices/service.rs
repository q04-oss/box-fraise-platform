use base64::{engine::general_purpose::STANDARD, Engine};

use crate::{
    app::AppState,
    error::{AppError, AppResult},
    types::UserId,
};
use super::{repository, types::DeviceRow};

// ── Pairing ───────────────────────────────────────────────────────────────────

pub async fn create_pair_token(state: &AppState, user_id: UserId) -> AppResult<String> {
    repository::create_pair_token(&state.db, user_id).await
}

// ── Registration ──────────────────────────────────────────────────────────────

pub async fn register_device(
    state:          &AppState,
    device_address: &str,
    signature:      &str,
    pairing_token:  &str,
) -> AppResult<DeviceRow> {
    // Verify the EIP-191 signature over the pairing token.
    // This proves the device controls the private key for device_address.
    let header = crate::auth::device::DeviceHeader {
        address:   device_address.to_owned(),
        signature: signature.to_owned(),
    };

    // We verify against the pairing token rather than the current minute here,
    // since the token already provides replay protection (single-use).
    verify_pairing_signature(&header, pairing_token)?;

    // Consume the pairing token — links device to the token's owner.
    let user_id = repository::consume_pair_token(&state.db, pairing_token)
        .await?
        .ok_or_else(|| AppError::bad_request("invalid or expired pairing token"))?;

    repository::insert_device(&state.db, user_id, device_address).await
}

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

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Verify that the device signed the pairing token with its Ethereum key.
/// This is distinct from the time-based auth used in `auth::device` because
/// the message is the token itself, not the current minute.
fn verify_pairing_signature(
    header:        &crate::auth::device::DeviceHeader,
    pairing_token: &str,
) -> AppResult<()> {
    use k256::ecdsa::{RecoveryId, Signature};
    use sha3::{Digest, Keccak256};

    let prefixed = format!(
        "\x19Ethereum Signed Message:\n{}{}",
        pairing_token.len(),
        pairing_token
    );
    let hash: [u8; 32] = Keccak256::digest(prefixed.as_bytes()).into();

    let sig_bytes = hex::decode(header.signature.trim_start_matches("0x"))
        .map_err(|_| AppError::bad_request("invalid signature encoding"))?;

    if sig_bytes.len() != 65 {
        return Err(AppError::bad_request("signature must be 65 bytes"));
    }

    let v = sig_bytes[64];
    let recovery_id = RecoveryId::from_byte(v.wrapping_sub(27) & 1)
        .ok_or_else(|| AppError::bad_request("invalid recovery id"))?;

    let signature = Signature::from_slice(&sig_bytes[..64])
        .map_err(|_| AppError::bad_request("invalid signature"))?;

    let key = k256::ecdsa::VerifyingKey::recover_from_prehash(&hash, &signature, recovery_id)
        .map_err(|_| AppError::Unauthorized)?;

    let encoded = key.to_encoded_point(false);
    let addr_hash: [u8; 32] = Keccak256::digest(&encoded.as_bytes()[1..]).into();
    let recovered = format!("0x{}", hex::encode(&addr_hash[12..]));

    if !recovered.eq_ignore_ascii_case(&header.address) {
        return Err(AppError::Unauthorized);
    }

    Ok(())
}
