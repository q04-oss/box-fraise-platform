/// Apple Sign In — server-side identity token verification.
///
/// Flow:
///   1. Client authenticates with Apple and receives an `identityToken` (RS256 JWT).
///   2. We decode the header to extract the `kid`.
///   3. We fetch Apple's JWKS endpoint to get the matching public key.
///   4. We verify the JWT signature and standard claims (iss, aud, exp).
use jsonwebtoken::{
    decode, decode_header,
    jwk::{Jwk, JwkSet},
    Algorithm, DecodingKey, Validation,
};
use serde::{Deserialize, Serialize};

use crate::{config::Config, error::AppError};

const APPLE_JWKS_URL: &str = "https://appleid.apple.com/auth/keys";
const APPLE_ISSUER:   &str = "https://appleid.apple.com";

#[derive(Debug, Deserialize)]
pub struct AppleClaims {
    pub sub:   String,  // unique stable user ID — store as apple_user_id
    pub email: Option<String>,
    pub iss:   String,
    pub aud:   serde_json::Value, // may be a string or array depending on client
    pub exp:   usize,
}

pub async fn verify_identity_token(
    token:  &str,
    cfg:    &Config,
    client: &reqwest::Client,
) -> Result<AppleClaims, AppError> {
    // Decode header only (no signature check yet) to find which key Apple used.
    let header = decode_header(token)
        .map_err(|_| AppError::bad_request("malformed Apple identity token"))?;

    let kid = header
        .kid
        .ok_or_else(|| AppError::bad_request("Apple token missing kid"))?;

    // Fetch Apple's current public key set.
    let jwks: JwkSet = client
        .get(APPLE_JWKS_URL)
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Apple JWKS fetch failed: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Apple JWKS parse failed: {e}")))?;

    let jwk: &Jwk = jwks
        .find(&kid)
        .ok_or_else(|| AppError::bad_request("Apple signing key not found"))?;

    let decoding_key = DecodingKey::from_jwk(jwk)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Apple key construction failed: {e}")))?;

    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&[APPLE_ISSUER]);

    // Audience is the Apple Services ID (web) or bundle ID (native).
    if let Some(aud) = &cfg.apple_client_id {
        validation.set_audience(&[aud.as_str()]);
    } else {
        validation.validate_aud = false;
    }

    let data = decode::<AppleClaims>(token, &decoding_key, &validation)
        .map_err(|e| AppError::Unauthorized)?;

    Ok(data.claims)
}

/// Resolve the display email for an Apple user.
/// Apple private-relay addresses (privaterelay.appleid.com) are valid —
/// keep them as-is; the platform sends mail through Resend which handles relay.
pub fn resolve_email(email: Option<String>) -> Option<String> {
    email
}
