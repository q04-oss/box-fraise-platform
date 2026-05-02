use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── Database rows ─────────────────────────────────────────────────────────────

/// Full row from the `businesses` table.
#[allow(missing_docs)]
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct BusinessRow {
    pub id:                         i32,
    pub location_id:                i32,
    pub soultoken_id:               Option<i32>,
    pub primary_holder_id:          i32,
    pub primary_holder_soultoken_id: Option<i32>,
    pub stripe_customer_id:         Option<String>,
    pub name:                       String,
    pub verification_status:        String,
    pub beacon_suspended:           bool,
    pub beacon_suspended_at:        Option<DateTime<Utc>>,
    pub suspended_at:               Option<DateTime<Utc>>,
    pub onboarded_at:               Option<DateTime<Utc>>,
    pub is_active:                  bool,
    pub platform_fee_cents:         i32,
    pub deleted_at:                 Option<DateTime<Utc>>,
    pub updated_at:                 DateTime<Utc>,
    pub created_at:                 DateTime<Utc>,
}

/// Full row from the `locations` table.
#[allow(missing_docs)]
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LocationRow {
    pub id:             i32,
    pub name:           String,
    pub location_type:  String,
    pub address:        String,
    pub latitude:       Option<f64>,
    pub longitude:      Option<f64>,
    pub timezone:       String,
    pub contact_email:  Option<String>,
    pub contact_phone:  Option<String>,
    pub google_place_id: Option<String>,
    pub is_active:      bool,
    pub created_at:     DateTime<Utc>,
}

/// Columns to SELECT for a full `BusinessRow`.
pub const BUSINESS_COLS: &str =
    "id, location_id, soultoken_id, primary_holder_id, primary_holder_soultoken_id, \
     stripe_customer_id, name, verification_status, beacon_suspended, beacon_suspended_at, \
     suspended_at, onboarded_at, is_active, platform_fee_cents, \
     deleted_at, updated_at, created_at";

/// Columns to SELECT for a full `LocationRow`.
pub const LOCATION_COLS: &str =
    "id, name, location_type, address, \
     CAST(latitude AS FLOAT8) AS latitude, CAST(longitude AS FLOAT8) AS longitude, \
     timezone, contact_email, contact_phone, google_place_id, is_active, created_at";

// ── Request bodies ────────────────────────────────────────────────────────────

/// Request body for `POST /api/businesses`.
#[derive(Debug, Deserialize)]
pub struct CreateBusinessRequest {
    /// Business name — 1 to 100 characters.
    pub name:          String,
    /// Physical street address.
    pub address:       String,
    /// GPS latitude (decimal degrees).
    pub latitude:      Option<f64>,
    /// GPS longitude (decimal degrees).
    pub longitude:     Option<f64>,
    /// IANA timezone identifier (default: "America/Edmonton").
    pub timezone:      Option<String>,
    /// Public contact email address.
    pub contact_email: Option<String>,
    /// Public contact phone number.
    pub contact_phone: Option<String>,
}

// ── Response bodies ───────────────────────────────────────────────────────────

/// Nested location detail within a [`BusinessResponse`].
#[derive(Debug, Serialize)]
pub struct LocationResponse {
    /// Location database ID.
    pub id:        i32,
    /// Display name of the location.
    pub name:      String,
    /// Full street address.
    pub address:   String,
    /// GPS latitude.
    pub latitude:  Option<f64>,
    /// GPS longitude.
    pub longitude: Option<f64>,
    /// IANA timezone identifier.
    pub timezone:  String,
}

/// Response returned by the businesses endpoints.
#[derive(Debug, Serialize)]
pub struct BusinessResponse {
    /// Business database ID.
    pub id:                  i32,
    /// Business display name.
    pub name:                String,
    /// BFIP verification status (`pending`, `active`, `suspended`).
    pub verification_status: String,
    /// Physical location details.
    pub location:            LocationResponse,
    /// Whether this business is active on the platform.
    pub is_active:           bool,
    /// When this business record was created.
    pub created_at:          DateTime<Utc>,
}
