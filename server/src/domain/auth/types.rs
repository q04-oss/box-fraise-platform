use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use crate::types::{StripeCustomerId, UserId};

// ── Database row ──────────────────────────────────────────────────────────────

/// Subset of `users` columns used across the auth domain.
/// Extend with additional columns as other domains require them.
#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct UserRow {
    pub id:                        UserId,
    pub apple_user_id:             Option<String>,
    pub email:                     String,
    pub display_name:              Option<String>,
    pub push_token:                Option<String>,
    pub user_code:                 Option<String>,
    pub verified:                  bool,
    pub banned:                    bool,
    pub table_verified:            bool,
    pub is_dorotka:                bool,
    pub stripe_customer_id:        Option<StripeCustomerId>,
    pub stripe_connect_account_id: Option<String>,
    pub stripe_connect_onboarded:  bool,
    pub ad_balance_cents:          i32,
    pub platform_credit_cents:     i32,
    pub social_time_bank_seconds:  i32,
    pub portal_opted_in:           bool,
    pub identity_verified:         bool,
    pub worker_status:             Option<String>,
    pub eth_address:               Option<String>,
    pub portrait_url:              Option<String>,
    pub password_hash:             Option<String>,
    pub created_at:                NaiveDateTime,
}

/// Columns to SELECT when fetching a full UserRow.
pub const USER_COLS: &str =
    "id, apple_user_id, email, display_name, push_token, user_code, \
     verified, banned, table_verified, is_dorotka, stripe_customer_id, \
     stripe_connect_account_id, stripe_connect_onboarded, \
     ad_balance_cents, platform_credit_cents, social_time_bank_seconds, \
     portal_opted_in, identity_verified, worker_status, eth_address, \
     portrait_url, password_hash, created_at";

// ── Request bodies ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct AppleAuthBody {
    pub identity_token: String,
    pub display_name:   Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OperatorAuthBody {
    pub code:        String,
    pub location_id: i32,
}

#[derive(Debug, Deserialize)]
pub struct DemoAuthBody {
    pub pin: String,
}

#[derive(Debug, Deserialize)]
pub struct RegisterBody {
    pub email:        String,
    pub password:     String,
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LoginBody {
    pub email:    String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct ForgotPasswordBody {
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct ResetPasswordBody {
    pub token:    String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct ClaimBookingBody {
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct PushTokenBody {
    pub push_token: String,
}

#[derive(Debug, Deserialize)]
pub struct DisplayNameBody {
    pub display_name: String,
}

// ── Staff auth ────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct StaffLoginBody {
    /// The location whose staff_pin is being presented.
    pub location_id: i32,
    /// The PIN printed on the staff card / shown in the business dashboard.
    pub pin: String,
}

// ── Response bodies ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct StaffAuthResponse {
    pub user_id:     i32,
    pub business_id: i32,
    /// Short-lived JWT signed with STAFF_JWT_SECRET. TTL: 8 hours.
    pub token:       String,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub user_id: UserId,
    pub token:   String,
    pub is_new:  bool,
    pub verified: bool,
}

#[derive(Debug, Serialize)]
pub struct MeResponse {
    #[serde(flatten)]
    pub user: UserRow,
    /// Populated once the `table` domain is ported.
    pub table_bookings: Vec<serde_json::Value>,
}
