#![allow(missing_docs)]
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── Database rows ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StaffRoleRow {
    pub id:           i32,
    pub user_id:      i32,
    pub location_id:  Option<i32>,
    pub role:         String,
    pub granted_by:   i32,
    pub confirmed_by: Option<i32>,
    pub confirmed_at: Option<DateTime<Utc>>,
    pub expires_at:   Option<DateTime<Utc>>,
    pub granted_at:   DateTime<Utc>,
    pub revoked_at:   Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StaffVisitRow {
    pub id:                      i32,
    pub location_id:             i32,
    pub staff_id:                i32,
    pub visit_type:              String,
    pub status:                  String,
    pub scheduled_at:            DateTime<Utc>,
    pub window_hours:            i32,
    pub support_booking_capacity: i32,
    pub business_notified_at:    Option<DateTime<Utc>>,
    pub staff_revealed_at:       Option<DateTime<Utc>>,
    pub arrived_at:              Option<DateTime<Utc>>,
    /// CAST(arrived_latitude AS FLOAT8)
    pub arrived_latitude:        Option<f64>,
    /// CAST(arrived_longitude AS FLOAT8)
    pub arrived_longitude:       Option<f64>,
    pub departed_at:             Option<DateTime<Utc>>,
    pub cancelled_at:            Option<DateTime<Utc>>,
    pub cancellation_reason:     Option<String>,
    pub expected_box_count:      i32,
    pub actual_box_count:        Option<i32>,
    pub delivery_signature:      Option<String>,
    pub evidence_hash:           Option<String>,
    pub evidence_storage_uri:    Option<String>,
    pub route_proof:             Option<String>,
    pub gift_box_covered:        bool,
    pub updated_at:              DateTime<Utc>,
    pub created_at:              DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct VisitSignatureRow {
    pub id:                    i32,
    pub visit_id:              i32,
    pub reviewer_id:           i32,
    pub signature:             Option<String>,
    pub evidence_hash_reviewed: Option<String>,
    pub assigned_at:           DateTime<Utc>,
    pub deadline:              DateTime<Utc>,
    pub signed_at:             Option<DateTime<Utc>>,
    pub missed_at:             Option<DateTime<Utc>>,
    pub deadline_enforced_at:  Option<DateTime<Utc>>,
    pub reassigned_reviewer_id: Option<i32>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct QualityAssessmentRow {
    pub id:                       i32,
    pub visit_id:                 i32,
    pub business_id:              i32,
    pub assessor_id:              i32,
    pub beacon_functioning:       bool,
    pub staff_performing_correctly: bool,
    pub standards_maintained:     bool,
    pub overall_pass:             bool,
    pub follow_up_required:       bool,
    pub follow_up_visit_id:       Option<i32>,
    pub notes:                    Option<String>,
    pub assessed_at:              DateTime<Utc>,
}

// ── Column lists ──────────────────────────────────────────────────────────────

pub const STAFF_ROLE_COLS: &str =
    "id, user_id, location_id, role, granted_by, confirmed_by, confirmed_at, \
     expires_at, granted_at, revoked_at";

pub const STAFF_VISIT_COLS: &str =
    "id, location_id, staff_id, visit_type, status, scheduled_at, window_hours, \
     support_booking_capacity, business_notified_at, staff_revealed_at, arrived_at, \
     CAST(arrived_latitude  AS FLOAT8) AS arrived_latitude, \
     CAST(arrived_longitude AS FLOAT8) AS arrived_longitude, \
     departed_at, cancelled_at, cancellation_reason, expected_box_count, actual_box_count, \
     delivery_signature, evidence_hash, evidence_storage_uri, route_proof, \
     gift_box_covered, updated_at, created_at";

pub const QUALITY_ASSESSMENT_COLS: &str =
    "id, visit_id, business_id, assessor_id, beacon_functioning, \
     staff_performing_correctly, standards_maintained, overall_pass, \
     follow_up_required, follow_up_visit_id, notes, assessed_at";

pub const VISIT_SIGNATURE_COLS: &str =
    "id, visit_id, reviewer_id, signature, evidence_hash_reviewed, \
     assigned_at, deadline, signed_at, missed_at, deadline_enforced_at, \
     reassigned_reviewer_id";

// ── Request bodies ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct GrantRoleRequest {
    pub user_id:      i32,
    /// delivery_staff | attestation_reviewer | platform_admin
    pub role:         String,
    pub location_id:  Option<i32>,
    pub expires_at:   Option<DateTime<Utc>>,
    /// Required for platform_admin grants — must differ from requesting user.
    pub confirmed_by: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct ScheduleVisitRequest {
    pub location_id:               i32,
    /// delivery | support | quality | combined
    pub visit_type:                String,
    pub scheduled_at:              DateTime<Utc>,
    pub window_hours:              Option<i32>,
    pub support_booking_capacity:  Option<i32>,
    pub expected_box_count:        Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct ArriveAtVisitRequest {
    pub arrived_latitude:  Option<f64>,
    pub arrived_longitude: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct CompleteVisitRequest {
    pub actual_box_count:   i32,
    pub delivery_signature: Option<String>,
    pub evidence_hash:      Option<String>,
    pub evidence_storage_uri: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct QualityAssessmentRequest {
    pub business_id:               i32,
    pub beacon_functioning:        bool,
    pub staff_performing_correctly: bool,
    pub standards_maintained:      bool,
    pub notes:                     Option<String>,
}

// ── Response bodies ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct StaffRoleResponse {
    pub id:          i32,
    pub user_id:     i32,
    pub role:        String,
    pub location_id: Option<i32>,
    pub granted_at:  DateTime<Utc>,
    pub expires_at:  Option<DateTime<Utc>>,
    pub is_active:   bool,
}

#[derive(Debug, Serialize)]
pub struct StaffVisitResponse {
    pub id:                 i32,
    pub location_id:        i32,
    pub visit_type:         String,
    pub status:             String,
    pub scheduled_at:       DateTime<Utc>,
    pub window_hours:       i32,
    pub arrived_at:         Option<DateTime<Utc>>,
    pub departed_at:        Option<DateTime<Utc>>,
    pub expected_box_count: i32,
    pub actual_box_count:   Option<i32>,
    pub gift_box_covered:   bool,
    pub created_at:         DateTime<Utc>,
}
