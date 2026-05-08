// This is the ONLY file permitted to call std::env::var / var_os / vars.
// All other code must obtain configuration through the Config struct.
// The allow below is the sole exemption — do not copy it elsewhere.
// See CONTRIBUTING.md and .clippy.toml for the workspace rule.
#![allow(clippy::disallowed_methods)]
#![allow(missing_docs)] // Config fields are self-documenting by name; see .env.example for descriptions.

use std::env;
use secrecy::SecretString;

/// Runtime configuration for the box-fraise platform server.
///
/// All fields are loaded once at startup from environment variables by
/// [`Config::load`]. The server exits immediately with a clear error message
/// if any required variable is missing or invalid — configuration errors are
/// never deferred to first use.
#[derive(Debug, Clone)]
pub struct Config {
    pub database_url:             SecretString,
    /// Optional production-only DATABASE_URL that connects as the non-superuser
    /// `app_user` role (Hardening Section 2c). When set, the main connection
    /// pool uses this URL and RLS policies in migration 002 actually enforce.
    /// When unset, the server falls back to `database_url` (typically the
    /// `fraise` superuser locally), which bypasses RLS via `BYPASSRLS`.
    pub app_user_database_url:    Option<SecretString>,
    pub jwt_secret:               SecretString,
    pub jwt_secret_previous:      Option<SecretString>,
    pub port:                     u16,
    pub hmac_shared_key:          Option<SecretString>,
    pub redis_url:                Option<SecretString>,
    pub stripe_secret_key:        SecretString,
    pub stripe_webhook_secret:    SecretString,
    pub admin_pin:                SecretString,
    pub chocolatier_pin:          SecretString,
    pub supplier_pin:             SecretString,
    pub review_pin:               Option<SecretString>,
    pub apple_team_id:            Option<String>,
    pub apple_key_id:             Option<String>,
    pub apple_client_id:          Option<String>,
    pub apple_private_key:        Option<SecretString>,
    pub resend_api_key:           Option<SecretString>,
    pub anthropic_api_key:        Option<SecretString>,
    pub anthropic_base_url:       Option<String>,
    pub cloudinary_cloud_name:    Option<String>,
    pub cloudinary_api_key:       Option<SecretString>,
    pub cloudinary_api_secret:    Option<SecretString>,
    pub staff_jwt_secret:         SecretString,
    pub staff_jwt_secret_previous: Option<SecretString>,
    pub square_app_id:            Option<String>,
    pub square_app_secret:        Option<SecretString>,
    pub square_oauth_redirect_url: Option<String>,
    pub square_token_encryption_key: Option<SecretString>,
    pub operator_email:           Option<String>,
    pub api_base_url:             String,
    pub app_store_id:             Option<String>,
    pub platform_fee_bips:        i64,
    pub square_order_webhook_signing_key: Option<SecretString>,
    pub square_order_notification_url:    Option<String>,
    /// HMAC key for soultoken display code derivation (BFIP Section 7 / cryptography.md Section 3).
    pub soultoken_hmac_key:    SecretString,
    /// Ed25519 private key (32 bytes) as 64 hex chars — soultoken payload signing
    /// (BFIP cryptography.md Section 4 / Hardening Section 1b).
    pub soultoken_signing_key_hex:   SecretString,
    /// Ed25519 public key (32 bytes) as 64 hex chars — published via the
    /// trust-registry endpoint for offline signature verification.
    pub soultoken_verifying_key_hex: String,
    /// DigitalOcean Spaces (S3-compatible) credentials. All five fields
    /// must be present together — the [`storage_config`] helper validates.
    pub spaces_access_key:           Option<SecretString>,
    pub spaces_secret_key:           Option<SecretString>,
    pub spaces_bucket:               Option<String>,
    pub spaces_endpoint:             Option<String>,
    pub spaces_region:               Option<String>,
    /// Default expiry for presigned `GET` URLs (seconds). DigitalOcean
    /// recommends ≤ 15 minutes (900s) to limit URL-leak blast radius.
    pub spaces_presign_expiry_seconds: u64,
    /// Sentry error-tracking DSN (Hardening §4). When set, Sentry captures
    /// panics, error-level tracing events, and a 10% sample of request
    /// traces. When unset, Sentry is disabled and the server logs locally only.
    pub sentry_dsn:                  Option<String>,
    /// Hardening §6 — explicit CORS allow-list. Defaults to
    /// `["http://localhost:3000"]` when `ALLOWED_ORIGINS` is unset.
    pub allowed_origins:             Vec<String>,
    /// Hardening §6 — Postgres pool max connections (default 20).
    pub db_max_connections:          u32,
    /// Hardening §6 — Postgres pool min warm connections (default 2).
    pub db_min_connections:          u32,
    /// Hardening §6 — Postgres pool acquire-timeout seconds (default 5).
    pub db_acquire_timeout_secs:     u64,
    /// Hardening §3a / Grade A item 2 — App Attest device-attestation gate
    /// for presence/beacon writes. Default `false` so dev and CI run without
    /// real iOS device assertions; production sets `true`.
    pub app_attest_enabled:          bool,
    /// App Attest RP ID — typically `"<TEAM_ID>.<BUNDLE_ID>"`. Required only
    /// for the registration-time `parse_attestation` path; assertion-time
    /// verification reads it from the stored attestation.
    pub app_attest_app_id:           Option<String>,
}

