/// Square API integration — OAuth token exchange and POS order creation.
///
/// # Two distinct client types
///
/// `OAuthClient` uses the platform's application credentials (SQUARE_APP_ID /
/// SQUARE_APP_SECRET) to exchange authorization codes for merchant tokens and
/// to refresh them. It never touches merchant funds directly.
///
/// `ApiClient` uses a merchant's access token (decrypted from the DB at call
/// time) to create orders on their Square POS. It is constructed per-request —
/// cheap, as it only borrows the platform's shared reqwest pool.
///
/// # Security note
///
/// Access tokens are decrypted from the DB immediately before use and are
/// never stored on the struct long-term. The `ApiClient` should be
/// dropped as soon as the API call completes. Neither struct derives Debug
/// or Serialize — tokens cannot leak via a log statement or error response.
use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::error::{AppError, AppResult};

const BASE: &str = "https://connect.squareup.com";

// ── Shared response types ─────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SquareError {
    detail: String,
}

#[derive(Deserialize)]
struct SquareErrorEnvelope {
    errors: Vec<SquareError>,
}

fn first_error(body: &str) -> String {
    serde_json::from_str::<SquareErrorEnvelope>(body)
        .ok()
        .and_then(|e| e.errors.into_iter().next())
        .map(|e| e.detail)
        .unwrap_or_else(|| "unknown Square error".into())
}

// ── OAuth client ──────────────────────────────────────────────────────────────

pub struct OAuthClient<'a> {
    app_id:       &'a str,
    app_secret:   &'a str,
    redirect_url: &'a str,
    http:         &'a reqwest::Client,
}

/// The token set returned by Square after a successful OAuth exchange or refresh.
#[derive(Debug)]
pub struct OAuthTokens {
    pub access_token:  String,
    pub refresh_token: String,
    pub merchant_id:   String,
    pub expires_at:    DateTime<Utc>,
}

impl<'a> OAuthClient<'a> {
    pub fn new(
        app_id:       &'a str,
        app_secret:   &'a str,
        redirect_url: &'a str,
        http:         &'a reqwest::Client,
    ) -> Self {
        Self { app_id, app_secret, redirect_url, http }
    }

    /// Exchanges an authorization code (from the OAuth callback) for an access
    /// token and refresh token.
    pub async fn exchange_code(&self, code: &str) -> AppResult<OAuthTokens> {
        self.token_request(&[
            ("client_id",     self.app_id),
            ("client_secret", self.app_secret),
            ("code",          code),
            ("grant_type",    "authorization_code"),
            ("redirect_uri",  self.redirect_url),
        ]).await
    }

    /// Obtains a new access token using the stored refresh token.
    /// Called transparently by the squareoauth service when the access token
    /// is within 24 hours of expiry.
    pub async fn refresh(&self, refresh_token: &str) -> AppResult<OAuthTokens> {
        self.token_request(&[
            ("client_id",     self.app_id),
            ("client_secret", self.app_secret),
            ("refresh_token", refresh_token),
            ("grant_type",    "refresh_token"),
        ]).await
    }

    async fn token_request(&self, params: &[(&str, &str)]) -> AppResult<OAuthTokens> {
        #[derive(Deserialize)]
        struct TokenResponse {
            access_token:  Option<String>,
            refresh_token: Option<String>,
            merchant_id:   Option<String>,
            expires_at:    Option<String>,
            error:         Option<String>,
            message:       Option<String>,
        }

        let resp = self.http
            .post(format!("{BASE}/oauth2/token"))
            .header("Square-Version", "2024-01-18")
            .json(&params.iter().cloned().collect::<HashMap<_, _>>())
            .send()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Square OAuth request: {e}")))?;

        let body = resp.text().await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Square OAuth read body: {e}")))?;

        let parsed: TokenResponse = serde_json::from_str(&body)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Square OAuth parse: {e}")))?;

        if let Some(err) = parsed.error.or(parsed.message) {
            return Err(AppError::Internal(anyhow::anyhow!("Square OAuth: {err}")));
        }

        let expires_at = parsed.expires_at
            .as_deref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .ok_or_else(|| AppError::Internal(anyhow::anyhow!("Square OAuth: missing expires_at")))?;

        Ok(OAuthTokens {
            access_token:  parsed.access_token.ok_or_else(|| AppError::Internal(anyhow::anyhow!("Square OAuth: missing access_token")))?,
            refresh_token: parsed.refresh_token.ok_or_else(|| AppError::Internal(anyhow::anyhow!("Square OAuth: missing refresh_token")))?,
            merchant_id:   parsed.merchant_id.ok_or_else(|| AppError::Internal(anyhow::anyhow!("Square OAuth: missing merchant_id")))?,
            expires_at,
        })
    }
}

