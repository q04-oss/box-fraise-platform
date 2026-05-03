use chrono::Utc;
use sqlx::PgPool;

use crate::{
    audit,
    error::{AppResult, DomainError},
    event_bus::EventBus,
    events::DomainEvent,
    types::UserId,
};
use crate::domain::auth::repository as user_repo;
use super::{
    repository,
    types::{
        InitiateAttestationRequest, RejectAttestationRequest,
        ReviewerSignAttestationRequest, StaffSignAttestationRequest,
        VisitAttestationRow,
    },
};

// ── Internal helpers ──────────────────────────────────────────────────────────

/// BFIP Section 6.5 — Reviewer assignment algorithm v1.
///
/// Selects two eligible reviewers, excluding those who worked at the delivery
/// staff's location in the last 30 days, or whose pair exceeds the 7-day
/// co-sign collusion limit (>3 times).
///
/// Returns `(reviewer_1_id, reviewer_2_id, cosign_count)`.
async fn assign_reviewers_for_visit(
    pool:        &PgPool,
    staff_id:    i32,
    location_id: i32,
) -> AppResult<(i32, i32, i64)> {
    // 1. Active attestation_reviewers — not the delivery staff, not same-location in 30 days.
    let candidates: Vec<(i32,)> = sqlx::query_as(
        "SELECT DISTINCT sr.user_id
         FROM staff_roles sr
         WHERE sr.role = 'attestation_reviewer'
           AND sr.revoked_at IS NULL
           AND (sr.expires_at IS NULL OR sr.expires_at > now())
           AND sr.user_id != $1
           AND sr.user_id NOT IN (
               SELECT user_id FROM staff_roles
               WHERE location_id = $2
                 AND revoked_at IS NULL
                 AND granted_at > now() - interval '30 days'
           )
         ORDER BY sr.user_id"
    )
    .bind(staff_id)
    .bind(location_id)
    .fetch_all(pool)
    .await
    .map_err(DomainError::Db)?;

    if candidates.len() < 2 {
        return Err(DomainError::InvalidInput(
            "not enough eligible reviewers (need 2; check reviewer staffing and location exclusions)"
                .to_string(),
        ));
    }

    let ids: Vec<i32> = candidates.into_iter().map(|(id,)| id).collect();

    // 2. Find a pair where cosign_count <= 3 in the last 7 days.
    for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            let r1 = ids[i];
            let r2 = ids[j];
            let cosign: i64 = sqlx::query_scalar(
                "SELECT COUNT(*)
                 FROM visit_signatures vs1
                 JOIN visit_signatures vs2 ON vs1.visit_id = vs2.visit_id
                 WHERE vs1.reviewer_id = $1
                   AND vs2.reviewer_id = $2
                   AND vs1.signed_at IS NOT NULL
                   AND vs2.signed_at IS NOT NULL
                   AND vs1.signed_at > now() - interval '7 days'"
            )
            .bind(r1)
            .bind(r2)
            .fetch_one(pool)
            .await
            .map_err(DomainError::Db)?;

            if cosign <= 3 {
                return Ok((r1, r2, cosign));
            }
        }
    }

    Err(DomainError::InvalidInput(
        "all eligible reviewer pairs exceed the co-sign collusion limit (>3 in 7 days)".to_string(),
    ))
}

// ── Service functions ─────────────────────────────────────────────────────────