impl Config {
    /// Load configuration from environment variables.
    ///
    /// Returns `Err` immediately if any required variable is absent or invalid.
    /// Panicking at load time is intentional — a misconfigured server should
    /// never reach the TCP bind.
    pub fn load() -> anyhow::Result<Self> {
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

        if env::var("SQUARE_APP_ID").is_ok() {
            if env::var("SQUARE_ORDER_WEBHOOK_SIGNING_KEY").is_err() {
                anyhow::bail!(
                    "SQUARE_ORDER_WEBHOOK_SIGNING_KEY is required when SQUARE_APP_ID is set"
                );
            }
            if env::var("SQUARE_ORDER_NOTIFICATION_URL").is_err() {
                anyhow::bail!(
                    "SQUARE_ORDER_NOTIFICATION_URL is required when SQUARE_APP_ID is set"
                );
            }
        }

        let soultoken_hmac_key_raw = require("SOULTOKEN_HMAC_KEY")?;
        if soultoken_hmac_key_raw.len() < 32 {
            anyhow::bail!("SOULTOKEN_HMAC_KEY must be at least 32 characters");
        }
        let soultoken_signing_key_hex_raw = require("SOULTOKEN_SIGNING_KEY_HEX")?;
        validate_ed25519_hex("SOULTOKEN_SIGNING_KEY_HEX", &soultoken_signing_key_hex_raw)?;
        let soultoken_verifying_key_hex_raw = require("SOULTOKEN_VERIFYING_KEY_HEX")?;
        validate_ed25519_hex("SOULTOKEN_VERIFYING_KEY_HEX", &soultoken_verifying_key_hex_raw)?;

        let hmac_shared_key = optional_secret("FRAISE_HMAC_SHARED_KEY");
        if hmac_shared_key.is_none() {
            tracing::warn!(
                "FRAISE_HMAC_SHARED_KEY not set — iOS HMAC request signing \
                 will fail for all requests"
            );
        }

        Ok(Self {
            database_url:          require_secret("DATABASE_URL")?,
            app_user_database_url: optional_secret("APP_USER_DATABASE_URL"),
            jwt_secret:            jwt_secret_raw.into(),
            jwt_secret_previous:   optional_secret("JWT_SECRET_PREVIOUS"),
            staff_jwt_secret:      staff_jwt_secret_raw.into(),
            staff_jwt_secret_previous: optional_secret("STAFF_JWT_SECRET_PREVIOUS"),
            stripe_secret_key:     require_secret("STRIPE_SECRET_KEY")?,
            stripe_webhook_secret: require_secret("STRIPE_WEBHOOK_SECRET")?,
            admin_pin:             admin_pin_raw.into(),
            chocolatier_pin:       chocolatier_pin_raw.into(),
            supplier_pin:          supplier_pin_raw.into(),
            port:                  optional_parse("PORT", 3001)?,
            hmac_shared_key,
            redis_url:             optional_secret("REDIS_URL"),
            review_pin:            optional_secret("REVIEW_PIN"),
            apple_private_key:     optional_secret("APPLE_PRIVATE_KEY"),
            resend_api_key:        optional_secret("RESEND_API_KEY"),
            anthropic_api_key:     optional_secret("ANTHROPIC_API_KEY"),
            anthropic_base_url:    optional("ANTHROPIC_API_BASE_URL"),
            cloudinary_api_key:    optional_secret("CLOUDINARY_API_KEY"),
            cloudinary_api_secret: optional_secret("CLOUDINARY_API_SECRET"),
            operator_email:        optional("OPERATOR_EMAIL"),
            apple_team_id:         optional("APPLE_TEAM_ID"),
            apple_key_id:          optional("APPLE_KEY_ID"),
            apple_client_id:       optional("APPLE_CLIENT_ID"),
            cloudinary_cloud_name: optional("CLOUDINARY_CLOUD_NAME"),
            api_base_url:          optional("API_BASE_URL")
                                       .unwrap_or_else(|| "http://localhost:3001".to_string()),
            app_store_id:          optional("APP_STORE_ID"),
            platform_fee_bips:     optional_parse("PLATFORM_FEE_BIPS", 500i64)?,
            square_order_webhook_signing_key: optional_secret("SQUARE_ORDER_WEBHOOK_SIGNING_KEY"),
            square_order_notification_url:    optional("SQUARE_ORDER_NOTIFICATION_URL"),
            square_app_id:             optional("SQUARE_APP_ID"),
            square_app_secret:         optional_secret("SQUARE_APP_SECRET"),
            square_oauth_redirect_url: optional("SQUARE_OAUTH_REDIRECT_URL"),
            square_token_encryption_key: optional_secret("SQUARE_TOKEN_ENCRYPTION_KEY"),
            soultoken_hmac_key:          soultoken_hmac_key_raw.into(),
            soultoken_signing_key_hex:   soultoken_signing_key_hex_raw.into(),
            soultoken_verifying_key_hex: soultoken_verifying_key_hex_raw,
            spaces_access_key:           optional_secret("SPACES_ACCESS_KEY"),
            spaces_secret_key:           optional_secret("SPACES_SECRET_KEY"),
            spaces_bucket:               optional("SPACES_BUCKET"),
            spaces_endpoint:             optional("SPACES_ENDPOINT"),
            spaces_region:               optional("SPACES_REGION"),
            spaces_presign_expiry_seconds: optional_parse("SPACES_PRESIGN_EXPIRY_SECONDS", 900u64)?,
            sentry_dsn:                  optional("SENTRY_DSN"),
            allowed_origins:             optional("ALLOWED_ORIGINS")
                                            .map(|s| s.split(',').map(|o| o.trim().to_string()).filter(|o| !o.is_empty()).collect::<Vec<_>>())
                                            .unwrap_or_else(|| vec!["http://localhost:3000".to_string()]),
            db_max_connections:          optional_parse("DB_MAX_CONNECTIONS", 20u32)?,
            db_min_connections:          optional_parse("DB_MIN_CONNECTIONS", 2u32)?,
            db_acquire_timeout_secs:     optional_parse("DB_ACQUIRE_TIMEOUT_SECS", 5u64)?,
            app_attest_enabled:          optional_parse("APP_ATTEST_ENABLED", false)?,
            app_attest_app_id:           optional("APP_ATTEST_APP_ID"),
        })
    }

