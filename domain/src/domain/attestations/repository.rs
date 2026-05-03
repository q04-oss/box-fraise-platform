#![allow(missing_docs)]
use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::error::{AppResult, DomainError};
use super::types::{
    AttestationAttemptRow, ReviewerAssignmentRow, VisitAttestationRow,
    ATTESTATION_COLS, ATTESTATION_ATTEMPT_COLS, REVIEWER_ASSIGNMENT_COLS,
};

// ── Attestations ──────────────────────────────────────────────────────────────

pub async fn create_attestation(
    pool:                  &PgPool,
    visit_id:              i32,
    user_id:               i32,
    staff_id:              i32,
    presence_threshold_id: i32,
    reviewer_1_id:         i32,
    reviewer_2_id:         i32,
    photo_hash:            Option<&str>,
    photo_storage_uri:     Option<&str>,
) -> AppResult<VisitAttestationRow> {
    sqlx::query_as(&format!(
        "INSERT INTO visit_attestations \
         (visit_id, user_id, staff_id, presence_threshold_id, \
          assigned_reviewer_1_id, assigned_reviewer_2_id, photo_hash, photo_storage_uri) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
         RETURNING {ATTESTATION_COLS}"
    ))
    .bind(visit_id)
    .bind(user_id)
    .bind(staff_id)
    .bind(presence_threshold_id)
    .bind(reviewer_1_id)
    .bind(reviewer_2_id)
    .bind(photo_hash)
    .bind(photo_storage_uri)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_attestation_by_id(
    pool: &PgPool,
    id:   i32,
) -> AppResult<Option<VisitAttestationRow>> {
    sqlx::query_as(&format!(
        "SELECT {ATTESTATION_COLS} FROM visit_attestations WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_attestations_by_user(
    pool:    &PgPool,
    user_id: i32,
) -> AppResult<Vec<VisitAttestationRow>> {
    sqlx::query_as(&format!(
        "SELECT {ATTESTATION_COLS} FROM visit_attestations \
         WHERE user_id = $1 ORDER BY created_at DESC"
    ))
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_pending_attestations_for_reviewer(
    pool:        &PgPool,
    reviewer_id: i32,
) -> AppResult<Vec<VisitAttestationRow>> {
    sqlx::query_as(&format!(
        "SELECT {ATTESTATION_COLS} FROM visit_attestations \
         WHERE (assigned_reviewer_1_id = $1 OR assigned_reviewer_2_id = $1) \
           AND status = 'co_sign_pending' \
         ORDER BY co_sign_deadline ASC NULLS LAST"
    ))
    .bind(reviewer_id)
    .fetch_all(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn update_attestation_staff_signed(
    pool:                   &PgPool,
    id:                     i32,
    staff_signature:        &str,
    photo_hash:             Option<&str>,
    location_confirmed:     bool,
    user_present_confirmed: bool,
    co_sign_deadline:       DateTime<Utc>,
) -> AppResult<VisitAttestationRow> {
    sqlx::query_as(&format!(
        "UPDATE visit_attestations SET \
         staff_signature           = $2, \
         photo_hash                = COALESCE($3, photo_hash), \
         location_confirmed        = $4, \
         user_present_confirmed    = $5, \
         user_identity_verified_at = now(), \
         co_sign_deadline          = $6, \
         status                    = 'co_sign_pending', \
         updated_at                = now() \
         WHERE id = $1 \
         RETURNING {ATTESTATION_COLS}"
    ))
    .bind(id)
    .bind(staff_signature)
    .bind(photo_hash)
    .bind(location_confirmed)
    .bind(user_present_confirmed)
    .bind(co_sign_deadline)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn approve_attestation(
    pool: &PgPool,
    id:   i32,
) -> AppResult<VisitAttestationRow> {
    sqlx::query_as(&format!(
        "UPDATE visit_attestations SET status = 'approved', updated_at = now() \
         WHERE id = $1 RETURNING {ATTESTATION_COLS}"
    ))
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn set_rejected(
    pool: &PgPool,
    id:   i32,
) -> AppResult<VisitAttestationRow> {
    sqlx::query_as(&format!(
        "UPDATE visit_attestations SET status = 'rejected', updated_at = now() \
         WHERE id = $1 RETURNING {ATTESTATION_COLS}"
    ))
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

// ── Attempt history ───────────────────────────────────────────────────────────

pub async fn record_attempt(
    pool:                  &PgPool,
    user_id:               i32,
    attestation_id:        i32,
    visit_id:              i32,
    reviewer_1_id:         i32,
    reviewer_2_id:         i32,
    attempt_number:        i32,
    outcome:               &str,
    rejection_reason:      Option<&str>,
    rejection_reviewer_id: Option<i32>,
) -> AppResult<AttestationAttemptRow> {
    sqlx::query_as(&format!(
        "INSERT INTO attestation_attempts \
         (user_id, attestation_id, visit_id, assigned_reviewer_1_id, assigned_reviewer_2_id, \
          attempt_number, outcome, rejection_reason, rejection_reviewer_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
         RETURNING {ATTESTATION_ATTEMPT_COLS}"
    ))
    .bind(user_id)
    .bind(attestation_id)
    .bind(visit_id)
    .bind(reviewer_1_id)
    .bind(reviewer_2_id)
    .bind(attempt_number)
    .bind(outcome)
    .bind(rejection_reason)
    .bind(rejection_reviewer_id)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

// ── Reviewer assignment log ───────────────────────────────────────────────────

pub async fn log_reviewer_assignment(
    pool:              &PgPool,
    visit_id:          i32,
    reviewer_id:       i32,
    cosign_count:      i32,
    collusion_passed:  bool,
    collusion_details: serde_json::Value,
) -> AppResult<ReviewerAssignmentRow> {
    sqlx::query_as(&format!(
        "INSERT INTO reviewer_assignment_log \
         (visit_id, reviewer_id, assignment_algorithm_version, collusion_check_passed, \
          collusion_check_details, recent_cosign_count) \
         VALUES ($1, $2, 'v1', $3, $4, $5) \
         RETURNING {REVIEWER_ASSIGNMENT_COLS}"
    ))
    .bind(visit_id)
    .bind(reviewer_id)
    .bind(collusion_passed)
    .bind(collusion_details)
    .bind(cosign_count)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

// ── Visit signatures (co-sign lifecycle) ─────────────────────────────────────

/// Record a reviewer's co-signature by inserting a row into visit_signatures.
///
/// `visit_signatures.signature` and `evidence_hash_reviewed` are NOT NULL in
/// the schema — the row is only ever created once the reviewer actually signs.
///
/// `ON CONFLICT DO NOTHING` prevents double-signing; zero rows_affected ⇒
/// the reviewer has already signed, and we return `DomainError::Conflict`.
pub async fn record_reviewer_signature(
    pool:                   &PgPool,
    visit_id:               i32,
    reviewer_id:            i32,
    deadline:               DateTime<Utc>,
    signature:              &str,
    evidence_hash_reviewed: &str,
) -> AppResult<()> {
    let result = sqlx::query(
        "INSERT INTO visit_signatures \
         (visit_id, reviewer_id, assigned_at, deadline, signature, evidence_hash_reviewed, signed_at) \
         VALUES ($1, $2, now(), $3, $4, $5, now()) \
         ON CONFLICT (visit_id, reviewer_id) DO NOTHING"
    )
    .bind(visit_id)
    .bind(reviewer_id)
    .bind(deadline)
    .bind(signature)
    .bind(evidence_hash_reviewed)
    .execute(pool)
    .await
    .map_err(DomainError::Db)?;

    if result.rows_affected() == 0 {
        return Err(DomainError::Conflict(
            "reviewer has already signed this visit".to_string(),
        ));
    }
    Ok(())
}

/// Returns true when both assigned reviewers have signed the visit.
pub async fn check_both_reviewers_signed(
    pool:          &PgPool,
    visit_id:      i32,
    reviewer_1_id: i32,
    reviewer_2_id: i32,
) -> AppResult<bool> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM visit_signatures \
         WHERE visit_id = $1 \
           AND reviewer_id IN ($2, $3) \
           AND signed_at IS NOT NULL"
    )
    .bind(visit_id)
    .bind(reviewer_1_id)
    .bind(reviewer_2_id)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)?;
    Ok(count >= 2)
}