/// Initiate a staff attestation (BFIP Section 6.3).
///
/// Requesting user must be the delivery staff for the visit.
/// Target user must have `verification_status = 'presence_confirmed'`.
pub async fn initiate_attestation(
    pool:               &PgPool,
    requesting_user_id: UserId,
    req:                InitiateAttestationRequest,
    event_bus:          &EventBus,
) -> AppResult<VisitAttestationRow> {
    let uid = i32::from(requesting_user_id);

    // 1. Load visit — must be in_progress.
    let (staff_id, location_id, status): (i32, i32, String) = sqlx::query_as(
        "SELECT staff_id, location_id, status FROM staff_visits WHERE id = $1"
    )
    .bind(req.visit_id)
    .fetch_optional(pool)
    .await
    .map_err(DomainError::Db)?
    .ok_or(DomainError::NotFound)?;

    if status != "in_progress" {
        return Err(DomainError::Conflict(
            "visit must be in_progress to initiate attestation".to_string(),
        ));
    }

    // 2. Requesting user must be the assigned delivery staff.
    if staff_id != uid {
        return Err(DomainError::Forbidden);
    }

    // 3. Target user must be presence_confirmed.
    let target = user_repo::find_by_id(pool, UserId::from(req.user_id))
        .await?
        .ok_or(DomainError::NotFound)?;

    if target.verification_status != "presence_confirmed" {
        return Err(DomainError::InvalidInput(
            "user must have verification_status 'presence_confirmed' for attestation".to_string(),
        ));
    }

    // 4. Validate presence_threshold belongs to this user and is met.
    let (pt_user_id, threshold_met_at): (i32, Option<chrono::DateTime<Utc>>) =
        sqlx::query_as(
            "SELECT user_id, threshold_met_at FROM presence_thresholds WHERE id = $1"
        )
        .bind(req.presence_threshold_id)
        .fetch_optional(pool)
        .await
        .map_err(DomainError::Db)?
        .ok_or_else(|| DomainError::InvalidInput("presence_threshold not found".to_string()))?;

    if pt_user_id != req.user_id {
        return Err(DomainError::InvalidInput(
            "presence_threshold does not belong to this user".to_string(),
        ));
    }
    if threshold_met_at.is_none() {
        return Err(DomainError::InvalidInput(
            "presence threshold has not been met yet".to_string(),
        ));
    }

    // 5. No active (non-rejected) attestation for this visit + user.
    let active: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM visit_attestations \
         WHERE visit_id = $1 AND user_id = $2 AND status != 'rejected'"
    )
    .bind(req.visit_id)
    .bind(req.user_id)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)?;

    if active > 0 {
        return Err(DomainError::Conflict(
            "an active attestation already exists for this visit and user".to_string(),
        ));
    }

    // 6. Assign two eligible reviewers (BFIP Section 6.5).
    let (r1_id, r2_id, cosign_count) =
        assign_reviewers_for_visit(pool, uid, location_id).await?;

    // 7. Create attestation record.
    let attestation = repository::create_attestation(
        pool,
        req.visit_id,
        req.user_id,
        uid,
        req.presence_threshold_id,
        r1_id,
        r2_id,
        req.photo_hash.as_deref(),
        req.photo_storage_uri.as_deref(),
    ).await?;

    // 8. Log reviewer assignments to reviewer_assignment_log.
    let details = serde_json::json!({
        "same_location_30d": false,
        "cosign_7d":          cosign_count,
    });
    let _ = repository::log_reviewer_assignment(
        pool, req.visit_id, r1_id, cosign_count as i32, true, details.clone(),
    ).await;
    let _ = repository::log_reviewer_assignment(
        pool, req.visit_id, r2_id, cosign_count as i32, true, details,
    ).await;

    // 9. Audit + event.
    audit::write(
        pool,
        Some(uid),
        None,
        "attestation.initiated",
        serde_json::json!({
            "attestation_id": attestation.id,
            "user_id":        req.user_id,
            "visit_id":       req.visit_id,
        }),
    ).await;

    event_bus.publish(DomainEvent::AttestationInitiated {
        attestation_id: attestation.id,
        user_id:        req.user_id,
        visit_id:       req.visit_id,
    });

    Ok(attestation)
}

/// Record the delivery staff's signature on an attestation (BFIP Section 6.4).
///
/// Sets `status = 'co_sign_pending'` and opens the 48-hour co-sign window.
/// Inserts `visit_signatures` rows for both assigned reviewers.
pub async fn staff_sign(
    pool:               &PgPool,
    attestation_id:     i32,
    requesting_user_id: UserId,
    req:                StaffSignAttestationRequest,
    _event_bus:         &EventBus,
) -> AppResult<VisitAttestationRow> {
    let uid = i32::from(requesting_user_id);

    // 1. Load attestation — must be 'pending'.
    let attest = repository::get_attestation_by_id(pool, attestation_id)
        .await?
        .ok_or(DomainError::NotFound)?;

    if attest.status != "pending" {
        return Err(DomainError::Conflict(
            "attestation must be in 'pending' status for staff sign".to_string(),
        ));
    }

    // 2. Requesting user must be the delivery staff.
    if attest.staff_id != uid {
        return Err(DomainError::Forbidden);
    }

    // 3. Set co_sign_deadline to now() + 48 hours.
    let deadline = Utc::now() + chrono::Duration::hours(48);

    // 4. Update attestation with signature and set status to co_sign_pending.
    let updated = repository::update_attestation_staff_signed(
        pool,
        attestation_id,
        &req.staff_signature,
        req.photo_hash.as_deref(),
        req.location_confirmed,
        req.user_present_confirmed,
        deadline,
    ).await?;

    // 6. Audit event.
    audit::write(
        pool,
        Some(uid),
        None,
        "attestation.staff_signed",
        serde_json::json!({ "attestation_id": attestation_id }),
    ).await;

    Ok(updated)
}

