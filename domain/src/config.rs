use std::env;
use secrecy::SecretString;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url:             SecretString,
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
}

impl Config {
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

        Ok(Self {
            database_url:          require_secret("DATABASE_URL")?,
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
            hmac_shared_key:       optional_secret("FRAISE_HMAC_SHARED_KEY"),
            redis_url:             optional_secret("REDIS_URL"),
            review_pin:            optional_secret("REVIEW_PIN"),
            apple_private_key:     optional_secret("APPLE_PRIVATE_KEY"),
            resend_api_key:        optional_secret("RESEND_API_KEY"),
            anthropic_api_key:     optional_secret("ANTHROPIC_API_KEY"),
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
        })
    }
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
