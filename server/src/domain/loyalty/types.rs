use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── Config ────────────────────────────────────────────────────────────────────

#[derive(Debug, sqlx::FromRow)]
pub struct LoyaltyConfig {
    pub steeps_per_reward:  i32,
    pub reward_description: String,
}

// ── Balance ───────────────────────────────────────────────────────────────────

/// Response for GET /api/businesses/:id/loyalty
///
/// All fields are derived from the event log at query time — there is no
/// mutable balance column. `current_balance` and `steeps_until_reward` are
/// computed here so the iOS app never needs to know the reward threshold.
#[derive(Debug, Serialize)]
pub struct LoyaltyBalance {
    pub steeps_earned:      i64,
    pub rewards_redeemed:   i64,
    /// Net steeps available: steeps_earned - (rewards_redeemed × steeps_per_reward)
    pub current_balance:    i64,
    pub steeps_per_reward:  i32,
    pub reward_description: String,
    /// How many more steeps until the next reward. Zero means a reward is available.
    pub steeps_until_reward: i64,
    pub reward_available:   bool,
    /// False for email+password accounts that haven't clicked their verification
    /// link. Walk-in QR stamps are gated on this. In-app payments always credit
    /// the steep regardless — payment is a stronger verification signal.
    pub email_verified:     bool,
}

// ── Events ────────────────────────────────────────────────────────────────────

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct LoyaltyEventRow {
    pub id:              i64,
    pub event_type:      String,
    pub source:          String,
    pub metadata:        serde_json::Value,
    pub created_at:      DateTime<Utc>,
}

// ── QR token ─────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct QrTokenResponse {
    /// The token to encode in the QR. Expires in 5 minutes.
    pub token:      String,
    /// When the token expires. iOS uses this to show a countdown and auto-refresh.
    pub expires_at: DateTime<Utc>,
}

// ── Stamp request (JSON API, used by iOS app scanner) ────────────────────────

#[derive(Debug, Deserialize)]
pub struct StampBody {
    /// The QR token scanned from the customer's screen.
    pub qr_token: String,
}

// ── Stamp result ──────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct StampResult {
    pub business_id:        i32,
    pub customer_name:      String,
    pub new_balance:        i64,
    pub reward_available:   bool,
    pub reward_description: String,
}