/// Record a reviewer's co-signature on an attestation (BFIP Section 6.6).
///
/// When both assigned reviewers have signed, the attestation is approved
/// and the user is promoted to `verification_status = 'attested'`.
pub async fn reviewer_sign(
    pool:               &PgPool,
    attestation_id:     i32,
    requesting_user_id: UserId,
    req:                ReviewerSignAttestationRequest,
    event_bus:          &EventBus,
) -> AppResult<VisitAttestationRow> {
    let uid = i32::from(requesting_user_id);

    // 1. Load attestation — must be 'co_sign_pending'.
    let attest = repository::get_attestation_by_id(pool, attestation_id)
        .await?
        .ok_or(DomainError::NotFound)?;

    if attest.status != "co_sign_pending" {
        return Err(DomainError::Conflict(
            "attestation must be in 'co_sign_pending' status for reviewer sign".to_string(),
        ));
    }

    // 2. Requesting user must be an assigned reviewer.
    if attest.assigned_reviewer_1_id != uid && attest.assigned_reviewer_2_id != uid {
        return Err(DomainError::Forbidden);
    }

    // 3. Deadline must not have passed.
    if let Some(deadline) = attest.co_sign_deadline {
        if Utc::now() > deadline {
            return Err(DomainError::Conflict("co-sign deadline has passed".to_string()));
        }
    }

    // 4. Record signature — INSERT with all fields (signature col is NOT NULL).
    //    ON CONFLICT DO NOTHING guards against double-signing.
    let sign_deadline = attest
        .co_sign_deadline
        .unwrap_or_else(|| Utc::now() + chrono::Duration::hours(48));
    repository::record_reviewer_signature(
        pool,
        attest.visit_id,
        uid,
        sign_deadline,
        &req.signature,
        &req.evidence_hash_reviewed,
    ).await?;

    // 5. Check if both reviewers have now signed.
    let both_signed = repository::check_both_reviewers_signed(
        pool,
        attest.visit_id,
        attest.assigned_reviewer_1_id,
        attest.assigned_reviewer_2_id,
    ).await?;

    if both_signed {
        // 6a. Approve attestation.
        let approved = repository::approve_attestation(pool, attestation_id).await?;

        // 6b. Promote user to 'attested'.
        sqlx::query(
            "UPDATE users SET verification_status = 'attested', \
             attested_at = now(), updated_at = now() WHERE id = $1"
        )
        .bind(attest.user_id)
        .execute(pool)
        .await
        .map_err(DomainError::Db)?;

        // 6c. Record attempt.
        let _ = repository::record_attempt(
            pool,
            attest.user_id,
            attestation_id,
            attest.visit_id,
            attest.assigned_reviewer_1_id,
            attest.assigned_reviewer_2_id,
            attest.attempt_number,
            "approved",
            None,
            None,
        ).await;

        // 6d. Audit + event.
        audit::write(
            pool,
            Some(uid),
            None,
            "attestation.approved",
            serde_json::json!({
                "attestation_id": attestation_id,
                "user_id":        attest.user_id,
            }),
        ).await;

        event_bus.publish(DomainEvent::AttestationApproved {
            attestation_id,
            user_id: attest.user_id,
        });

        Ok(approved)
    } else {
        audit::write(
            pool,
            Some(uid),
            None,
            "attestation.reviewer_signed",
            serde_json::json!({ "attestation_id": attestation_id, "reviewer_id": uid }),
        ).await;

        repository::get_attestation_by_id(pool, attestation_id)
            .await?
            .ok_or(DomainError::NotFound)
    }
}

