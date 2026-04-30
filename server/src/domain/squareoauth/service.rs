/// Square OAuth service — connect URL generation, callback handling, token refresh.
///
/// # CSRF protection
///
/// The OAuth state parameter is a cryptographically random UUID stored in Redis
/// with a 10-minute TTL. The callback atomically reads and deletes it (GETDEL)
/// before processing — a replayed or forged state string is rejected.
///
/// Redis is required for the OAuth flow. If Redis is not configured,
/// `connect_url()` returns a 503 so the flow cannot be initiated.
///
/// # Token refresh
///
/// `load_decrypted()` is the call site for other domains that need a live
/// Square access token. It checks expiry and refreshes transparently when
/// the token is within 24 hours of expiry, writing the new token back to the
/// DB and emitting an audit event.
use chrono::Utc;
use deadpool_redis::redis;
use secrecy::ExposeSecret;
use uuid::Uuid;

use crate::{
    audit,
    app::AppState,
    crypto,
    error::{AppError, AppResult},
    integrations::square::{ApiClient, OAuthClient},
};
use super::repository::{self, EncryptedTokenRow};

const STATE_PREFIX: &str = "fraise:square-oauth-state:";
const STATE_TTL_SECS: u64 = 600; // 10 minutes
const REFRESH_THRESHOLD_HOURS: i64 = 24;

/// Decrypted token data, usable for API calls.
/// Never stored, never logged — constructed on the fly and dropped after use.
pub struct LiveTokens {
    pub access_token:      String,
    pub square_location_id: String,
    pub merchant_id:       String,
}

// ── Connect ───────────────────────────────────────────────────────────────────

/// Builds the Square OAuth authorization URL and stores the CSRF state token
/// in Redis. Returns Err if Square is not configured or Redis is unavailable.
pub async fn connect_url(
    state:       &AppState,
    business_id: i32,
) -> AppResult<String> {
    let app_id = state.cfg.square_app_id.as_deref()
        .ok_or_else(|| AppError::bad_request("Square integration is not configured"))?;

    let redis_pool = state.redis.as_ref()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!(
            "Redis is required for Square OAuth CSRF protection — set REDIS_URL"
        )))?;

    // State token encodes the business_id so the callback knows which business
    // is completing the flow even if the user agent changes between requests.
    let state_token = Uuid::new_v4().to_string();
    let redis_key   = format!("{STATE_PREFIX}{state_token}");
    let redis_value = business_id.to_string();

    let mut conn = redis_pool.get().await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis pool error: {e}")))?;

    let result: redis::Value = redis::cmd("SET")
        .arg(&redis_key)
        .arg(&redis_value)
        .arg("EX")
        .arg(STATE_TTL_SECS)
        .arg("NX")
        .query_async(&mut *conn)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis SET state: {e}")))?;

    // NX ensures uniqueness — Nil means a collision (astronomically unlikely).
    if matches!(result, redis::Value::Nil) {
        return Err(AppError::Internal(anyhow::anyhow!("State token collision — retry")));
    }

    let url = format!(
        "https://connect.squareup.com/oauth2/authorize\
         ?client_id={app_id}\
         &scope=ORDERS_WRITE+PAYMENTS_READ+CUSTOMERS_READ+CUSTOMERS_WRITE\
         &session=false\
         &state={state_token}"
    );

    Ok(url)
}

// ── Callback ──────────────────────────────────────────────────────────────────

/// Handles the OAuth callback: validates CSRF state, exchanges the code for
/// tokens, resolves the merchant's active location, encrypts and stores them,
/// writes audit event.
///
/// `square_base` is the Square API base URL. Production always passes
/// `crate::integrations::square::BASE`. Tests pass a mock server URL.
pub async fn handle_callback(
    state:       &AppState,
    code:        &str,
    state_token: &str,
    ip:          Option<std::net::IpAddr>,
    square_base: &str,
) -> AppResult<i32> {
    // ── 1. Validate and consume CSRF state ────────────────────────────────────
    let business_id = consume_state(state, state_token).await?;

    // ── 2. Exchange code for tokens ───────────────────────────────────────────
    let app_id       = state.cfg.square_app_id.as_deref()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("SQUARE_APP_ID missing")))?;
    let app_secret   = state.cfg.square_app_secret.as_ref()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("SQUARE_APP_SECRET missing")))?;
    let redirect_url = state.cfg.square_oauth_redirect_url.as_deref()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("SQUARE_OAUTH_REDIRECT_URL missing")))?;

    let client = OAuthClient::new_with_base(
        app_id, app_secret.expose_secret(), redirect_url, &state.http, square_base,
    );
    let tokens = client.exchange_code(code).await?;

    // ── 3. Resolve the merchant's primary Square location ─────────────────────
    // GET /v2/locations using the new access token — take the first ACTIVE one.
    // If Square returns no active locations, fail with 502 so the operator
    // knows their connection didn't fully complete and what to fix.
    let api_client = ApiClient::new_with_base(&tokens.access_token, &state.http, square_base);
    let square_location_id = api_client.get_first_active_location().await?;

    // ── 4. Encrypt and store ──────────────────────────────────────────────────
    let enc_key = encryption_key(state)?;
    let row = EncryptedTokenRow {
        encrypted_access_token:  crypto::encrypt(enc_key, &tokens.access_token)?,
        encrypted_refresh_token: crypto::encrypt(enc_key, &tokens.refresh_token)?,
        merchant_id:             tokens.merchant_id.clone(),
        square_location_id:      square_location_id.to_string(),
        expires_at:              tokens.expires_at,
    };
    repository::upsert_tokens(&state.db, business_id, &row).await?;

    // ── 5. Audit ──────────────────────────────────────────────────────────────
    audit::write(
        &state.db,
        None,
        Some(business_id),
        "square_oauth.connected",
        serde_json::json!({ "merchant_id": tokens.merchant_id }),
        ip,
    ).await;

    Ok(business_id)
}

