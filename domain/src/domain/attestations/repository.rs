#![allow(missing_docs)]
use chrono::{DateTime, Utc};
use sqlx::PgConnection;

use crate::error::{AppResult, DomainError};
use super::types::{
    AttestationAttemptRow, ReviewerAssignmentRow, VisitAttestationRow,
    ATTESTATION_COLS, ATTESTATION_ATTEMPT_COLS, REVIEWER_ASSIGNMENT_COLS,
};

// ── Attestations ──────────────────────────────────────────────────────────────

pub async fn create_attestation(
    conn:                  &mut PgConnection,
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
    .fetch_one(conn)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_attestation_by_id(
    conn: &mut PgConnection,
    id:   i32,
) -> AppResult<Option<VisitAttestationRow>> {
    sqlx::query_as(&format!(
        "SELECT {ATTESTATION_COLS} FROM visit_attestations WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(conn)
    .await
    .map_err(DomainError::Db)
}

/// Same as `get_attestation_by_id` but acquires a row-level
/// `FOR UPDATE` lock so concurrent callers serialise.
///
/// Used by `reviewer_sign` to close the race that two reviewers
/// signing simultaneously each saw a `co_sign_pending` snapshot and
/// each only saw their own signature when checking "have both signed?",
/// leaving the attestation un-approved despite both signatures landing.
/// The lock makes the second reviewer wait for the first to commit,
/// so its post-write `count(visit_signatures) >= 2` check correctly
/// observes both signatures and triggers approval.
pub async fn get_attestation_by_id_for_update(
    conn: &mut PgConnection,
    id:   i32,
) -> AppResult<Option<VisitAttestationRow>> {
    sqlx::query_as(&format!(
        "SELECT {ATTESTATION_COLS} FROM visit_attestations \
         WHERE id = $1 FOR UPDATE"
    ))
    .bind(id)
    .fetch_optional(conn)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_attestations_by_user(
    conn:    &mut PgConnection,
    user_id: i32,
) -> AppResult<Vec<VisitAttestationRow>> {
    sqlx::query_as(&format!(
        "SELECT {ATTESTATION_COLS} FROM visit_attestations \
         WHERE user_id = $1 ORDER BY created_at DESC"
    ))
    .bind(user_id)
    .fetch_all(conn)
    .await
    .map_err(DomainError::Db)
}

/// Number of `visit_attestations` rows on this visit that have the given
/// user assigned as either reviewer slot. Used by the
/// `require_visit_reader` authz helper — non-zero means the caller may read
/// the visit's evidence even if they aren't the assigned staff or admin.
pub async fn count_reviewer_assignments_for_visit(
    conn:     &mut PgConnection,
    visit_id: i32,
    user_id:  i32,
) -> AppResult<i64> {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM visit_attestations \
         WHERE visit_id = $1 \
           AND (assigned_reviewer_1_id = $2 OR assigned_reviewer_2_id = $2)"
    )
    .bind(visit_id)
    .bind(user_id)
    .fetch_one(conn)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_pending_attestations_for_reviewer(
    conn:        &mut PgConnection,
    reviewer_id: i32,
) -> AppResult<Vec<VisitAttestationRow>> {
    sqlx::query_as(&format!(
        "SELECT {ATTESTATION_COLS} FROM visit_attestations \
         WHERE (assigned_reviewer_1_id = $1 OR assigned_reviewer_2_id = $1) \
           AND status = 'co_sign_pending' \
         ORDER BY co_sign_deadline ASC NULLS LAST"
    ))
    .bind(reviewer_id)
    .fetch_all(conn)
    .await
    .map_err(DomainError::Db)
}

pub async fn update_attestation_staff_signed(
    conn:                   &mut PgConnection,
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
    .fetch_one(conn)
    .await
    .map_err(DomainError::Db)
}

pub async fn approve_attestation(
    conn: &mut PgConnection,
    id:   i32,
) -> AppResult<VisitAttestationRow> {
    sqlx::query_as(&format!(
        "UPDATE visit_attestations SET status = 'approved', updated_at = now() \
         WHERE id = $1 RETURNING {ATTESTATION_COLS}"
    ))
    .bind(id)
    .fetch_one(conn)
    .await
    .map_err(DomainError::Db)
}

pub async fn set_rejected(
    conn: &mut PgConnection,
    id:   i32,
) -> AppResult<VisitAttestationRow> {
    sqlx::query_as(&format!(
        "UPDATE visit_attestations SET status = 'rejected', updated_at = now() \
         WHERE id = $1 RETURNING {ATTESTATION_COLS}"
    ))
    .bind(id)
    .fetch_one(conn)
    .await
    .map_err(DomainError::Db)
}

// ── Attempt history ───────────────────────────────────────────────────────────

pub async fn record_attempt(
    conn:                  &mut PgConnection,
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
    .fetch_one(conn)
    .await
    .map_err(DomainError::Db)
}

// ── Reviewer assignment log ───────────────────────────────────────────────────

pub async fn log_reviewer_assignment(
    conn:              &mut PgConnection,
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
    .fetch_one(conn)
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
    conn:                   &mut PgConnection,
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
    .execute(conn)
    .await
    .map_err(DomainError::Db)?;

    if result.rows_affected() == 0 {
        return Err(DomainError::Conflict(
            "reviewer has already signed this visit".to_string(),
        ));
    }
    Ok(())
}

/// Returns the two stored signature records when both assigned reviewers have
/// signed; `None` otherwise. Each record is the raw `visit_signatures.signature`
/// value (`verifying_key_hex:signature_hex` after Hardening Section 1c) — the
/// service layer parses and re-verifies them via aggregated Ed25519.
pub async fn check_both_reviewers_signed(
    conn:          &mut PgConnection,
    visit_id:      i32,
    reviewer_1_id: i32,
    reviewer_2_id: i32,
) -> AppResult<Option<(String, String)>> {
    let rows: Vec<(i32, String)> = sqlx::query_as(
        "SELECT reviewer_id, signature FROM visit_signatures \
         WHERE visit_id = $1 \
           AND reviewer_id IN ($2, $3) \
           AND signed_at IS NOT NULL"
    )
    .bind(visit_id)
    .bind(reviewer_1_id)
    .bind(reviewer_2_id)
    .fetch_all(conn)
    .await
    .map_err(DomainError::Db)?;

    if rows.len() < 2 {
        return Ok(None);
    }
    let mut sig_1 = None;
    let mut sig_2 = None;
    for (rid, sig) in rows {
        if rid == reviewer_1_id {
            sig_1 = Some(sig);
        } else if rid == reviewer_2_id {
            sig_2 = Some(sig);
        }
    }
    match (sig_1, sig_2) {
        (Some(a), Some(b)) => Ok(Some((a, b))),
        _                  => Ok(None),
    }
}
