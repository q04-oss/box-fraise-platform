use std::env;
use secrecy::SecretString;

/// All runtime configuration loaded from environment variables at startup.
///
/// Fields that hold secrets (API keys, signing keys, passwords, credentials)
/// are typed as `SecretString` rather than `String`. This prevents them from
/// appearing in log output (Debug prints `[REDACTED]`), ensures they are
/// zeroized on drop, and makes accidental serialization into a response a
/// compile error. Call `.expose_secret()` only at the exact point of use.
///
/// Fields that are public identifiers (Apple team ID, bundle ID, cloud name)
/// remain `String` — wrapping them as secrets would be security theater.
#[derive(Debug, Clone)]
pub struct Config {
    // ── Core ─────────────────────────────────────────────────────────────────
    /// Threat: credential exposure via log/error leakage — contains DB password.
    pub database_url: SecretString,
    /// Threat: token forgery if leaked — used to sign and verify all JWTs.
    pub jwt_secret:   SecretString,
    pub port:         u16,

    // ── iOS request signing ───────────────────────────────────────────────────
    /// Threat: HMAC bypass — fallback key for non-attested iOS clients.
    /// If absent, requests without an attest key are rejected.
    pub hmac_shared_key: Option<SecretString>,
    /// Redis URL for nonce deduplication. When set, nonces are stored with
    /// `SET fraise:nonce:<uuid> EX 300 NX` — atomic and multi-instance safe.
    /// When absent, an in-process HashMap is used (single instance only).
    pub redis_url: Option<SecretString>,

    // ── Stripe ────────────────────────────────────────────────────────────────
    /// Threat: payment fraud — full Stripe API access with this key.
    pub stripe_secret_key:     SecretString,
    /// Threat: webhook spoofing — verifies Stripe webhook authenticity.
    pub stripe_webhook_secret: SecretString,

    // ── Operator PINs ─────────────────────────────────────────────────────────
    /// Threat: admin API access — PINs gate all operator-level mutations.
    pub admin_pin:       SecretString,
    pub chocolatier_pin: SecretString,
    pub supplier_pin:    SecretString,
    pub review_pin:      Option<SecretString>,

    // ── Apple ─────────────────────────────────────────────────────────────────
    /// Public identifiers — not secrets, intentionally plain String.
    pub apple_team_id:   Option<String>,
    pub apple_key_id:    Option<String>,
    pub apple_client_id: Option<String>,
    /// Threat: Apple Sign In token forgery if leaked — private signing key.
    pub apple_private_key: Option<SecretString>,

    // ── Resend (email) ────────────────────────────────────────────────────────
    /// Threat: email abuse / phishing at scale if leaked.
    pub resend_api_key: Option<SecretString>,

    // ── Anthropic ─────────────────────────────────────────────────────────────
    /// Threat: compute cost / data exfiltration if leaked.
    pub anthropic_api_key: Option<SecretString>,

    // ── Cloudinary ────────────────────────────────────────────────────────────
    /// Public cloud name — not a secret.
    pub cloudinary_cloud_name: Option<String>,
    /// Threat: media upload abuse / asset deletion if leaked.
    pub cloudinary_api_key:    Option<SecretString>,
    pub cloudinary_api_secret: Option<SecretString>,

    // ── Staff auth ────────────────────────────────────────────────────────────
    /// Separate signing secret for staff JWTs so a compromised user token is
    /// cryptographically invalid at any staff endpoint even if a serde bug appeared.
    /// Threat: staff endpoint access by regular users — structural isolation via
    /// StaffClaims is the first line; a separate secret is defense-in-depth.
    pub staff_jwt_secret: SecretString,

    // ── Square OAuth ──────────────────────────────────────────────────────────
    /// Public Square application ID — safe to log.
    pub square_app_id: Option<String>,
    /// Threat: OAuth impersonation — used to exchange authorization codes for tokens.
    pub square_app_secret: Option<SecretString>,
    /// Must exactly match the redirect URL registered in the Square Developer dashboard.
    pub square_oauth_redirect_url: Option<String>,
    /// 32-byte hex key for AES-256-GCM encryption of Square OAuth tokens at rest.
    /// Threat: DB credential leak exposes Square tokens — application-layer encryption
    /// ensures tokens are useless without this key.
    /// Generate with: openssl rand -hex 32
    pub square_token_encryption_key: Option<SecretString>,

    // ── Venue ordering ────────────────────────────────────────────────────────
    /// Platform fee on venue drink orders, in basis points. 500 = 5%.
    /// Applied as Stripe's application_fee_amount on Connect charges.
    /// Default: 500 (5%). Set to 0 to disable the fee during testing.
    pub platform_fee_bips: i64,