// ── Load with transparent refresh ────────────────────────────────────────────

/// Returns live, decrypted tokens for a business, refreshing them if within
/// 24 hours of expiry. Called by any domain that needs to call the Square API.
///
/// Returns Err if the business has not connected Square.
pub async fn load_decrypted(
    state:       &AppState,
    business_id: i32,
) -> AppResult<LiveTokens> {
    let enc_key = encryption_key(state)?;

    let stored = repository::load_tokens(&state.db, business_id)
        .await?
        .ok_or_else(|| AppError::bad_request("this business has not connected Square"))?;

    let access_token  = crypto::decrypt(enc_key, &stored.encrypted_access_token)?;
    let refresh_token = crypto::decrypt(enc_key, &stored.encrypted_refresh_token)?;

    // Refresh if the token expires within the threshold.
    let hours_remaining = stored.expires_at.signed_duration_since(Utc::now()).num_hours();
    if hours_remaining < REFRESH_THRESHOLD_HOURS {
        let access_token = refresh_token_for_business(
            state, business_id, &refresh_token, &stored.merchant_id, &stored.square_location_id,
        ).await?;
        return Ok(LiveTokens {
            access_token,
            square_location_id: stored.square_location_id,
            merchant_id:        stored.merchant_id,
        });
    }

    Ok(LiveTokens {
        access_token,
        square_location_id: stored.square_location_id,
        merchant_id:        stored.merchant_id,
    })
}

// ── Internal helpers ──────────────────────────────────────────────────────────

async fn consume_state(state: &AppState, state_token: &str) -> AppResult<i32> {
    let redis_pool = state.redis.as_ref()
        .ok_or(AppError::Unauthorized)?;

    let key = format!("{STATE_PREFIX}{state_token}");
    let mut conn = redis_pool.get().await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis pool error: {e}")))?;

    let value: Option<String> = redis::cmd("GETDEL")
        .arg(&key)
        .query_async(&mut *conn)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis GETDEL: {e}")))?;

    let business_id_str = value.ok_or(AppError::Unauthorized)?; // expired or forged
    business_id_str.parse::<i32>()
        .map_err(|_| AppError::Internal(anyhow::anyhow!("invalid business_id in OAuth state")))
}

async fn refresh_token_for_business(
    state:             &AppState,
    business_id:       i32,
    refresh_token:     &str,
    merchant_id:       &str,
    square_location_id: &str,
) -> AppResult<String> {
    let app_id       = state.cfg.square_app_id.as_deref()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("SQUARE_APP_ID missing")))?;
    let app_secret   = state.cfg.square_app_secret.as_ref()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("SQUARE_APP_SECRET missing")))?;
    let redirect_url = state.cfg.square_oauth_redirect_url.as_deref().unwrap_or("");

    let client = OAuthClient::new(app_id, app_secret.expose_secret(), redirect_url, &state.http);
    let tokens = client.refresh(refresh_token).await?;

    let enc_key = encryption_key(state)?;
    let row = EncryptedTokenRow {
        encrypted_access_token:  crypto::encrypt(enc_key, &tokens.access_token)?,
        encrypted_refresh_token: crypto::encrypt(enc_key, &tokens.refresh_token)?,
        merchant_id:             merchant_id.to_string(),
        square_location_id:      square_location_id.to_string(),
        expires_at:              tokens.expires_at,
    };
    repository::upsert_tokens(&state.db, business_id, &row).await?;

    audit::write(
        &state.db,
        None,
        Some(business_id),
        "square_oauth.token_refreshed",
        serde_json::json!({ "merchant_id": merchant_id }),
        None,
    ).await;

    Ok(tokens.access_token)
}

fn encryption_key(state: &AppState) -> AppResult<&str> {
    state.cfg.square_token_encryption_key
        .as_ref()
        .map(|s| s.expose_secret().as_str())
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!(
            "SQUARE_TOKEN_ENCRYPTION_KEY not set — Square token storage is unavailable"
        )))
}
