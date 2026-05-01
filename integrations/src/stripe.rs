/// Stripe API client — thin reqwest wrapper over the REST API.
use serde::Deserialize;

const BASE: &str = "https://api.stripe.com/v1";

// ── Client ────────────────────────────────────────────────────────────────────

pub struct StripeClient<'a> {
    key:  &'a str,
    http: &'a reqwest::Client,
}

impl<'a> StripeClient<'a> {
    pub fn new(key: &'a str, http: &'a reqwest::Client) -> Self {
        Self { key, http }
    }

    pub async fn create_payment_intent(
        &self,
        amount_cents: i64,
        currency:     &str,
        customer_id:  Option<&str>,
        metadata:     &[(&str, &str)],
    ) -> anyhow::Result<PaymentIntent> {
        let mut params: Vec<(&str, String)> = vec![
            ("amount",         amount_cents.to_string()),
            ("currency",       currency.to_string()),
            ("capture_method", "manual".to_string()),
        ];
        if let Some(cid) = customer_id {
            params.push(("customer", cid.to_string()));
        }
        for (k, v) in metadata {
            params.push((k, v.to_string()));
        }
        self.post_form("/payment_intents", &params).await
    }

    pub async fn charge_off_session(
        &self,
        amount_cents:   i64,
        currency:       &str,
        customer_id:    &str,
        payment_method: &str,
        metadata:       &[(&str, &str)],
    ) -> anyhow::Result<PaymentIntent> {
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

    pub async fn get_payment_intent(&self, id: &str) -> anyhow::Result<PaymentIntent> {
        self.get(&format!("/payment_intents/{id}")).await
    }

    pub async fn capture_payment_intent(&self, id: &str) -> anyhow::Result<PaymentIntent> {
        self.post_form(&format!("/payment_intents/{id}/capture"), &[]).await
    }

    pub async fn cancel_payment_intent(&self, id: &str) -> anyhow::Result<()> {
        let _: serde_json::Value = self
            .post_form(&format!("/payment_intents/{id}/cancel"), &[])
            .await?;
        Ok(())
    }

    pub async fn create_payment_intent_connect(
        &self,
        amount_cents:    i64,
        fee_cents:       i64,
        connect_account: &str,
        customer_id:     Option<&str>,
        metadata:        &[(&str, &str)],
    ) -> anyhow::Result<PaymentIntent> {
        let mut params: Vec<(&str, String)> = vec![
            ("amount",                     amount_cents.to_string()),
            ("currency",                   "cad".to_string()),
            ("capture_method",             "automatic".to_string()),
            ("transfer_data[destination]", connect_account.to_string()),
        ];
        if fee_cents > 0 {
            params.push(("application_fee_amount", fee_cents.to_string()));
        }
        if let Some(cid) = customer_id {
            params.push(("customer", cid.to_string()));
        }
        for (k, v) in metadata {
            params.push((k, v.to_string()));
        }
        self.post_form("/payment_intents", &params).await
    }

    pub async fn create_connect_account(&self, email: &str) -> anyhow::Result<String> {
        #[derive(serde::Deserialize)]
        struct Account { id: String }
        let account: Account = self.post_form("/accounts", &[
            ("type",  "express".to_string()),
            ("email", email.to_string()),
            ("capabilities[card_payments][requested]", "true".to_string()),
            ("capabilities[transfers][requested]",     "true".to_string()),
        ]).await?;
        Ok(account.id)
    }

    pub async fn create_account_link(
        &self,
        account_id:  &str,
        refresh_url: &str,
        return_url:  &str,
    ) -> anyhow::Result<String> {
        #[derive(serde::Deserialize)]
        struct AccountLink { url: String }
        let link: AccountLink = self.post_form("/account_links", &[
            ("account",     account_id.to_string()),
            ("refresh_url", refresh_url.to_string()),
            ("return_url",  return_url.to_string()),
            ("type",        "account_onboarding".to_string()),
        ]).await?;
        Ok(link.url)
    }

    pub async fn create_customer(
        &self,
        email: &str,
        name:  Option<&str>,
    ) -> anyhow::Result<String> {
        let mut params = vec![("email", email.to_string())];
        if let Some(n) = name {
            params.push(("name", n.to_string()));
        }
        let c: Customer = self.post_form("/customers", &params).await?;
        Ok(c.id)
    }

    /// Verify a Stripe-Signature header against the raw request body.
    pub fn verify_webhook(
        &self,
        payload:    &[u8],
        sig_header: &str,
        secret:     &str,
    ) -> anyhow::Result<serde_json::Value> {
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
            return Err(anyhow::anyhow!("invalid Stripe-Signature header"));
        }

        let signed = format!("{}.{}", timestamp, String::from_utf8_lossy(payload));
        let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, secret.as_bytes());
        let expected = ring::hmac::sign(&key, signed.as_bytes());
        let expected_hex = hex::encode(expected.as_ref());

        let valid = signatures.iter().any(|s| constant_time_eq(s.as_bytes(), expected_hex.as_bytes()));
        if !valid {
            return Err(anyhow::anyhow!("invalid Stripe-Signature"));
        }

        serde_json::from_slice(payload)
            .map_err(|e| anyhow::anyhow!("invalid webhook payload: {e}"))
    }

    async fn post_form<T: for<'de> Deserialize<'de>>(
        &self,
        path:   &str,
        params: &[(&str, String)],
    ) -> anyhow::Result<T> {
        let resp = self.http
            .post(format!("{BASE}{path}"))
            .bearer_auth(self.key)
            .form(params)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Stripe request failed: {e}"))?;
        self.parse(resp).await
    }

    async fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> anyhow::Result<T> {
        let resp = self.http
            .get(format!("{BASE}{path}"))
            .bearer_auth(self.key)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Stripe request failed: {e}"))?;
        self.parse(resp).await
    }

    async fn parse<T: for<'de> Deserialize<'de>>(
        &self,
        resp: reqwest::Response,
    ) -> anyhow::Result<T> {
        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("Stripe parse failed: {e}"))?;

        if !status.is_success() {
            let msg = body["error"]["message"]
                .as_str()
                .unwrap_or("stripe error")
                .to_owned();
            return Err(anyhow::anyhow!("{msg}"));
        }

        serde_json::from_value(body)
            .map_err(|e| anyhow::anyhow!("Stripe deserialize failed: {e}"))
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
