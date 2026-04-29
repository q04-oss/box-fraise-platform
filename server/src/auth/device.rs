/// Cardputer device authentication — EIP-191 personal_sign verification.
///
/// The device signs the current Unix minute (as a decimal string) with its
/// Ethereum private key. The server accepts signatures within a ±1 minute
/// window to accommodate clock skew.
///
/// Header format:  Authorization: Fraise <address>:<signature>
use k256::ecdsa::{RecoveryId, Signature};
use sha3::{Digest, Keccak256};

use crate::error::AppError;

pub struct DeviceHeader {
    pub address:   String,
    pub signature: String,
}

/// Parse `Fraise <address>:<signature>` from the raw Authorization header value.
pub fn parse_auth_header(value: &str) -> Option<DeviceHeader> {
    let rest = value.strip_prefix("Fraise ")?;
    let (address, signature) = rest.split_once(':')?;
    Some(DeviceHeader {
        address:   address.to_owned(),
        signature: signature.to_owned(),
    })
}

/// Verify the signature and return the recovered Ethereum address (lowercase, 0x-prefixed).
/// Returns `Err` if the signature is invalid or does not match any accepted minute.
pub fn verify_signature(header: &DeviceHeader) -> Result<String, AppError> {
    let now_minute = unix_minute_now();

    // Accept current minute and ±1 to handle clock skew.
    for candidate in [now_minute - 1, now_minute, now_minute + 1] {
        let message = candidate.to_string();
        if let Ok(addr) = recover_address(&message, &header.signature) {
            if addr.eq_ignore_ascii_case(&header.address) {
                return Ok(addr);
            }
        }
    }

    Err(AppError::Unauthorized)
}

// ── Internal helpers ─────────────────────────────────────────────────────────

fn unix_minute_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        / 60
}

/// EIP-191 personal_sign: recover the signer's Ethereum address from a message + signature.
fn recover_address(message: &str, signature_hex: &str) -> anyhow::Result<String> {
    // 1. Construct the Ethereum signed message prefix.
    let prefixed = format!("\x19Ethereum Signed Message:\n{}{}", message.len(), message);

    // 2. Keccak-256 hash.
    let hash: [u8; 32] = Keccak256::digest(prefixed.as_bytes()).into();

    // 3. Decode signature bytes (65 bytes: r[32] + s[32] + v[1]).
    let sig_bytes = hex::decode(signature_hex.trim_start_matches("0x"))?;
    anyhow::ensure!(sig_bytes.len() == 65, "signature must be 65 bytes");

    let v = sig_bytes[64];
    // v = 27 or 28 in legacy personal_sign; map to recovery_id 0 or 1.
    let recovery_id = RecoveryId::from_byte(v.wrapping_sub(27) & 1)
        .ok_or_else(|| anyhow::anyhow!("invalid recovery id"))?;

    let signature = Signature::from_slice(&sig_bytes[..64])?;

    // 4. Recover the public key.
    let verifying_key =
        k256::ecdsa::VerifyingKey::recover_from_prehash(&hash, &signature, recovery_id)?;

    // 5. Derive the Ethereum address (keccak256 of uncompressed pubkey, last 20 bytes).
    let encoded = verifying_key.to_encoded_point(false);
    let pubkey_bytes = &encoded.as_bytes()[1..]; // strip 0x04 uncompressed prefix

    let address_bytes: [u8; 32] = Keccak256::digest(pubkey_bytes).into();
    let address = hex::encode(&address_bytes[12..]); // last 20 bytes

    Ok(format!("0x{address}"))
}
