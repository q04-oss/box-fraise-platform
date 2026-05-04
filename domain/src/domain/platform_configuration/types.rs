#![allow(missing_docs)]
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PlatformConfigurationRow {
    pub id:                i32,
    pub key:               String,
    pub value:             String,
    pub value_type:        String,
    pub description:       String,
    pub cache_ttl_seconds: i32,
    pub updated_by:        Option<i32>,
    pub updated_at:        DateTime<Utc>,
    pub created_at:        DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PlatformConfigurationHistoryRow {
    pub id:               i32,
    pub configuration_id: i32,
    pub previous_value:   String,
    pub new_value:        String,
    pub changed_by:       i32,
    pub changed_at:       DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateConfigurationRequest {
    pub value: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlatformConfigurationResponse {
    pub key:               String,
    pub value:             String,
    pub value_type:        String,
    pub description:       String,
    pub cache_ttl_seconds: i32,
    pub updated_at:        DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PlatformConfigurationHistoryResponse {
    pub key:            String,
    pub previous_value: String,
    pub new_value:      String,
    pub changed_at:     DateTime<Utc>,
}

pub const PLATFORM_CONFIG_COLS: &str =
    "id, key, value, value_type, description, cache_ttl_seconds, \
     updated_by, updated_at, created_at";

/// BFIP Section 15 default configuration entries.
///
/// Inserted at server startup with ON CONFLICT DO NOTHING — safe to re-run.
pub const DEFAULTS: &[(&str, &str, &str, &str)] = &[
    ("cooling_period_days",              "7",     "integer", "Days from identity confirmation before cooling ends"),
    ("cooling_app_opens_required",       "3",     "integer", "App opens required on separate days during cooling"),
    ("presence_events_required",         "3",     "integer", "Qualifying presence events required for Stage 3"),
    ("presence_days_required",           "3",     "integer", "Separate calendar days required for Stage 3"),
    ("min_dwell_minutes",                "15",    "integer", "Minimum beacon dwell time in minutes"),
    ("default_rssi_threshold",           "-70",   "integer", "Default minimum RSSI in dBm"),
    ("soultoken_expiry_months",          "12",    "integer", "Soultoken validity period in months"),
    ("attestation_token_expiry_minutes", "15",    "integer", "Third-party token validity in minutes"),
    ("co_sign_deadline_hours",           "48",    "integer", "Hours reviewers have to co-sign"),
    ("platform_gift_limit_months",       "6",     "integer", "Months between platform-covered gifts"),
    ("delivery_staff_reveal_hours",      "2",     "integer", "Hours before window to reveal schedule to staff"),
    ("business_notification_hours",      "4",     "integer", "Hours before window for business notification"),
    ("background_check_expiry_months",   "12",    "integer", "Months before background check expires"),
    ("cleared_requires_all_checks",      "true",  "boolean", "All five check types required for cleared"),
];
