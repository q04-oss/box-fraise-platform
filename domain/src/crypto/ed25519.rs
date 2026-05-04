use ed25519_dalek::{Signer, Verifier, SigningKey, VerifyingKey};
use rand::rngs::OsRng;

/// Ed25519 key pair for soultoken signing.
///
/// BFIP reference/cryptography.md Section 4.
pub struct Ed25519KeyPair {
    signing_key: SigningKey,
}

#[allow(missing_docs)]
#[derive(Debug)]
pub enum Ed25519Error {
    InvalidKey(String),
    InvalidSignature(String),
    InvalidHex(String),
}

impl Ed25519KeyPair {
    /// Generate a new random key pair using OsRng.
    ///
    /// For testing only — production keys must be generated externally
    /// and loaded via `from_bytes`.
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        Self { signing_key }
    }

    /// Load from 32 raw bytes (the Ed25519 scalar).
    pub fn from_bytes(bytes: &[u8; 32]) -> Result<Self, Ed25519Error> {
        let signing_key = SigningKey::from_bytes(bytes);
        Ok(Self { signing_key })
    }

    /// Load from hex-encoded 32 bytes.
    pub fn from_hex(hex: &str) -> Result<Self, Ed25519Error> {
        let bytes = hex::decode(hex)
            .map_err(|e| Ed25519Error::InvalidHex(e.to_string()))?;
        let bytes: [u8; 32] = bytes.try_into()
            .map_err(|_| Ed25519Error::InvalidKey(
                "Ed25519 key must be 32 bytes".to_string(),
            ))?;
        Self::from_bytes(&bytes)
    }

    /// Sign a payload — returns hex-encoded signature (128 hex chars).
    ///
    /// The payload is SHA-256 hashed before signing.
    pub fn sign(&self, payload: &[u8]) -> String {
        let hash = sha256_bytes(payload);
        let signature = self.signing_key.sign(&hash);
        hex::encode(signature.to_bytes())
    }

    /// Export the verifying (public) key as hex.
    ///
    /// This is what goes in the trust registry.
    pub fn verifying_key_hex(&self) -> String {
        hex::encode(self.signing_key.verifying_key().to_bytes())
    }

    /// Export the signing key as hex.
    ///
    /// Handle with extreme care — never log or expose this.
    pub fn signing_key_hex(&self) -> String {
        hex::encode(self.signing_key.to_bytes())
    }
}

/// Verify an Ed25519 signature.
///
/// - `verifying_key_hex`: 64 hex chars (32 bytes)
/// - `payload`: the original payload (will be SHA-256 hashed before verification)
/// - `signature_hex`: 128 hex chars (64 bytes)
///
/// Returns `Ok(true)` if valid, `Ok(false)` if invalid,
/// `Err` if the key or signature cannot be decoded.
pub fn verify_ed25519(
    verifying_key_hex: &str,
    payload:           &[u8],
    signature_hex:     &str,
) -> Result<bool, Ed25519Error> {
    let key_bytes = hex::decode(verifying_key_hex)
        .map_err(|e| Ed25519Error::InvalidHex(e.to_string()))?;
    let key_bytes: [u8; 32] = key_bytes.try_into()
        .map_err(|_| Ed25519Error::InvalidKey(
            "Verifying key must be 32 bytes".to_string(),
        ))?;
    let verifying_key = VerifyingKey::from_bytes(&key_bytes)
        .map_err(|e| Ed25519Error::InvalidKey(e.to_string()))?;

    let sig_bytes = hex::decode(signature_hex)
        .map_err(|e| Ed25519Error::InvalidHex(e.to_string()))?;
    let sig_bytes: [u8; 64] = sig_bytes.try_into()
        .map_err(|_| Ed25519Error::InvalidSignature(
            "Signature must be 64 bytes".to_string(),
        ))?;
    let signature = ed25519_dalek::Signature::from_bytes(&sig_bytes);

    let hash = sha256_bytes(payload);
    Ok(verifying_key.verify(&hash, &signature).is_ok())
}

