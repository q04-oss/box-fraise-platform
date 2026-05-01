/// Application-layer encryption for sensitive values stored in the database.
///
/// # Why application-layer rather than database-level?
///
/// Database-level encryption (Postgres TDE, pgcrypto) is transparent to the
/// application — a compromised application credential still reads plaintext.
/// Application-layer encryption means the DB only ever holds ciphertext: a DB
/// credential leak without the `SQUARE_TOKEN_ENCRYPTION_KEY` yields nothing
/// usable. The tradeoff is that the application must manage the key lifecycle,
/// but for a platform where Square OAuth tokens authorise financial operations
/// this is the correct tradeoff.
///
/// # Format
///
/// Ciphertext is stored as a hex string: `{nonce_hex}{ciphertext_hex}`.
/// The nonce is 12 bytes (96 bits), randomly generated per encryption.
/// The tag (16 bytes) is appended by AES-GCM and included in ciphertext_hex.
/// Total overhead: 24 hex chars (nonce) + 32 hex chars (tag) = 56 chars per value.
///
/// # Key format
///
/// `SQUARE_TOKEN_ENCRYPTION_KEY` must be exactly 64 hex characters (32 bytes).
/// Generate with: `openssl rand -hex 32`
use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use secrecy::{ExposeSecret, SecretString};

use crate::error::{DomainError, AppResult};

/// Encrypts `plaintext` with AES-256-GCM using the provided hex key.
/// Returns a hex string suitable for DB storage.
pub fn encrypt(key_hex: &str, plaintext: &str) -> AppResult<String> {
    let key_bytes = hex::decode(key_hex)
        .map_err(|_| DomainError::Internal(anyhow::anyhow!("invalid encryption key format")))?;
    if key_bytes.len() != 32 {
        return Err(DomainError::Internal(anyhow::anyhow!(
            "SQUARE_TOKEN_ENCRYPTION_KEY must be 32 bytes (64 hex chars)"
        )));
    }

    let cipher = Aes256Gcm::new_from_slice(&key_bytes)
        .map_err(|e| DomainError::Internal(anyhow::anyhow!("cipher init: {e}")))?;
    let nonce  = Aes256Gcm::generate_nonce(&mut OsRng);

    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|e| DomainError::Internal(anyhow::anyhow!("encryption failed: {e}")))?;

    Ok(format!("{}{}", hex::encode(nonce), hex::encode(ciphertext)))
}

/// Decrypts a value produced by `encrypt`. Returns the plaintext.
pub fn decrypt(key_hex: &str, stored: &str) -> AppResult<String> {
    let key_bytes = hex::decode(key_hex)
        .map_err(|_| DomainError::Internal(anyhow::anyhow!("invalid encryption key format")))?;
    if key_bytes.len() != 32 {
        return Err(DomainError::Internal(anyhow::anyhow!(
            "SQUARE_TOKEN_ENCRYPTION_KEY must be 32 bytes (64 hex chars)"
        )));
    }

    // Nonce is the first 24 hex chars (12 bytes).
    if stored.len() < 24 {
        return Err(DomainError::Internal(anyhow::anyhow!("ciphertext too short")));
    }
    let (nonce_hex, ct_hex) = stored.split_at(24);
    let nonce_bytes = hex::decode(nonce_hex)
        .map_err(|_| DomainError::Internal(anyhow::anyhow!("invalid nonce in stored value")))?;
    let ct = hex::decode(ct_hex)
        .map_err(|_| DomainError::Internal(anyhow::anyhow!("invalid ciphertext in stored value")))?;

    let cipher = Aes256Gcm::new_from_slice(&key_bytes)
        .map_err(|e| DomainError::Internal(anyhow::anyhow!("cipher init: {e}")))?;
    let nonce = Nonce::from_slice(&nonce_bytes);

    let plaintext = cipher
        .decrypt(nonce, ct.as_ref())
        .map_err(|_| DomainError::Internal(anyhow::anyhow!("decryption failed — wrong key or corrupted ciphertext")))?;

    String::from_utf8(plaintext)
        .map_err(|e| DomainError::Internal(anyhow::anyhow!("decrypted value is not valid UTF-8: {e}")))
}

/// Encrypts a `SecretString`. The secret is exposed only within this function
/// and immediately dropped after encryption — it never touches a log or error.
pub fn encrypt_secret(key_hex: &str, secret: &SecretString) -> AppResult<String> {
    encrypt(key_hex, secret.expose_secret())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> String {
        // 32 bytes of zeros — for tests only.
        "0".repeat(64)
    }

    #[test]
    fn round_trip() {
        let key      = test_key();
        let original = "sq0atp-supersecrettoken";
        let stored   = encrypt(&key, original).unwrap();
        let recovered = decrypt(&key, &stored).unwrap();
        assert_eq!(original, recovered);
    }

    #[test]
    fn different_nonce_each_call() {
        let key = test_key();
        let a = encrypt(&key, "token").unwrap();
        let b = encrypt(&key, "token").unwrap();
        // Same plaintext → different nonces → different ciphertext.
        assert_ne!(a, b);
    }

    #[test]
    fn wrong_key_fails() {
        let key1 = test_key();
        let key2 = "1".repeat(64);
        let stored = encrypt(&key1, "token").unwrap();
        assert!(decrypt(&key2, &stored).is_err());
    }

    #[test]
    fn short_key_rejected() {
        assert!(encrypt("deadbeef", "token").is_err());
    }
}
