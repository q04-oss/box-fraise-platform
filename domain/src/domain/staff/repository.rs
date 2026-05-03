#![allow(missing_docs)]
use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::error::{AppResult, DomainError};
use super::types::{
    QualityAssessmentRow, StaffRoleRow, StaffVisitRow, VisitSignatureRow,
    QUALITY_ASSESSMENT_COLS, STAFF_ROLE_COLS, STAFF_VISIT_COLS, VISIT_SIGNATURE_COLS,
};

// ── Staff roles ───────────────────────────────────────────────────────────────

pub async fn grant_role(
    pool:         &PgPool,
    user_id:      i32,
    location_id:  Option<i32>,
    role:         &str,
    granted_by:   i32,
    confirmed_by: Option<i32>,
    expires_at:   Option<DateTime<Utc>>,
) -> AppResult<StaffRoleRow> {
    // When confirmed_by is provided, record confirmed_at = now().
    sqlx::query_as(&format!(
        "INSERT INTO staff_roles \
         (user_id, location_id, role, granted_by, confirmed_by, confirmed_at, expires_at) \
         VALUES ($1, $2, $3, $4, $5, CASE WHEN $5 IS NOT NULL THEN now() ELSE NULL END, $6) \
         RETURNING {STAFF_ROLE_COLS}"
    ))
    .bind(user_id)
    .bind(location_id)
    .bind(role)
    .bind(granted_by)
    .bind(confirmed_by)
    .bind(expires_at)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_active_roles_by_user(
    pool:    &PgPool,
    user_id: i32,
) -> AppResult<Vec<StaffRoleRow>> {
    sqlx::query_as(&format!(
        "SELECT {STAFF_ROLE_COLS} FROM staff_roles \
         WHERE user_id = $1 \
           AND revoked_at IS NULL \
           AND (expires_at IS NULL OR expires_at > now()) \
         ORDER BY granted_at DESC"
    ))
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_active_role(
    pool:    &PgPool,
    user_id: i32,
    role:    &str,
) -> AppResult<Option<StaffRoleRow>> {
    sqlx::query_as(&format!(
        "SELECT {STAFF_ROLE_COLS} FROM staff_roles \
         WHERE user_id = $1 \
           AND role = $2 \
           AND revoked_at IS NULL \
           AND (expires_at IS NULL OR expires_at > now()) \
         ORDER BY granted_at DESC LIMIT 1"
    ))
    .bind(user_id)
    .bind(role)
    .fetch_optional(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn revoke_role(
    pool:    &PgPool,
    role_id: i32,
) -> AppResult<StaffRoleRow> {
    sqlx::query_as(&format!(
        "UPDATE staff_roles SET revoked_at = now() WHERE id = $1 \
         RETURNING {STAFF_ROLE_COLS}"
    ))
    .bind(role_id)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

// ── Staff visits ──────────────────────────────────────────────────────────────

pub async fn create_visit(
    pool:                     &PgPool,
    location_id:              i32,
    staff_id:                 i32,
    visit_type:               &str,
    scheduled_at:             DateTime<Utc>,
    window_hours:             i32,
    support_booking_capacity: i32,
    expected_box_count:       i32,
) -> AppResult<StaffVisitRow> {
    sqlx::query_as(&format!(
        "INSERT INTO staff_visits \
         (location_id, staff_id, visit_type, scheduled_at, window_hours, \
          support_booking_capacity, expected_box_count) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) \
         RETURNING {STAFF_VISIT_COLS}"
    ))
    .bind(location_id)
    .bind(staff_id)
    .bind(visit_type)
    .bind(scheduled_at)
    .bind(window_hours)
    .bind(support_booking_capacity)
    .bind(expected_box_count)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_visit_by_id(
    pool:     &PgPool,
    visit_id: i32,
) -> AppResult<Option<StaffVisitRow>> {
    sqlx::query_as(&format!(
        "SELECT {STAFF_VISIT_COLS} FROM staff_visits WHERE id = $1"
    ))
    .bind(visit_id)
    .fetch_optional(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_visits_by_location(
    pool:        &PgPool,
    location_id: i32,
) -> AppResult<Vec<StaffVisitRow>> {
    sqlx::query_as(&format!(
        "SELECT {STAFF_VISIT_COLS} FROM staff_visits \
         WHERE location_id = $1 AND status != 'cancelled' \
         ORDER BY scheduled_at DESC"
    ))
    .bind(location_id)
    .fetch_all(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_visits_by_staff(
    pool:     &PgPool,
    staff_id: i32,
) -> AppResult<Vec<StaffVisitRow>> {
    sqlx::query_as(&format!(
        "SELECT {STAFF_VISIT_COLS} FROM staff_visits \
         WHERE staff_id = $1 AND status != 'cancelled' \
         ORDER BY scheduled_at DESC"
    ))
    .bind(staff_id)
    .fetch_all(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_all_visits(pool: &PgPool) -> AppResult<Vec<StaffVisitRow>> {
    sqlx::query_as(&format!(
        "SELECT {STAFF_VISIT_COLS} FROM staff_visits \
         WHERE status != 'cancelled' \
         ORDER BY scheduled_at DESC"
    ))
    .fetch_all(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn update_visit_arrived(
    pool:              &PgPool,
    visit_id:          i32,
    arrived_at:        DateTime<Utc>,
    arrived_latitude:  Option<f64>,
    arrived_longitude: Option<f64>,
) -> AppResult<StaffVisitRow> {
    sqlx::query_as(&format!(
        "UPDATE staff_visits SET \
         status            = 'in_progress', \
         arrived_at        = $2, \
         arrived_latitude  = $3, \
         arrived_longitude = $4, \
         staff_revealed_at = COALESCE(staff_revealed_at, now()), \
         updated_at        = now() \
         WHERE id = $1 \
         RETURNING {STAFF_VISIT_COLS}"
    ))
    .bind(visit_id)
    .bind(arrived_at)
    .bind(arrived_latitude)
    .bind(arrived_longitude)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn update_visit_completed(
    pool:                &PgPool,
    visit_id:            i32,
    actual_box_count:    i32,
    delivery_signature:  Option<&str>,
    evidence_hash:       Option<&str>,
    evidence_storage_uri: Option<&str>,
) -> AppResult<StaffVisitRow> {
    sqlx::query_as(&format!(
        "UPDATE staff_visits SET \
         status               = 'completed', \
         departed_at          = now(), \
         actual_box_count     = $2, \
         delivery_signature   = $3, \
         evidence_hash        = $4, \
         evidence_storage_uri = $5, \
         updated_at           = now() \
         WHERE id = $1 \
         RETURNING {STAFF_VISIT_COLS}"
    ))
    .bind(visit_id)
    .bind(actual_box_count)
    .bind(delivery_signature)
    .bind(evidence_hash)
    .bind(evidence_storage_uri)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn cancel_visit(
    pool:                &PgPool,
    visit_id:            i32,
    cancellation_reason: Option<&str>,
) -> AppResult<StaffVisitRow> {
    sqlx::query_as(&format!(
        "UPDATE staff_visits SET \
         status               = 'cancelled', \
         cancelled_at         = now(), \
         cancellation_reason  = $2, \
         updated_at           = now() \
         WHERE id = $1 \
         RETURNING {STAFF_VISIT_COLS}"
    ))
    .bind(visit_id)
    .bind(cancellation_reason)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

// ── Quality assessments ───────────────────────────────────────────────────────

pub async fn create_quality_assessment(
    pool:                      &PgPool,
    visit_id:                  i32,
    business_id:               i32,
    assessor_id:               i32,
    beacon_functioning:        bool,
    staff_performing_correctly: bool,
    standards_maintained:      bool,
    notes:                     Option<&str>,
) -> AppResult<QualityAssessmentRow> {
    let overall_pass = beacon_functioning && staff_performing_correctly && standards_maintained;
    sqlx::query_as(&format!(
        "INSERT INTO quality_assessments \
         (visit_id, business_id, assessor_id, beacon_functioning, \
          staff_performing_correctly, standards_maintained, overall_pass, notes) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
         RETURNING {QUALITY_ASSESSMENT_COLS}"
    ))
    .bind(visit_id)
    .bind(business_id)
    .bind(assessor_id)
    .bind(beacon_functioning)
    .bind(staff_performing_correctly)
    .bind(standards_maintained)
    .bind(overall_pass)
    .bind(notes)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

/// Insert a business_assessment_history record.
///
/// Returns the current count of failed assessments in the last 12 months
/// (including the one just inserted). Updates businesses.beacon_suspended
/// if the count reaches 3.
pub async fn record_assessment_history(
    pool:          &PgPool,
    business_id:   i32,
    assessment_id: i32,
    passed:        bool,
    beacon_id:     Option<i32>,
) -> AppResult<i64> {
    // Count existing failures before this assessment.
    let prior_fails: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM business_assessment_history \
         WHERE business_id = $1 AND passed = false \
           AND assessed_at > now() - interval '12 months'"
    )
    .bind(business_id)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)?;

    let new_fail_count = if !passed { prior_fails + 1 } else { prior_fails };

    sqlx::query(
        "INSERT INTO business_assessment_history \
         (business_id, assessment_id, passed, failed_count_at_time, beacon_id) \
         VALUES ($1, $2, $3, $4, $5)"
    )
    .bind(business_id)
    .bind(assessment_id)
    .bind(passed)
    .bind(new_fail_count)
    .bind(beacon_id)
    .execute(pool)
    .await
    .map_err(DomainError::Db)?;

    // Suspend beacon on third failure (BFIP Section 12.4).
    if !passed && new_fail_count >= 3 {
        sqlx::query(
            "UPDATE businesses SET \
             beacon_suspended = true, \
             beacon_suspended_at = now(), \
             updated_at = now() \
             WHERE id = $1"
        )
        .bind(business_id)
        .execute(pool)
        .await
        .map_err(DomainError::Db)?;
    }

    Ok(new_fail_count)
}

// ── Visit signatures ──────────────────────────────────────────────────────────

pub async fn assign_reviewer(
    pool:        &PgPool,
    visit_id:    i32,
    reviewer_id: i32,
    deadline:    DateTime<Utc>,
) -> AppResult<VisitSignatureRow> {
    sqlx::query_as(&format!(
        "INSERT INTO visit_signatures (visit_id, reviewer_id, assigned_at, deadline) \
         VALUES ($1, $2, now(), $3) \
         RETURNING {VISIT_SIGNATURE_COLS}"
    ))
    .bind(visit_id)
    .bind(reviewer_id)
    .bind(deadline)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn record_signature(
    pool:                  &PgPool,
    visit_id:              i32,
    reviewer_id:           i32,
    signature:             &str,
    evidence_hash_reviewed: &str,
) -> AppResult<VisitSignatureRow> {
    sqlx::query_as(&format!(
        "UPDATE visit_signatures SET \
         signature              = $3, \
         evidence_hash_reviewed = $4, \
         signed_at              = now() \
         WHERE visit_id = $1 AND reviewer_id = $2 \
         RETURNING {VISIT_SIGNATURE_COLS}"
    ))
    .bind(visit_id)
    .bind(reviewer_id)
    .bind(signature)
    .bind(evidence_hash_reviewed)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}
