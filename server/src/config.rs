use std::env;

/// All runtime configuration loaded from environment variables at startup.
/// Required fields fail fast at boot; optional fields are `Option<String>`.
#[derive(Debug, Clone)]
pub struct Config {
    // ── Core ─────────────────────────────────────────────────────────────────
    pub database_url: String,
    pub jwt_secret:   String,
    pub port:         u16,

    // ── iOS request signing ───────────────────────────────────────────────────
    /// Fallback HMAC key for non-attested iOS clients.
    /// If absent, requests without an attest key are rejected.
    pub hmac_shared_key: Option<String>,

    // ── Stripe ────────────────────────────────────────────────────────────────
    pub stripe_secret_key:     String,
    pub stripe_webhook_secret: String,

    // ── Operator PINs ─────────────────────────────────────────────────────────
    pub admin_pin:       String,
    pub chocolatier_pin: String,
    pub supplier_pin:    String,
    pub review_pin:      Option<String>,

    // ── Apple ─────────────────────────────────────────────────────────────────
    pub apple_team_id:     Option<String>,
    pub apple_key_id:      Option<String>,
    pub apple_private_key: Option<String>,
    pub apple_client_id:   Option<String>,

    // ── Resend (email) ────────────────────────────────────────────────────────
    pub resend_api_key: Option<String>,

    // ── Anthropic ─────────────────────────────────────────────────────────────
    pub anthropic_api_key: Option<String>,

    // ── Cloudinary ────────────────────────────────────────────────────────────
    pub cloudinary_cloud_name: Option<String>,
    pub cloudinary_api_key:    Option<String>,
    pub cloudinary_api_secret:  Option<String>,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let jwt_secret = require("JWT_SECRET")?;
        if jwt_secret.len() < 32 {
            anyhow::bail!("JWT_SECRET must be at least 32 characters");
        }

        let admin_pin       = require("ADMIN_PIN")?;
        let chocolatier_pin = require("CHOCOLATIER_PIN")?;
        let supplier_pin    = require("SUPPLIER_PIN")?;

        validate_pin("ADMIN_PIN",       &admin_pin)?;
        validate_pin("CHOCOLATIER_PIN", &chocolatier_pin)?;
        validate_pin("SUPPLIER_PIN",    &supplier_pin)?;

        Ok(Self {
            // required
            database_url:          require("DATABASE_URL")?,
            jwt_secret,
            stripe_secret_key:     require("STRIPE_SECRET_KEY")?,
            stripe_webhook_secret: require("STRIPE_WEBHOOK_SECRET")?,
            admin_pin,
            chocolatier_pin,
            supplier_pin,

            // optional with default
            port: optional_parse("PORT", 3001)?,

            // optional
            hmac_shared_key:       optional("FRAISE_HMAC_SHARED_KEY"),
            review_pin:            optional("REVIEW_PIN"),
            apple_team_id:         optional("APPLE_TEAM_ID"),
            apple_key_id:          optional("APPLE_KEY_ID"),
            apple_private_key:     optional("APPLE_PRIVATE_KEY"),
            apple_client_id:       optional("APPLE_CLIENT_ID"),
            resend_api_key:        optional("RESEND_API_KEY"),
            anthropic_api_key:     optional("ANTHROPIC_API_KEY"),
            cloudinary_cloud_name: optional("CLOUDINARY_CLOUD_NAME"),
            cloudinary_api_key:    optional("CLOUDINARY_API_KEY"),
            cloudinary_api_secret:  optional("CLOUDINARY_API_SECRET"),
        })
    }
}

fn require(key: &str) -> anyhow::Result<String> {
    env::var(key).map_err(|_| anyhow::anyhow!("required env var `{key}` is not set"))
}

fn optional(key: &str) -> Option<String> {
    env::var(key).ok()
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