    // ── Square order webhooks ─────────────────────────────────────────────────
    /// Signing key for the Square order.updated webhook subscription.
    /// Separate from SQUARE_WEBHOOK_SIGNING_KEY (which covers payment events)
    /// because Square issues a distinct key per webhook subscription.
    /// Threat: webhook spoofing — a missing key causes the endpoint to return 503.
    pub square_order_webhook_signing_key: Option<SecretString>,
    /// Must exactly match the order webhook URL in the Square Developer dashboard.
    /// e.g. https://your-api.railway.app/api/webhooks/square/orders
    pub square_order_notification_url: Option<String>,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        // Validate before wrapping so we work on plain &str.
        let jwt_secret_raw = require("JWT_SECRET")?;
        if jwt_secret_raw.len() < 32 {
            anyhow::bail!("JWT_SECRET must be at least 32 characters");
        }

        let staff_jwt_secret_raw = require("STAFF_JWT_SECRET")?;
        if staff_jwt_secret_raw.len() < 32 {
            anyhow::bail!("STAFF_JWT_SECRET must be at least 32 characters");
        }

        let admin_pin_raw       = require("ADMIN_PIN")?;
        let chocolatier_pin_raw = require("CHOCOLATIER_PIN")?;
        let supplier_pin_raw    = require("SUPPLIER_PIN")?;

        validate_pin("ADMIN_PIN",       &admin_pin_raw)?;
        validate_pin("CHOCOLATIER_PIN", &chocolatier_pin_raw)?;
        validate_pin("SUPPLIER_PIN",    &supplier_pin_raw)?;

        Ok(Self {
            // required secrets
            database_url:          require_secret("DATABASE_URL")?,
            jwt_secret:            jwt_secret_raw.into(),
            staff_jwt_secret:      staff_jwt_secret_raw.into(),
            stripe_secret_key:     require_secret("STRIPE_SECRET_KEY")?,
            stripe_webhook_secret: require_secret("STRIPE_WEBHOOK_SECRET")?,
            admin_pin:             admin_pin_raw.into(),
            chocolatier_pin:       chocolatier_pin_raw.into(),
            supplier_pin:          supplier_pin_raw.into(),

            // optional with default
            port: optional_parse("PORT", 3001)?,

            // optional secrets
            hmac_shared_key:   optional_secret("FRAISE_HMAC_SHARED_KEY"),
            redis_url:         optional_secret("REDIS_URL"),
            review_pin:        optional_secret("REVIEW_PIN"),
            apple_private_key: optional_secret("APPLE_PRIVATE_KEY"),
            resend_api_key:    optional_secret("RESEND_API_KEY"),
            anthropic_api_key: optional_secret("ANTHROPIC_API_KEY"),
            cloudinary_api_key:    optional_secret("CLOUDINARY_API_KEY"),
            cloudinary_api_secret: optional_secret("CLOUDINARY_API_SECRET"),

            // optional public identifiers
            apple_team_id:         optional("APPLE_TEAM_ID"),
            apple_key_id:          optional("APPLE_KEY_ID"),
            apple_client_id:       optional("APPLE_CLIENT_ID"),
            cloudinary_cloud_name: optional("CLOUDINARY_CLOUD_NAME"),

            // staff auth
            staff_jwt_secret: require_secret("STAFF_JWT_SECRET")?,

            // venue ordering
            platform_fee_bips:                    optional_parse("PLATFORM_FEE_BIPS", 500i64)?,
            square_order_webhook_signing_key:     optional_secret("SQUARE_ORDER_WEBHOOK_SIGNING_KEY"),
            square_order_notification_url:        optional("SQUARE_ORDER_NOTIFICATION_URL"),

            // Square OAuth — all optional; Square integration is disabled when absent
            square_app_id:                optional("SQUARE_APP_ID"),
            square_app_secret:            optional_secret("SQUARE_APP_SECRET"),
            square_oauth_redirect_url:    optional("SQUARE_OAUTH_REDIRECT_URL"),
            square_token_encryption_key:  optional_secret("SQUARE_TOKEN_ENCRYPTION_KEY"),
        })
    }
}

// ── Loaders ───────────────────────────────────────────────────────────────────

fn require(key: &str) -> anyhow::Result<String> {
    env::var(key).map_err(|_| anyhow::anyhow!("required env var `{key}` is not set"))
}

fn require_secret(key: &str) -> anyhow::Result<SecretString> {
    require(key).map(SecretString::from)
}

fn optional(key: &str) -> Option<String> {
    env::var(key).ok()
}

fn optional_secret(key: &str) -> Option<SecretString> {
    env::var(key).ok().map(SecretString::from)
}

fn optional_parse<T>(key: &str, default: T) -> anyhow::Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match env::var(key) {
        Ok(v)  => v.parse::<T>().map_err(|e| anyhow::anyhow!("`{key}` is invalid: {e}")),
        Err(_) => Ok(default),
    }
}

/// Reject weak PINs at startup so misconfigured deployments fail fast.
/// Requirements: at least 8 characters, not all the same character.
fn validate_pin(key: &str, pin: &str) -> anyhow::Result<()> {
    if pin.len() < 8 {
        anyhow::bail!("`{key}` must be at least 8 characters");
    }
    let first = pin.chars().next().unwrap();
    if pin.chars().all(|c| c == first) {
        anyhow::bail!("`{key}` must not be all the same character (e.g. '11111111')");
    }
    Ok(())
}