    /// Build a [`StorageConfig`](box_fraise_integrations::storage::StorageConfig)
    /// when all five `SPACES_*` env vars are present. Returns `None` in dev/test
    /// where storage isn't configured — callers should treat that as
    /// "storage feature disabled" and surface a 503 to clients.
    pub fn storage_config(&self) -> Option<box_fraise_integrations::storage::StorageConfig> {
        use secrecy::ExposeSecret;
        let access_key = self.spaces_access_key.as_ref()?.expose_secret().to_owned();
        let secret_key = self.spaces_secret_key.as_ref()?.expose_secret().to_owned();
        let bucket     = self.spaces_bucket.clone()?;
        let endpoint   = self.spaces_endpoint.clone()
            .unwrap_or_else(|| "https://nyc3.digitaloceanspaces.com".to_owned());
        let region     = self.spaces_region.clone().unwrap_or_else(|| "nyc3".to_owned());
        Some(box_fraise_integrations::storage::StorageConfig {
            access_key, secret_key, bucket, endpoint, region,
            presign_expiry_seconds: self.spaces_presign_expiry_seconds,
        })
    }
}

fn validate_ed25519_hex(key: &str, value: &str) -> anyhow::Result<()> {
    if value.len() != 64 {
        anyhow::bail!("`{key}` must be exactly 64 hex chars (32 bytes), got {} chars", value.len());
    }
    if !value.chars().all(|c| c.is_ascii_hexdigit()) {
        anyhow::bail!("`{key}` must be valid lowercase or uppercase hex");
    }
    Ok(())
}

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

fn validate_pin(key: &str, pin: &str) -> anyhow::Result<()> {
    if pin.len() < 8 {
        anyhow::bail!("`{key}` must be at least 8 characters");
    }
    let first = pin.chars().next().unwrap();
    if pin.chars().all(|c| c == first) {
        anyhow::bail!("`{key}` must not be all the same character");
    }
    Ok(())
}
