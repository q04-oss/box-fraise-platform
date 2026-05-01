use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use crate::types::UserId;

/// Tiers payable directly via Stripe. Higher tiers require a manual invoice —
/// never accept Stripe payment for them regardless of what the client sends.
pub const STRIPE_PAYABLE_TIERS: &[&str] = &["maison", "reserve", "atelier"];

/// Membership cost in CAD cents per tier.
pub fn tier_amount_cents(tier: &str) -> Option<i64> {
    match tier {
        "maison"     => Some(300_000),        // CA$3,000
        "reserve"    => Some(1_000_000),      // CA$10,000
        "atelier"    => Some(5_000_000),      // CA$50,000
        "fondateur"  => Some(10_000_000),     // CA$100,000
        "patrimoine" => Some(50_000_000),     // CA$500,000
        "souverain"  => Some(100_000_000),    // CA$1,000,000
        "unnamed"    => Some(300_000_000_00), // CA$3,000,000,000
        _            => None,
    }
}

// ── Stored rows ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct MembershipRow {
    pub id:         i32,
    pub user_id:    UserId,
    pub tier:       String,
    pub status:     String,
    pub started_at: Option<NaiveDateTime>,
    pub renews_at:  Option<NaiveDateTime>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct MemberRow {
    pub user_id:      UserId,
    pub display_name: Option<String>,
    pub portrait_url: Option<String>,
    pub tier:         String,
}

// ── Request bodies ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct PaymentIntentBody {
    pub tier: String,
}

// ── Response bodies ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct PaymentIntentResponse {
    pub client_secret: String,
    pub amount_cents:  i64,
    pub tier:          String,
}