/// Verify that ALL provided signatures are valid against the payload.
///
/// Used for attestation co-sign verification.
/// Returns `Ok(true)` only if every key/signature pair is valid.
pub fn verify_aggregated_ed25519(
    verifying_keys: &[&str],
    payload:        &[u8],
    signatures:     &[&str],
) -> Result<bool, Ed25519Error> {
    if verifying_keys.len() != signatures.len() {
        return Ok(false);
    }
    if verifying_keys.is_empty() {
        return Ok(false);
    }
    for (key_hex, sig_hex) in verifying_keys.iter().zip(signatures.iter()) {
        if !verify_ed25519(key_hex, payload, sig_hex)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// SHA-256 hash of raw bytes.
///
/// Internal helper — payload is hashed before signing/verification.
fn sha256_bytes(input: &[u8]) -> [u8; 32] {
    use ring::digest;
    let digest = digest::digest(&digest::SHA256, input);
    let mut output = [0u8; 32];
    output.copy_from_slice(digest.as_ref());
    output
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_and_sign_verifies_correctly() {
        let kp      = Ed25519KeyPair::generate();
        let payload = b"bfip test payload";
        let sig     = kp.sign(payload);
        let result  = verify_ed25519(&kp.verifying_key_hex(), payload, &sig).unwrap();
        assert!(result, "valid signature must verify as Ok(true)");
    }

    #[test]
    fn tampered_payload_fails_verification() {
        let kp  = Ed25519KeyPair::generate();
        let sig = kp.sign(b"hello world");
        let result = verify_ed25519(&kp.verifying_key_hex(), b"hello worlD", &sig).unwrap();
        assert!(!result, "tampered payload must not verify");
    }

    #[test]
    fn wrong_key_fails_verification() {
        let kp_a = Ed25519KeyPair::generate();
        let kp_b = Ed25519KeyPair::generate();
        let sig  = kp_a.sign(b"payload");
        let result = verify_ed25519(&kp_b.verifying_key_hex(), b"payload", &sig).unwrap();
        assert!(!result, "signature from key A must not verify with key B");
    }

    #[test]
    fn invalid_key_hex_returns_error() {
        let kp  = Ed25519KeyPair::generate();
        let sig = kp.sign(b"payload");
        let err = verify_ed25519("not hex", b"payload", &sig).unwrap_err();
        assert!(matches!(err, Ed25519Error::InvalidHex(_)));
    }

    #[test]
    fn invalid_signature_hex_returns_error() {
        let kp  = Ed25519KeyPair::generate();
        let err = verify_ed25519(&kp.verifying_key_hex(), b"payload", "not hex").unwrap_err();
        assert!(matches!(err, Ed25519Error::InvalidHex(_)));
    }

    #[test]
    fn wrong_length_key_returns_error() {
        // 31 bytes = 62 hex chars.
        let short_key = "ab".repeat(31);
        let kp  = Ed25519KeyPair::generate();
        let sig = kp.sign(b"payload");
        let err = verify_ed25519(&short_key, b"payload", &sig).unwrap_err();
        assert!(matches!(err, Ed25519Error::InvalidKey(_)));
    }

    #[test]
    fn from_hex_round_trips_correctly() {
        let kp1     = Ed25519KeyPair::generate();
        let hex     = kp1.signing_key_hex();
        let kp2     = Ed25519KeyPair::from_hex(&hex).expect("from_hex must succeed");
        let payload = b"deterministic test";
        assert_eq!(kp1.sign(payload), kp2.sign(payload),
            "key reloaded from hex must produce identical signatures");
    }

    #[test]
    fn aggregated_verify_succeeds_when_all_valid() {
        let kp_a = Ed25519KeyPair::generate();
        let kp_b = Ed25519KeyPair::generate();
        let payload = b"co-sign payload";
        let sig_a = kp_a.sign(payload);
        let sig_b = kp_b.sign(payload);

        let result = verify_aggregated_ed25519(
            &[kp_a.verifying_key_hex().as_str(), kp_b.verifying_key_hex().as_str()],
            payload,
            &[sig_a.as_str(), sig_b.as_str()],
        ).unwrap();
        assert!(result, "both valid signatures must aggregate to Ok(true)");
    }

    #[test]
    fn aggregated_verify_fails_when_one_invalid() {
        let kp_a    = Ed25519KeyPair::generate();
        let kp_b    = Ed25519KeyPair::generate();
        let payload = b"the real payload";
        let sig_a   = kp_a.sign(payload);
        let sig_b   = kp_b.sign(b"a different payload");

        let result = verify_aggregated_ed25519(
            &[kp_a.verifying_key_hex().as_str(), kp_b.verifying_key_hex().as_str()],
            payload,
            &[sig_a.as_str(), sig_b.as_str()],
        ).unwrap();
        assert!(!result, "mismatched signature must cause aggregate to return false");
    }

    #[test]
    fn aggregated_verify_fails_with_empty_inputs() {
        let result = verify_aggregated_ed25519(&[], b"payload", &[]).unwrap();
        assert!(!result, "empty inputs must return false");
    }

    #[test]
    fn aggregated_verify_fails_with_mismatched_counts() {
        let kp_a = Ed25519KeyPair::generate();
        let kp_b = Ed25519KeyPair::generate();
        let payload = b"payload";
        let sig_a = kp_a.sign(payload);

        let result = verify_aggregated_ed25519(
            &[kp_a.verifying_key_hex().as_str(), kp_b.verifying_key_hex().as_str()],
            payload,
            &[sig_a.as_str()],
        ).unwrap();
        assert!(!result, "mismatched key/sig counts must return false");
    }

    #[test]
    fn sign_is_deterministic_for_same_key_and_payload() {
        let kp      = Ed25519KeyPair::generate();
        let payload = b"determinism check";
        let sig1    = kp.sign(payload);
        let sig2    = kp.sign(payload);
        assert_eq!(sig1, sig2,
            "Ed25519 deterministic signing must produce identical signatures for same key+payload");
    }

    #[test]
    fn verifying_key_hex_is_64_chars() {
        let kp = Ed25519KeyPair::generate();
        assert_eq!(kp.verifying_key_hex().len(), 64,
            "verifying key hex must be 64 chars (32 bytes)");
    }

    #[test]
    fn signing_key_hex_is_64_chars() {
        let kp = Ed25519KeyPair::generate();
        assert_eq!(kp.signing_key_hex().len(), 64,
            "signing key hex must be 64 chars (32 bytes)");
    }
}
