#![allow(missing_docs)]
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── Database rows ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct VisitAttestationRow {
    pub id:                       i32,
    pub visit_id:                 i32,
    pub user_id:                  i32,
    pub staff_id:                 i32,
    pub presence_threshold_id:    i32,
    pub assigned_reviewer_1_id:   i32,
    pub assigned_reviewer_2_id:   i32,
    pub user_present_confirmed:   bool,
    pub user_identity_verified_at: Option<DateTime<Utc>>,
    pub location_confirmed:       bool,
    pub photo_hash:               Option<String>,
    pub photo_storage_uri:        Option<String>,
    pub staff_signature:          Option<String>,
    pub co_sign_deadline:         Option<DateTime<Utc>>,
    pub status:                   String,
    pub attempt_number:           i32,
    pub updated_at:               DateTime<Utc>,
    pub created_at:               DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct AttestationAttemptRow {
    pub id:                     i32,
    pub user_id:                i32,
    pub attestation_id:         i32,
    pub visit_id:               i32,
    pub assigned_reviewer_1_id: i32,
    pub assigned_reviewer_2_id: i32,
    pub attempt_number:         i32,
    pub outcome:                String,
    pub rejection_reason:       Option<String>,
    pub rejection_reviewer_id:  Option<i32>,
    pub occurred_at:            DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct ReviewerAssignmentRow {
    pub id:                          i32,
    pub visit_id:                    i32,
    pub reviewer_id:                 i32,
    pub assigned_at:                 DateTime<Utc>,
    pub assignment_algorithm_version: String,
    pub collusion_check_passed:      bool,
    pub collusion_check_details:     serde_json::Value,
    pub recent_cosign_count:         i32,
}

// ── Column lists ──────────────────────────────────────────────────────────────

pub const ATTESTATION_COLS: &str =
    "id, visit_id, user_id, staff_id, presence_threshold_id, \
     assigned_reviewer_1_id, assigned_reviewer_2_id, user_present_confirmed, \
     user_identity_verified_at, location_confirmed, photo_hash, photo_storage_uri, \
     staff_signature, co_sign_deadline, status, attempt_number, updated_at, created_at";

pub const ATTESTATION_ATTEMPT_COLS: &str =
    "id, user_id, attestation_id, visit_id, assigned_reviewer_1_id, assigned_reviewer_2_id, \
     attempt_number, outcome, rejection_reason, rejection_reviewer_id, occurred_at";

pub const REVIEWER_ASSIGNMENT_COLS: &str =
    "id, visit_id, reviewer_id, assigned_at, assignment_algorithm_version, \
     collusion_check_passed, collusion_check_details, recent_cosign_count";

// ── Request bodies ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct InitiateAttestationRequest {
    pub visit_id:              i32,
    pub user_id:               i32,
    pub presence_threshold_id: i32,
    pub photo_hash:            Option<String>,
    pub photo_storage_uri:     Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct StaffSignAttestationRequest {
    pub staff_signature:        String,
    pub photo_hash:             Option<String>,
    pub location_confirmed:     bool,
    pub user_present_confirmed: bool,
}

#[derive(Debug, Deserialize)]
pub struct ReviewerSignAttestationRequest {
    pub signature:              String,
    pub evidence_hash_reviewed: String,
}

#[derive(Debug, Deserialize)]
pub struct RejectAttestationRequest {
    pub rejection_reason: String,
}