/// Reject an attestation (BFIP Section 6.7).
///
/// Only an assigned reviewer may reject. The user's status is reset to
/// `'presence_confirmed'` and the attempt is recorded in `attestation_attempts`.
pub async fn reject_attestation(
    pool:               &PgPool,
    attestation_id:     i32,
    requesting_user_id: UserId,
    req:                RejectAttestationRequest,
    event_bus:          &EventBus,
) -> AppResult<VisitAttestationRow> {
    let uid = i32::from(requesting_user_id);

    // 1. Load attestation.
    let attest = repository::get_attestation_by_id(pool, attestation_id)
        .await?
        .ok_or(DomainError::NotFound)?;

    // 2. Status must be 'pending' or 'co_sign_pending'.
    if !["pending", "co_sign_pending"].contains(&attest.status.as_str()) {
        return Err(DomainError::Conflict(
            "attestation cannot be rejected in its current status".to_string(),
        ));
    }

    // 3. Only assigned reviewers may reject.
    if attest.assigned_reviewer_1_id != uid && attest.assigned_reviewer_2_id != uid {
        return Err(DomainError::Forbidden);
    }

    // 4. Set status = 'rejected'.
    let rejected = repository::set_rejected(pool, attestation_id).await?;

    // 5. Reset user to 'presence_confirmed'.
    sqlx::query(
        "UPDATE users SET verification_status = 'presence_confirmed', updated_at = now() \
         WHERE id = $1"
    )
    .bind(attest.user_id)
    .execute(pool)
    .await
    .map_err(DomainError::Db)?;

    // 6. Record attempt.
    let _ = repository::record_attempt(
        pool,
        attest.user_id,
        attestation_id,
        attest.visit_id,
        attest.assigned_reviewer_1_id,
        attest.assigned_reviewer_2_id,
        attest.attempt_number,
        "rejected",
        Some(&req.rejection_reason),
        Some(uid),
    ).await;

    // 7. Audit + event.
    audit::write(
        pool,
        Some(uid),
        None,
        "attestation.rejected",
        serde_json::json!({
            "attestation_id":        attestation_id,
            "user_id":               attest.user_id,
            "rejection_reason":      &req.rejection_reason,
            "rejection_reviewer_id": uid,
        }),
    ).await;

    event_bus.publish(DomainEvent::AttestationRejected {
        attestation_id,
        user_id:               attest.user_id,
        rejection_reviewer_id: uid,
    });

    Ok(rejected)
}

/// List attestations in `'co_sign_pending'` status assigned to this reviewer.
pub async fn list_pending_for_reviewer(
    pool:    &PgPool,
    user_id: UserId,
) -> AppResult<Vec<VisitAttestationRow>> {
    repository::get_pending_attestations_for_reviewer(pool, i32::from(user_id)).await
}

