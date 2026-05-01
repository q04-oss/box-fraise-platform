/// Apple Sign In — server-side identity token verification.
///
/// Flow:
///   1. Client authenticates with Apple and receives an `identityToken` (RS256 JWT).
///   2. We decode the header to extract the `kid`.
///   3. We fetch Apple's JWKS endpoint to find the matching public key.
///   4. We verify the JWT signature and standard claims (iss, aud, exp).
use jsonwebtoken::{
    decode, decode_header,
    jwk::{Jwk, JwkSet},
    Algorithm, DecodingKey, Validation,
};
use serde::Deserialize;

use crate::{config::Config, error::{DomainError, AppResult}};

const APPLE_JWKS_URL: &str = "https://appleid.apple.com/auth/keys";
const APPLE_ISSUER:   &str = "https://appleid.apple.com";

#[derive(Debug, Deserialize)]
pub struct AppleClaims {
    /// Stable, unique user identifier — store as `apple_user_id`.
    pub sub:   String,
    pub email: Option<String>,
}

pub async fn verify_identity_token(
    token:  &str,
    cfg:    &Config,
    client: &reqwest::Client,
) -> AppResult<AppleClaims> {
    // Audience validation is required. Reject early rather than silently skip it.
    let audience = cfg
        .apple_client_id
        .as_deref()
        .ok_or_else(|| DomainError::Internal(anyhow::anyhow!(
            "APPLE_CLIENT_ID is required for Apple Sign In"
        )))?;

    // Decode header only (no signature check yet) to identify which key Apple used.
    let header = decode_header(token)
        .map_err(|_| DomainError::invalid_input("malformed Apple identity token"))?;

    let kid = header.kid
        .ok_or_else(|| DomainError::invalid_input("Apple token missing kid"))?;

    // Fetch Apple's current public key set.
    let jwks: JwkSet = client
        .get(APPLE_JWKS_URL)
        .send()
        .await
        .map_err(|e| DomainError::Internal(anyhow::anyhow!("Apple JWKS fetch failed: {e}")))?
        .json()
        .await
        .map_err(|e| DomainError::Internal(anyhow::anyhow!("Apple JWKS parse failed: {e}")))?;

    let jwk: &Jwk = jwks
        .find(&kid)
        .ok_or_else(|| DomainError::invalid_input("Apple signing key not found"))?;

    let decoding_key = DecodingKey::from_jwk(jwk)
        .map_err(|e| DomainError::Internal(anyhow::anyhow!("Apple key construction failed: {e}")))?;

    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&[APPLE_ISSUER]);
    validation.set_audience(&[audience]);

    let data = decode::<AppleClaims>(token, &decoding_key, &validation)
        .map_err(|_| DomainError::Unauthorized)?;

    Ok(data.claims)
}
