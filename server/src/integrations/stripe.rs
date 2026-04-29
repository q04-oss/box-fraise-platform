/// Stripe API client — thin reqwest wrapper over the REST API.
///
/// We use the REST API directly rather than a third-party SDK to avoid
/// dependency version conflicts with axum's hyper stack.
use serde::Deserialize;

use crate::error::{AppError, AppResult};

const BASE: &str = "https://api.stripe.com/v1";

// ── Client ────────────────────────────────────────────────────────────────────

pub struct StripeClient<'a> {
    key:    &'a str,
    http:   &'a reqwest::Client,
}

impl<'a> StripeClient<'a> {
    pub fn new(key: &'a str, http: &'a reqwest::Client) -> Self {
        Self { key, http }
    }

    // ── Payment intents ───────────────────────────────────────────────────────

    pub async fn create_payment_intent(
        &self,
        amount_cents:  i64,
        currency:      &str,
        customer_id:   Option<&str>,
        metadata:      &[(&str, &str)],
    ) -> AppResult<PaymentIntent> {
        let mut params: Vec<(&str, String)> = vec![
            ("amount",          amount_cents.to_string()),
            ("currency",        currency.to_string()),
            ("capture_method",  "manual".to_string()),
        ];

        if let Some(cid) = customer_id {
            params.push(("customer", cid.to_string()));
        }

        for (k, v) in metadata {
            params.push((k, v.to_string()));
        }

        self.post_form("/payment_intents", &params).await
    }

    /// Charge a saved payment method off-session (standing orders, renewals).
    ///
    /// Uses auto-capture (`capture_method` defaults to `automatic`) since these
    /// are unattended charges — there is no client present to complete a manual
    /// capture window.
    pub async fn charge_off_session(
        &self,
        amount_cents:    i64,
        currency:        &str,
        customer_id:     &str,
        payment_method:  &str,
        metadata:        &[(&str, &str)],
    ) -> AppResult<PaymentIntent> {
        let mut params: Vec<(&str, String)> = vec![
            ("amount",         amount_cents.to_string()),
            ("currency",       currency.to_string()),
            ("customer",       customer_id.to_string()),
            ("payment_method", payment_method.to_string()),
            ("confirm",        "true".to_string()),
            ("off_session",    "true".to_string()),
        ];

        for (k, v) in metadata {
            params.push((k, v.to_string()));
        }

        self.post_form("/payment_intents", &params).await
    }

    pub async fn get_payment_intent(&self, id: &str) -> AppResult<PaymentIntent> {
        self.get(&format!("/payment_intents/{id}")).await
    }

    pub async fn capture_payment_intent(&self, id: &str) -> AppResult<PaymentIntent> {
        self.post_form(&format!("/payment_intents/{id}/capture"), &[]).await
    }

    pub async fn cancel_payment_intent(&self, id: &str) -> AppResult<()> {
        let _: serde_json::Value = self
            .post_form(&format!("/payment_intents/{id}/cancel"), &[])
            .await?;
        Ok(())
    }

    // ── Customers ─────────────────────────────────────────────────────────────

    pub async fn create_customer(
        &self,
        email: &str,
        name:  Option<&str>,
    ) -> AppResult<String> {
        let mut params = vec![("email", email.to_string())];
        if let Some(n) = name {
            params.push(("name", n.to_string()));
        }
        let c: Customer = self.post_form("/customers", &params).await?;
        Ok(c.id)
    }

    // ── Webhook verification ──────────────────────────────────────────────────

    /// Verify a Stripe-Signature header against the raw request body.
    /// Returns the parsed event on success.
    pub fn verify_webhook(
        &self,
        payload:    &[u8],
        sig_header: &str,
        secret:     &str,
    ) -> AppResult<serde_json::Value> {
        // Header format: t=<timestamp>,v1=<signature>[,v0=<signature>]
        let mut timestamp = "";
        let mut signatures: Vec<&str> = vec![];

        for part in sig_header.split(',') {
            if let Some(t) = part.strip_prefix("t=") {
                timestamp = t;
            } else if let Some(s) = part.strip_prefix("v1=") {
                signatures.push(s);
            }
        }

        if timestamp.is_empty() || signatures.is_empty() {
            return Err(AppError::bad_request("invalid Stripe-Signature header"));
        }

        let signed = format!("{}.{}", timestamp, String::from_utf8_lossy(payload));

        let key = ring::hmac::Key::new(
            ring::hmac::HMAC_SHA256,
            secret.as_bytes(),
        );
        let expected = ring::hmac::sign(&key, signed.as_bytes());
        let expected_hex = hex::encode(expected.as_ref());

        let valid = signatures.iter().any(|s| constant_time_eq(s.as_bytes(), expected_hex.as_bytes()));
        if !valid {
            return Err(AppError::Unauthorized);
        }

        serde_json::from_slice(payload)
            .map_err(|e| AppError::bad_request(format!("invalid webhook payload: {e}")))
    }

    // ── Internal ──────────────────────────────────────────────────────────────

    async fn post_form<T: for<'de> Deserialize<'de>>(
        &self,
        path:   &str,
        params: &[(&str, String)],
    ) -> AppResult<T> {
        let resp = self.http
            .post(format!("{BASE}{path}"))
            .bearer_auth(self.key)
            .form(params)
            .send()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Stripe request failed: {e}")))?;

        self.parse(resp).await
    }

    async fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> AppResult<T> {
        let resp = self.http
            .get(format!("{BASE}{path}"))
            .bearer_auth(self.key)
            .send()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Stripe request failed: {e}")))?;

        self.parse(resp).await
    }

    async fn parse<T: for<'de> Deserialize<'de>>(
        &self,
        resp: reqwest::Response,
    ) -> AppResult<T> {
        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Stripe parse failed: {e}")))?;

        if !status.is_success() {
            let msg = body["error"]["message"]
                .as_str()
                .unwrap_or("stripe error")
                .to_owned();
            return Err(AppError::bad_request(msg));
        }

        serde_json::from_value(body)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Stripe deserialize failed: {e}")))
    }
}

// ── Response types ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct PaymentIntent {
    pub id:            String,
    pub client_secret: Option<String>,
    pub status:        String,
    pub amount:        i64,
}

#[derive(Debug, Deserialize)]
struct Customer {
    id: String,
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() { return false; }
    a.iter().zip(b.iter()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}