/// List all attestations for the authenticated user (as the attested person).
pub async fn list_my_attestations(
    pool:    &PgPool,
    user_id: UserId,
) -> AppResult<Vec<VisitAttestationRow>> {
    repository::get_attestations_by_user(pool, i32::from(user_id)).await
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::staff::{
            service as staff_svc,
            types::{ArriveAtVisitRequest, GrantRoleRequest, ScheduleVisitRequest},
        },
        event_bus::EventBus,
        types::UserId,
    };
    use sqlx::PgPool;

    // ── Test context ──────────────────────────────────────────────────────────

    struct Ctx {
        admin:        UserId,
        staff:        UserId,
        reviewer_1:   UserId,
        reviewer_2:   UserId,
        visit_id:     i32,
        target:       UserId,
        threshold_id: i32,
    }

    /// Full attestation context: admin, delivery staff, 2 reviewers, in-progress
    /// visit, and a presence-confirmed target user with a met threshold.
    async fn setup(pool: &PgPool) -> Ctx {
        use fake::{Fake, faker::internet::en::SafeEmail};

        let bus = EventBus::new();

        let mk_admin = |email: String| async move {
            let (id,): (i32,) = sqlx::query_as(
                "INSERT INTO users (email, email_verified, is_platform_admin) \
                 VALUES ($1, true, true) RETURNING id",
            )
            .bind(&email)
            .fetch_one(pool)
            .await
            .unwrap();
            UserId::from(id)
        };

        let mk_user = |email: String| async move {
            let (id,): (i32,) = sqlx::query_as(
                "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id",
            )
            .bind(&email)
            .fetch_one(pool)
            .await
            .unwrap();
            UserId::from(id)
        };

        let admin = mk_admin(SafeEmail().fake::<String>()).await;
        let staff = mk_user(SafeEmail().fake::<String>()).await;

        let (loc_id,): (i32,) = sqlx::query_as(
            "INSERT INTO locations (name, location_type, address, timezone) \
             VALUES ('Attest Store', 'box_fraise_store', '1 Attest St', 'America/Edmonton') \
             RETURNING id",
        )
        .fetch_one(pool)
        .await
        .unwrap();

        let (biz_id,): (i32,) = sqlx::query_as(
            "INSERT INTO businesses (location_id, primary_holder_id, name, verification_status) \
             VALUES ($1, $2, 'Attest Biz', 'active') RETURNING id",
        )
        .bind(loc_id)
        .bind(i32::from(admin))
        .fetch_one(pool)
        .await
        .unwrap();

        // Grant delivery_staff role.
        staff_svc::grant_staff_role(
            pool,
            admin,
            GrantRoleRequest {
                user_id:      i32::from(staff),
                role:         "delivery_staff".to_owned(),
                location_id:  Some(loc_id),
                expires_at:   None,
                confirmed_by: None,
            },
            &bus,
        )
        .await
        .unwrap();

        // Schedule + arrive at visit.
        let visit = staff_svc::schedule_visit(
            pool,
            staff,
            ScheduleVisitRequest {
                location_id:              loc_id,
                visit_type:               "combined".to_owned(),
                scheduled_at:             chrono::Utc::now() + chrono::Duration::hours(1),
                window_hours:             Some(4),
                support_booking_capacity: Some(0),
                expected_box_count:       Some(0),
            },
            &bus,
        )
        .await
        .unwrap();

        staff_svc::arrive_at_visit(
            pool,
            visit.id,
            staff,
            ArriveAtVisitRequest { arrived_latitude: None, arrived_longitude: None },
        )
        .await
        .unwrap();

        // Create 2 attestation reviewers (no location — eligible everywhere).
        let r1 = mk_user(SafeEmail().fake::<String>()).await;
        let r2 = mk_user(SafeEmail().fake::<String>()).await;

        for rid in [r1, r2] {
            staff_svc::grant_staff_role(
                pool,
                admin,
                GrantRoleRequest {
                    user_id:      i32::from(rid),
                    role:         "attestation_reviewer".to_owned(),
                    location_id:  None,
                    expires_at:   None,
                    confirmed_by: None,
                },
                &bus,
            )
            .await
            .unwrap();
        }

        // Target user: presence_confirmed status.
        let (target_id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified, verification_status) \
             VALUES ($1, true, 'presence_confirmed') RETURNING id",
        )
        .bind(&SafeEmail().fake::<String>())
        .fetch_one(pool)
        .await
        .unwrap();

        // Met presence threshold for the target user.
        let (threshold_id,): (i32,) = sqlx::query_as(
            "INSERT INTO presence_thresholds \
             (user_id, business_id, event_count, days_count, threshold_met_at) \
             VALUES ($1, $2, 3, 3, now()) RETURNING id",
        )
        .bind(target_id)
        .bind(biz_id)
        .fetch_one(pool)
        .await
        .unwrap();

        Ctx {
            admin,
            staff,
            reviewer_1: r1,
            reviewer_2: r2,
            visit_id: visit.id,
            target: UserId::from(target_id),
            threshold_id,
        }
    }

    fn initiate_req(ctx: &Ctx) -> InitiateAttestationRequest {
        InitiateAttestationRequest {
            visit_id:              ctx.visit_id,
            user_id:               i32::from(ctx.target),
            presence_threshold_id: ctx.threshold_id,
            photo_hash:            Some("sha256-photo".to_owned()),
            photo_storage_uri:     Some("s3://photos/test".to_owned()),
        }
    }

    async fn run_staff_sign(pool: &PgPool, ctx: &Ctx, attestation_id: i32) -> VisitAttestationRow {
        let bus = EventBus::new();
        staff_sign(
            pool,
            attestation_id,
            ctx.staff,
            StaffSignAttestationRequest {
                staff_signature:        "staff-sig-abc".to_owned(),
                photo_hash:             None,
                location_confirmed:     true,
                user_present_confirmed: true,
            },
            &bus,
        )
        .await
        .expect("staff_sign must succeed")
    }

    async fn run_reviewer_sign(
        pool:          &PgPool,
        attestation_id: i32,
        reviewer:      UserId,
    ) -> VisitAttestationRow {
        let bus = EventBus::new();
        reviewer_sign(
            pool,
            attestation_id,
            reviewer,
            ReviewerSignAttestationRequest {
                signature:              "reviewer-sig".to_owned(),
                evidence_hash_reviewed: "evidence-hash".to_owned(),
            },
            &bus,
        )
        .await
        .expect("reviewer_sign must succeed")
    }

    // ── Tests 1–3: initiate_attestation ──────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn initiate_attestation_assigns_two_reviewers_and_creates_record(pool: PgPool) {
        let ctx = setup(&pool).await;
        let bus = EventBus::new();

        let attest = initiate_attestation(&pool, ctx.staff, initiate_req(&ctx), &bus)
            .await
            .expect("initiate_attestation must succeed");

        assert_eq!(attest.status, "pending");
        assert_eq!(attest.visit_id, ctx.visit_id);
        assert_eq!(attest.user_id, i32::from(ctx.target));
        assert_ne!(attest.assigned_reviewer_1_id, attest.assigned_reviewer_2_id);
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn initiate_attestation_fails_if_user_not_presence_confirmed(pool: PgPool) {
        let ctx = setup(&pool).await;
        let bus = EventBus::new();

        // Insert a non-presence_confirmed user.
        let (other_id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified, verification_status) \
             VALUES ('not-confirmed@test.test', true, 'identity_confirmed') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        let err = initiate_attestation(
            &pool,
            ctx.staff,
            InitiateAttestationRequest {
                visit_id:              ctx.visit_id,
                user_id:               other_id,
                presence_threshold_id: ctx.threshold_id,
                photo_hash:            None,
                photo_storage_uri:     None,
            },
            &bus,
        )
        .await
        .unwrap_err();

        assert!(
            matches!(err, DomainError::InvalidInput(_)),
            "expected InvalidInput, got: {err:?}"
        );
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn initiate_attestation_fails_with_no_eligible_reviewers(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let bus = EventBus::new();

        // Minimal setup with no attestation_reviewer roles in the DB.
        let (admin_id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified, is_platform_admin) \
             VALUES ('admin@nr.test', true, true) RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let admin = UserId::from(admin_id);

        let (staff_id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified) VALUES ('staff@nr.test', true) RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let staff = UserId::from(staff_id);

        let (loc_id,): (i32,) = sqlx::query_as(
            "INSERT INTO locations (name, location_type, address, timezone) \
             VALUES ('NR Store', 'box_fraise_store', '1 NR St', 'America/Edmonton') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        let (biz_id,): (i32,) = sqlx::query_as(
            "INSERT INTO businesses (location_id, primary_holder_id, name, verification_status) \
             VALUES ($1, $2, 'NR Biz', 'active') RETURNING id",
        )
        .bind(loc_id)
        .bind(admin_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        staff_svc::grant_staff_role(
            &pool,
            admin,
            GrantRoleRequest {
                user_id: staff_id, role: "delivery_staff".to_owned(),
                location_id: Some(loc_id), expires_at: None, confirmed_by: None,
            },
            &bus,
        )
        .await
        .unwrap();

        let visit = staff_svc::schedule_visit(
            &pool,
            staff,
            ScheduleVisitRequest {
                location_id: loc_id, visit_type: "delivery".to_owned(),
                scheduled_at: chrono::Utc::now() + chrono::Duration::hours(1),
                window_hours: Some(4), support_booking_capacity: Some(0), expected_box_count: Some(0),
            },
            &bus,
        )
        .await
        .unwrap();

        staff_svc::arrive_at_visit(
            &pool, visit.id, staff,
            ArriveAtVisitRequest { arrived_latitude: None, arrived_longitude: None },
        )
        .await
        .unwrap();

        let (target_id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified, verification_status) \
             VALUES ($1, true, 'presence_confirmed') RETURNING id",
        )
        .bind(&SafeEmail().fake::<String>())
        .fetch_one(&pool)
        .await
        .unwrap();

        let (threshold_id,): (i32,) = sqlx::query_as(
            "INSERT INTO presence_thresholds \
             (user_id, business_id, event_count, days_count, threshold_met_at) \
             VALUES ($1, $2, 3, 3, now()) RETURNING id",
        )
        .bind(target_id)
        .bind(biz_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        let err = initiate_attestation(
            &pool,
            staff,
            InitiateAttestationRequest {
                visit_id: visit.id, user_id: target_id, presence_threshold_id: threshold_id,
                photo_hash: None, photo_storage_uri: None,
            },
            &bus,
        )
        .await
        .unwrap_err();

        assert!(
            matches!(err, DomainError::InvalidInput(_)),
            "expected InvalidInput (no reviewers), got: {err:?}"
        );
    }

    // ── Tests 4–5: staff_sign ─────────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn staff_sign_sets_status_to_co_sign_pending(pool: PgPool) {
        let ctx = setup(&pool).await;
        let bus = EventBus::new();

        let attest = initiate_attestation(&pool, ctx.staff, initiate_req(&ctx), &bus)
            .await
            .unwrap();

        let signed = run_staff_sign(&pool, &ctx, attest.id).await;

        assert_eq!(signed.status, "co_sign_pending");
        assert!(signed.staff_signature.is_some());
        assert!(signed.co_sign_deadline.is_some());
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn staff_sign_fails_if_not_assigned_staff(pool: PgPool) {
        let ctx = setup(&pool).await;
        let bus = EventBus::new();

        let attest = initiate_attestation(&pool, ctx.staff, initiate_req(&ctx), &bus)
            .await
            .unwrap();

        let err = staff_sign(
            &pool,
            attest.id,
            ctx.reviewer_1,
            StaffSignAttestationRequest {
                staff_signature:        "impostor-sig".to_owned(),
                photo_hash:             None,
                location_confirmed:     true,
                user_present_confirmed: true,
            },
            &bus,
        )
        .await
        .unwrap_err();

        assert!(matches!(err, DomainError::Forbidden));
    }

    // ── Tests 6–9: reviewer_sign / approve ────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn first_reviewer_sign_keeps_co_sign_pending(pool: PgPool) {
        let ctx = setup(&pool).await;
        let bus = EventBus::new();

        let attest = initiate_attestation(&pool, ctx.staff, initiate_req(&ctx), &bus).await.unwrap();
        run_staff_sign(&pool, &ctx, attest.id).await;

        let after = run_reviewer_sign(&pool, attest.id, ctx.reviewer_1).await;
        assert_eq!(after.status, "co_sign_pending", "one reviewer signed, still pending");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn second_reviewer_sign_approves_attestation(pool: PgPool) {
        let ctx = setup(&pool).await;
        let bus = EventBus::new();

        let attest = initiate_attestation(&pool, ctx.staff, initiate_req(&ctx), &bus).await.unwrap();
        run_staff_sign(&pool, &ctx, attest.id).await;
        run_reviewer_sign(&pool, attest.id, ctx.reviewer_1).await;

        let approved = run_reviewer_sign(&pool, attest.id, ctx.reviewer_2).await;
        assert_eq!(approved.status, "approved");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn approved_attestation_promotes_user_to_attested(pool: PgPool) {
        let ctx = setup(&pool).await;
        let bus = EventBus::new();

        let attest = initiate_attestation(&pool, ctx.staff, initiate_req(&ctx), &bus).await.unwrap();
        run_staff_sign(&pool, &ctx, attest.id).await;
        run_reviewer_sign(&pool, attest.id, ctx.reviewer_1).await;
        run_reviewer_sign(&pool, attest.id, ctx.reviewer_2).await;

        let status: String = sqlx::query_scalar(
            "SELECT verification_status FROM users WHERE id = $1",
        )
        .bind(i32::from(ctx.target))
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(status, "attested");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn approved_attestation_records_attempt(pool: PgPool) {
        let ctx = setup(&pool).await;
        let bus = EventBus::new();

        let attest = initiate_attestation(&pool, ctx.staff, initiate_req(&ctx), &bus).await.unwrap();
        run_staff_sign(&pool, &ctx, attest.id).await;
        run_reviewer_sign(&pool, attest.id, ctx.reviewer_1).await;
        run_reviewer_sign(&pool, attest.id, ctx.reviewer_2).await;

        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attestation_attempts \
             WHERE attestation_id = $1 AND outcome = 'approved'",
        )
        .bind(attest.id)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(count, 1, "approved attempt must be recorded");
    }

    // ── Tests 10–11: reject_attestation ──────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn rejected_attestation_records_attempt_with_rejected_outcome(pool: PgPool) {
        let ctx = setup(&pool).await;
        let bus = EventBus::new();

        let attest = initiate_attestation(&pool, ctx.staff, initiate_req(&ctx), &bus).await.unwrap();
        run_staff_sign(&pool, &ctx, attest.id).await;

        reject_attestation(
            &pool,
            attest.id,
            ctx.reviewer_1,
            RejectAttestationRequest { rejection_reason: "identity mismatch".to_owned() },
            &bus,
        )
        .await
        .expect("reject must succeed");

        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attestation_attempts \
             WHERE attestation_id = $1 AND outcome = 'rejected'",
        )
        .bind(attest.id)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(count, 1);
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn rejected_attestation_resets_user_to_presence_confirmed(pool: PgPool) {
        let ctx = setup(&pool).await;
        let bus = EventBus::new();

        let attest = initiate_attestation(&pool, ctx.staff, initiate_req(&ctx), &bus).await.unwrap();
        run_staff_sign(&pool, &ctx, attest.id).await;

        reject_attestation(
            &pool,
            attest.id,
            ctx.reviewer_1,
            RejectAttestationRequest { rejection_reason: "photo mismatch".to_owned() },
            &bus,
        )
        .await
        .unwrap();

        let status: String = sqlx::query_scalar(
            "SELECT verification_status FROM users WHERE id = $1",
        )
        .bind(i32::from(ctx.target))
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(status, "presence_confirmed");
    }

    // ── Tests 12–13: list queries ─────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn list_pending_for_reviewer_returns_co_sign_pending(pool: PgPool) {
        let ctx = setup(&pool).await;
        let bus = EventBus::new();

        let attest = initiate_attestation(&pool, ctx.staff, initiate_req(&ctx), &bus).await.unwrap();
        run_staff_sign(&pool, &ctx, attest.id).await;

        let pending = list_pending_for_reviewer(&pool, ctx.reviewer_1).await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].status, "co_sign_pending");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn list_my_attestations_returns_own_rows(pool: PgPool) {
        let ctx = setup(&pool).await;
        let bus = EventBus::new();

        initiate_attestation(&pool, ctx.staff, initiate_req(&ctx), &bus).await.unwrap();

        let mine = list_my_attestations(&pool, ctx.target).await.unwrap();
        assert_eq!(mine.len(), 1);
        assert_eq!(mine[0].user_id, i32::from(ctx.target));
    }

    // ── Adversarial tests ─────────────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_staff_sign_others_attestation(pool: PgPool) {
        let ctx = setup(&pool).await;
        let bus = EventBus::new();

        let attest = initiate_attestation(&pool, ctx.staff, initiate_req(&ctx), &bus).await.unwrap();

        let err = staff_sign(
            &pool,
            attest.id,
            ctx.admin,
            StaffSignAttestationRequest {
                staff_signature: "forged".to_owned(),
                photo_hash:      None,
                location_confirmed:     true,
                user_present_confirmed: true,
            },
            &bus,
        )
        .await
        .unwrap_err();

        assert!(matches!(err, DomainError::Forbidden));
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_reviewer_cannot_sign_unassigned_attestation(pool: PgPool) {
        let ctx = setup(&pool).await;
        let bus = EventBus::new();

        let attest = initiate_attestation(&pool, ctx.staff, initiate_req(&ctx), &bus).await.unwrap();
        run_staff_sign(&pool, &ctx, attest.id).await;

        // staff user is not a reviewer.
        let err = reviewer_sign(
            &pool,
            attest.id,
            ctx.staff,
            ReviewerSignAttestationRequest {
                signature:              "bad".to_owned(),
                evidence_hash_reviewed: "bad".to_owned(),
            },
            &bus,
        )
        .await
        .unwrap_err();

        assert!(matches!(err, DomainError::Forbidden));
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_reject_without_reviewer_assignment(pool: PgPool) {
        let ctx = setup(&pool).await;
        let bus = EventBus::new();

        let attest = initiate_attestation(&pool, ctx.staff, initiate_req(&ctx), &bus).await.unwrap();
        run_staff_sign(&pool, &ctx, attest.id).await;

        // admin is not an assigned reviewer.
        let err = reject_attestation(
            &pool,
            attest.id,
            ctx.admin,
            RejectAttestationRequest { rejection_reason: "unauthorised".to_owned() },
            &bus,
        )
        .await
        .unwrap_err();

        assert!(matches!(err, DomainError::Forbidden));
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_initiate_for_non_presence_confirmed_user(pool: PgPool) {
        let ctx = setup(&pool).await;
        let bus = EventBus::new();

        // Try to attest the admin (whose status is 'registered').
        let err = initiate_attestation(
            &pool,
            ctx.staff,
            InitiateAttestationRequest {
                visit_id:              ctx.visit_id,
                user_id:               i32::from(ctx.admin),
                presence_threshold_id: ctx.threshold_id,
                photo_hash:            None,
                photo_storage_uri:     None,
            },
            &bus,
        )
        .await
        .unwrap_err();

        assert!(
            matches!(err, DomainError::InvalidInput(_)),
            "expected InvalidInput, got: {err:?}"
        );
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_double_sign_reviewer(pool: PgPool) {
        let ctx = setup(&pool).await;
        let bus = EventBus::new();

        let attest = initiate_attestation(&pool, ctx.staff, initiate_req(&ctx), &bus).await.unwrap();
        run_staff_sign(&pool, &ctx, attest.id).await;

        // First sign succeeds.
        run_reviewer_sign(&pool, attest.id, ctx.reviewer_1).await;

        // Second sign by the same reviewer must fail.
        let err = reviewer_sign(
            &pool,
            attest.id,
            ctx.reviewer_1,
            ReviewerSignAttestationRequest {
                signature:              "double-sig".to_owned(),
                evidence_hash_reviewed: "double-hash".to_owned(),
            },
            &bus,
        )
        .await
        .unwrap_err();

        assert!(
            matches!(err, DomainError::Conflict(_)),
            "double-sign must be Conflict, got: {err:?}"
        );
    }
}