// ── API client ────────────────────────────────────────────────────────────────

pub struct ApiClient<'a> {
    access_token: &'a str,
    http:         &'a reqwest::Client,
}

#[derive(Debug, Clone)]
pub struct OrderLineItem {
    pub name:        String,
    pub quantity:    String, // Square uses string quantities ("1", "2")
    pub price_cents: i64,
}

impl<'a> ApiClient<'a> {
    pub fn new(access_token: &'a str, http: &'a reqwest::Client) -> Self {
        Self { access_token, http }
    }

    /// Creates a paid order on the merchant's Square POS/KDS.
    ///
    /// `reference_id` is the Box Fraise `venue_order.id` — visible on the
    /// Square dashboard and KDS so Belle can correlate app orders with tickets.
    ///
    /// Returns the Square order ID for storage in `venue_orders`.
    pub async fn create_order(
        &self,
        location_id:  &str,
        items:        &[OrderLineItem],
        reference_id: &str,
        idempotency_key: &str,
    ) -> AppResult<String> {
        let line_items: Vec<serde_json::Value> = items.iter().map(|item| {
            serde_json::json!({
                "name":     item.name,
                "quantity": item.quantity,
                "base_price_money": {
                    "amount":   item.price_cents,
                    "currency": "CAD"
                }
            })
        }).collect();

        let body = serde_json::json!({
            "idempotency_key": idempotency_key,
            "order": {
                "location_id":  location_id,
                "reference_id": reference_id,
                "line_items":   line_items,
                "state":        "OPEN"
            }
        });

        let resp = self.http
            .post(format!("{BASE}/v2/orders"))
            .header("Authorization",  format!("Bearer {}", self.access_token))
            .header("Content-Type",   "application/json")
            .header("Square-Version", "2024-01-18")
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Square create_order: {e}")))?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::Internal(anyhow::anyhow!(
                "Square create_order failed: {}", first_error(&text)
            )));
        }

        let json: serde_json::Value = resp.json().await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Square create_order parse: {e}")))?;

        json["order"]["id"]
            .as_str()
            .map(String::from)
            .ok_or_else(|| AppError::Internal(anyhow::anyhow!("Square create_order: missing order.id")))
    }
}

// ── Webhook validation ────────────────────────────────────────────────────────

/// Validates the authenticity of a Square webhook payload.
///
/// Square computes: Base64(HMAC-SHA256(signing_key, notification_url + body))
/// The notification_url must exactly match the URL configured in Square's
/// Developer dashboard — including scheme, host, path, no trailing slash.
pub fn validate_webhook(
    signing_key:      &str,
    notification_url: &str,
    body:             &[u8],
    signature:        &str,
) -> bool {
    use ring::hmac;
    if signature.is_empty() { return false; }

    let key      = hmac::Key::new(hmac::HMAC_SHA256, signing_key.as_bytes());
    let mut ctx  = hmac::Context::with_key(&key);
    ctx.update(notification_url.as_bytes());
    ctx.update(body);
    let expected = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        ctx.sign().as_ref(),
    );

    // Constant-time comparison prevents timing oracle on the signature.
    use ring::constant_time::verify_slices_are_equal;
    verify_slices_are_equal(expected.as_bytes(), signature.as_bytes()).is_ok()
}
