use chrono::Utc;
use sqlx::PgPool;

use crate::{
    audit,
    error::{AppResult, DomainError},
    event_bus::EventBus,
    events::DomainEvent,
    transaction::{AdminRlsTransaction, RlsTransaction},
    types::UserId,
};
use crate::domain::auth::repository as user_repo;
use super::{
    repository,
    types::{
        ArriveAtVisitRequest, CompleteVisitRequest, GrantRoleRequest,
        QualityAssessmentRequest, QualityAssessmentRow,
        ScheduleVisitRequest, StaffRoleResponse, StaffVisitResponse, StaffVisitRow,
        StaffRoleRow,
    },
};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn to_role_response(row: StaffRoleRow) -> StaffRoleResponse {
    let is_active = row.revoked_at.is_none()
        && row.expires_at.map(|e| e > Utc::now()).unwrap_or(true);
    StaffRoleResponse {
        id:          row.id,
        user_id:     row.user_id,
        role:        row.role,
        location_id: row.location_id,
        granted_at:  row.granted_at,
        expires_at:  row.expires_at,
        is_active,
    }
}

fn to_visit_response(row: StaffVisitRow) -> StaffVisitResponse {
    StaffVisitResponse {
        id:                 row.id,
        location_id:        row.location_id,
        visit_type:         row.visit_type,
        status:             row.status,
        scheduled_at:       row.scheduled_at,
        window_hours:       row.window_hours,
        arrived_at:         row.arrived_at,
        departed_at:        row.departed_at,
        expected_box_count: row.expected_box_count,
        actual_box_count:   row.actual_box_count,
        gift_box_covered:   row.gift_box_covered,
        created_at:         row.created_at,
    }
}

/// SHA-256 hex format check: 64 chars, all lowercase hex.
/// Used to validate client-supplied evidence/photo hashes — pairs with
/// the canonical server-side `StorageClient::compute_evidence_hash` which
/// produces strings of exactly this shape.
fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

// ── Commands ──────────────────────────────────────────────────────────────────

/// Grant a staff role to a user (BFIP Section 6.1).
///
/// Requires the requesting user to be a platform admin. Runs under
/// `AdminRlsTransaction` (cleanup #3) so the per-tx `app.is_admin`
/// setting is in place for any RLS-protected reads/writes.
///
/// `staff_roles` is for **operational** roles only (`delivery_staff`,
/// `attestation_reviewer`). Platform-admin status lives on
/// `users.is_platform_admin` — see `docs/ACCESS_CONTROL_MATRIX.md` Section 5.
/// Attempting to grant `platform_admin` here is rejected at the service
/// layer; the database CHECK constraint added in migration 008 is a
/// defence-in-depth backstop.
pub async fn grant_staff_role(
    tx:                 &mut AdminRlsTransaction,
    pool:               &PgPool, // pool: for audit writes + cross-domain user_repo reads (auth domain not yet migrated)
    requesting_user_id: UserId,
    req:                GrantRoleRequest,
    event_bus:          &EventBus,
) -> AppResult<StaffRoleResponse> {
    let rid = i32::from(requesting_user_id);

    // 1. Requesting user must be platform_admin (boolean column — sole
    //    enforcement path; a staff_roles row would be authoritative-looking
    //    but inert).
    //
    // Cross-domain read into the auth domain — auth repository has not yet
    // been migrated to the `&mut PgConnection` shape, so we keep the
    // `&pool` call here per the cleanup #3 rollout rule.
    let requester = user_repo::find_by_id(pool, requesting_user_id)
        .await?
        .ok_or(DomainError::Unauthorized)?;
    if requester.is_banned { return Err(DomainError::Forbidden); }
    if !requester.is_platform_admin {
        return Err(DomainError::Forbidden);
    }

    // 2. platform_admin is not a valid staff_roles role — promotion to
    //    admin must go through the admin user-management API
    //    (toggling users.is_platform_admin). Reject explicitly and
    //    early so the error message is actionable.
    if req.role == "platform_admin" {
        return Err(DomainError::invalid_input(
            "platform_admin status is set via users.is_platform_admin, \
             not staff_roles. Use the admin user-management API.",
        ));
    }

    // 3. delivery_staff requires location_id.
    let allowed_roles = ["delivery_staff", "attestation_reviewer"];
    if !allowed_roles.contains(&req.role.as_str()) {
        return Err(DomainError::invalid_input(
            "role must be one of: delivery_staff, attestation_reviewer",
        ));
    }
    if req.role == "delivery_staff" && req.location_id.is_none() {
        return Err(DomainError::invalid_input(
            "delivery_staff role requires location_id",
        ));
    }

    // 4. No active duplicate role at the same location.
    if let Some(existing) = repository::get_active_role(tx.as_mut(), req.user_id, &req.role).await? {
        if existing.location_id == req.location_id {
            return Err(DomainError::conflict(
                "user already has an active role of this type at this location",
            ));
        }
    }

    // 5. Create the role.
    let role = repository::grant_role(
        tx.as_mut(),
        req.user_id,
        req.location_id,
        &req.role,
        rid,
        req.confirmed_by,
        req.expires_at,
    ).await?;

    // 6. Audit event.
    // Audit writes use `pool` (separate connection) — they commit
    // independently so the audit row lands even if `tx` is rolled back.
    audit::write(
        pool,
        Some(rid),
        None,
        "staff.role_granted",
        serde_json::json!({ "user_id": req.user_id, "role": &role.role }),
    ).await;

    // 7. Publish domain event.
    event_bus.publish(DomainEvent::StaffRoleGranted {
        user_id: role.user_id,
        role:    role.role.clone(),
    });

    Ok(to_role_response(role))
}

/// List all active staff roles for the requesting user.
///
/// Read-only, no audit — kept on `&PgPool` per cleanup #3 rule. Acquires
/// a pool connection internally to satisfy the repository's
/// `&mut PgConnection` signature.
pub async fn get_my_roles(
    pool:    &PgPool,
    user_id: UserId,
) -> AppResult<Vec<StaffRoleResponse>> {
    let mut conn = pool.acquire().await.map_err(DomainError::Db)?;
    let rows = repository::get_active_roles_by_user(&mut conn, i32::from(user_id)).await?;
    Ok(rows.into_iter().map(to_role_response).collect())
}

/// Schedule a staff visit (BFIP Section 10).
///
/// Delivery staff can only schedule at their assigned location.
/// Platform admins can schedule at any location.
pub async fn schedule_visit(
    tx:                 &mut RlsTransaction,
    pool:               &PgPool, // pool: for audit writes + cross-domain user_repo reads (auth domain not yet migrated)
    requesting_user_id: UserId,
    req:                ScheduleVisitRequest,
    event_bus:          &EventBus,
) -> AppResult<StaffVisitResponse> {
    let uid = i32::from(requesting_user_id);

    // 1. Requesting user must have delivery_staff or platform_admin role.
    let user = user_repo::find_by_id(pool, requesting_user_id)
        .await?
        .ok_or(DomainError::Unauthorized)?;
    if user.is_banned { return Err(DomainError::Forbidden); }

    if !user.is_platform_admin {
        let role = repository::get_active_role(tx.as_mut(), uid, "delivery_staff")
            .await?
            .ok_or(DomainError::Forbidden)?;

        // 2. delivery_staff can only schedule at their assigned location.
        if role.location_id != Some(req.location_id) {
            return Err(DomainError::Forbidden);
        }
    }

    // 3. Validate visit_type.
    let allowed_types = ["delivery", "support", "quality", "combined"];
    if !allowed_types.contains(&req.visit_type.as_str()) {
        return Err(DomainError::invalid_input(
            "visit_type must be one of: delivery, support, quality, combined",
        ));
    }

    // 4. Create visit.
    let visit = repository::create_visit(
        tx.as_mut(),
        req.location_id,
        uid,
        &req.visit_type,
        req.scheduled_at,
        req.window_hours.unwrap_or(4),
        req.support_booking_capacity.unwrap_or(0),
        req.expected_box_count.unwrap_or(0),
    ).await?;

    // 5. Audit event.
    // Audit writes use `pool` (separate connection) — they commit
    // independently so the audit row lands even if `tx` is rolled back.
    audit::write(
        pool,
        Some(uid),
        None,
        "staff.visit_scheduled",
        serde_json::json!({ "visit_id": visit.id, "location_id": visit.location_id }),
    ).await;

    // 6. Publish domain event.
    event_bus.publish(DomainEvent::VisitScheduled {
        visit_id:    visit.id,
        location_id: visit.location_id,
    });

    Ok(to_visit_response(visit))
}

/// Record arrival at a scheduled visit (sets status to in_progress).
pub async fn arrive_at_visit(
    tx:                 &mut RlsTransaction,
    pool:               &PgPool, // pool: for audit writes only — audit is outside the transaction so it lands even on rollback
    visit_id:           i32,
    requesting_user_id: UserId,
    req:                ArriveAtVisitRequest,
) -> AppResult<StaffVisitResponse> {
    let uid = i32::from(requesting_user_id);

    // 1. Visit must exist and be scheduled.
    let visit = repository::get_visit_by_id(tx.as_mut(), visit_id)
        .await?
        .ok_or(DomainError::NotFound)?;

    if visit.status != "scheduled" {
        return Err(DomainError::conflict("visit is not in scheduled status"));
    }

    // 2. Requesting user must be the assigned staff.
    if visit.staff_id != uid {
        return Err(DomainError::Forbidden);
    }

    // 3. Update to in_progress.
    let updated = repository::update_visit_arrived(
        tx.as_mut(),
        visit_id,
        Utc::now(),
        req.arrived_latitude,
        req.arrived_longitude,
    ).await?;

    // 4. Audit event.
    // Audit writes use `pool` (separate connection) — they commit
    // independently so the audit row lands even if `tx` is rolled back.
    audit::write(
        pool,
        Some(uid),
        None,
        "staff.visit_arrived",
        serde_json::json!({ "visit_id": visit_id }),
    ).await;

    Ok(to_visit_response(updated))
}

/// Mark a visit completed with box count and evidence.
pub async fn complete_visit(
    tx:                 &mut RlsTransaction,
    pool:               &PgPool, // pool: for audit writes + cross-domain user_repo reads (auth domain not yet migrated)
    visit_id:           i32,
    requesting_user_id: UserId,
    req:                CompleteVisitRequest,
    event_bus:          &EventBus,
) -> AppResult<StaffVisitResponse> {
    let uid = i32::from(requesting_user_id);

    // 1. Visit must be in_progress.
    let visit = repository::get_visit_by_id(tx.as_mut(), visit_id)
        .await?
        .ok_or(DomainError::NotFound)?;

    if visit.status != "in_progress" {
        return Err(DomainError::conflict("visit must be in_progress to complete"));
    }

    // 2. Requesting user must be the assigned staff.
    if visit.staff_id != uid {
        let user = user_repo::find_by_id(pool, requesting_user_id).await?.ok_or(DomainError::Unauthorized)?;
        if !user.is_platform_admin {
            return Err(DomainError::Forbidden);
        }
    }

    // 3. Validate evidence hash + URI invariants (Hardening cleanup #5).
    //
    // Full hash verification (download from S3 + recompute) is deferred —
    // the upload endpoint at `POST /api/staff/visits/:id/evidence` already
    // computes the hash server-side via
    // `StorageClient::compute_evidence_hash`, so we validate format and
    // presence only here.
    //
    // TODO(hardening): consider storing the server-computed hash at upload
    // time (e.g. in a `visit_evidence` table keyed by storage URI) and
    // looking it up by `evidence_storage_uri` instead of trusting the
    // client-provided value at all.
    if req.evidence_storage_uri.is_some() && req.evidence_hash.is_none() {
        return Err(DomainError::invalid_input(
            "evidence_hash is required when evidence_storage_uri is provided",
        ));
    }
    if let Some(hash) = req.evidence_hash.as_deref() {
        if !is_sha256_hex(hash) {
            return Err(DomainError::invalid_input(
                "evidence_hash must be a 64-character lowercase hex string (SHA-256)",
            ));
        }
    }

    // 4. Update to completed.
    let updated = repository::update_visit_completed(
        tx.as_mut(),
        visit_id,
        req.actual_box_count,
        req.delivery_signature.as_deref(),
        req.evidence_hash.as_deref(),
        req.evidence_storage_uri.as_deref(),
    ).await?;

    // 5. Audit event.
    // Audit writes use `pool` (separate connection) — they commit
    // independently so the audit row lands even if `tx` is rolled back.
    audit::write(
        pool,
        Some(uid),
        None,
        "staff.visit_completed",
        serde_json::json!({ "visit_id": visit_id, "actual_box_count": req.actual_box_count }),
    ).await;

    // 6. Publish domain event.
    event_bus.publish(DomainEvent::VisitCompleted { visit_id });

    Ok(to_visit_response(updated))
}

/// Submit a quality assessment for a business during a staff visit (BFIP Section 12.3).
pub async fn submit_quality_assessment(
    tx:                 &mut RlsTransaction,
    pool:               &PgPool, // pool: for audit writes + cross-domain user_repo reads (auth domain not yet migrated)
    visit_id:           i32,
    requesting_user_id: UserId,
    req:                QualityAssessmentRequest,
    event_bus:          &EventBus,
) -> AppResult<QualityAssessmentRow> {
    let uid = i32::from(requesting_user_id);

    // 1. Visit must be in_progress or completed.
    let visit = repository::get_visit_by_id(tx.as_mut(), visit_id)
        .await?
        .ok_or(DomainError::NotFound)?;

    if !["in_progress", "completed"].contains(&visit.status.as_str()) {
        return Err(DomainError::conflict(
            "visit must be in_progress or completed for quality assessment",
        ));
    }

    // 2. Requesting user must be the visit's staff or platform admin.
    if visit.staff_id != uid {
        let user = user_repo::find_by_id(pool, requesting_user_id).await?.ok_or(DomainError::Unauthorized)?;
        if !user.is_platform_admin {
            return Err(DomainError::Forbidden);
        }
    }

    // 3. Create quality assessment.
    let assessment = repository::create_quality_assessment(
        tx.as_mut(),
        visit_id,
        req.business_id,
        uid,
        req.beacon_functioning,
        req.staff_performing_correctly,
        req.standards_maintained,
        req.notes.as_deref(),
    ).await?;

    // 4. Record in history and get current failure count.
    let fail_count = repository::record_assessment_history(
        tx.as_mut(),
        req.business_id,
        assessment.id,
        assessment.overall_pass,
        None,
    ).await?;

    // 5. Handle failure thresholds (BFIP Section 12.3, 12.4).
    if !assessment.overall_pass {
        if fail_count == 2 {
            if let Err(e) = sqlx::query(
                "INSERT INTO verification_events \
                 (user_id, event_type, reference_type, reference_id, actor_id, metadata) \
                 VALUES ($1, 'business_approaching_suspension', 'business', $2, $3, $4)"
            )
            .bind(uid).bind(req.business_id).bind(uid)
            .bind(serde_json::json!({ "fail_count": fail_count }))
            .execute(tx.as_mut()).await
            {
                tracing::error!(error = %e, "verification_events (business_approaching_suspension) failed");
            }
        }
        if fail_count >= 3 {
            if let Err(e) = sqlx::query(
                "INSERT INTO verification_events \
                 (user_id, event_type, reference_type, reference_id, actor_id, metadata) \
                 VALUES ($1, 'business_suspended', 'business', $2, $3, $4)"
            )
            .bind(uid).bind(req.business_id).bind(uid)
            .bind(serde_json::json!({ "fail_count": fail_count }))
            .execute(tx.as_mut()).await
            {
                tracing::error!(error = %e, "verification_events (business_suspended) failed");
            }

            audit::write(
                pool,
                Some(uid),
                None,
                "business.beacon_suspended",
                serde_json::json!({ "business_id": req.business_id, "fail_count": fail_count }),
            ).await;
        }
    }

    // 6. Audit event.
    // Audit writes use `pool` (separate connection) — they commit
    // independently so the audit row lands even if `tx` is rolled back.
    audit::write(
        pool,
        Some(uid),
        None,
        "staff.quality_assessment_submitted",
        serde_json::json!({
            "visit_id":    visit_id,
            "business_id": req.business_id,
            "overall_pass": assessment.overall_pass,
        }),
    ).await;

    // 7. Publish domain event.
    event_bus.publish(DomainEvent::QualityAssessmentSubmitted {
        visit_id,
        business_id: req.business_id,
        overall_pass: assessment.overall_pass,
    });

    Ok(assessment)
}

/// List visits — platform admins see all, delivery staff see their own.
///
/// Read-only, no audit — kept on `&PgPool` per cleanup #3 rule. Acquires
/// a pool connection internally to satisfy the repository's
/// `&mut PgConnection` signature.
pub async fn list_visits(
    pool:    &PgPool,
    user_id: UserId,
) -> AppResult<Vec<StaffVisitResponse>> {
    let uid  = i32::from(user_id);
    let user = user_repo::find_by_id(pool, user_id).await?.ok_or(DomainError::Unauthorized)?;

    let mut conn = pool.acquire().await.map_err(DomainError::Db)?;
    let rows = if user.is_platform_admin {
        repository::get_all_visits(&mut conn).await?
    } else {
        repository::get_visits_by_staff(&mut conn, uid).await?
    };

    Ok(rows.into_iter().map(to_visit_response).collect())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{event_bus::EventBus, types::UserId};
    use sqlx::PgPool;

    // ── Fixtures ──────────────────────────────────────────────────────────────

    async fn create_platform_admin(pool: &PgPool, email: &str) -> UserId {
        let (id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified, is_platform_admin) \
             VALUES ($1, true, true) RETURNING id"
        )
        .bind(email).fetch_one(pool).await.unwrap();
        UserId::from(id)
    }

    async fn create_user(pool: &PgPool, email: &str) -> UserId {
        let (id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id"
        )
        .bind(email).fetch_one(pool).await.unwrap();
        UserId::from(id)
    }

    async fn create_location(pool: &PgPool) -> i32 {
        let (id,): (i32,) = sqlx::query_as(
            "INSERT INTO locations (name, location_type, address, timezone) \
             VALUES ('Test Store', 'box_fraise_store', '1 Main St', 'America/Edmonton') \
             RETURNING id"
        )
        .fetch_one(pool).await.unwrap();
        id
    }

    async fn create_business_at_location(pool: &PgPool, owner_id: i32, location_id: i32) -> i32 {
        let (id,): (i32,) = sqlx::query_as(
            "INSERT INTO businesses (location_id, primary_holder_id, name, verification_status) \
             VALUES ($1, $2, 'Test Business', 'active') RETURNING id"
        )
        .bind(location_id).bind(owner_id)
        .fetch_one(pool).await.unwrap();
        id
    }

    fn grant_req(user_id: i32, role: &str, location_id: Option<i32>) -> GrantRoleRequest {
        GrantRoleRequest {
            user_id,
            role:         role.to_owned(),
            location_id,
            expires_at:   None,
            confirmed_by: None,
        }
    }

    fn schedule_req(location_id: i32) -> ScheduleVisitRequest {
        ScheduleVisitRequest {
            location_id,
            visit_type:               "delivery".to_owned(),
            scheduled_at:             chrono::Utc::now() + chrono::Duration::hours(2),
            window_hours:             Some(4),
            support_booking_capacity: Some(0),
            expected_box_count:       Some(5),
        }
    }

    fn quality_req(business_id: i32, pass: bool) -> QualityAssessmentRequest {
        QualityAssessmentRequest {
            business_id,
            beacon_functioning:        pass,
            staff_performing_correctly: pass,
            standards_maintained:      pass,
            notes:                     None,
        }
    }

    // ── Tx-driver helpers — open the right wrapper, call the service, commit ─

    async fn call_grant_staff_role(
        pool:               &PgPool,
        requesting_user_id: UserId,
        req:                GrantRoleRequest,
        event_bus:          &EventBus,
    ) -> AppResult<StaffRoleResponse> {
        let mut tx = AdminRlsTransaction::begin(pool).await?;
        let resp = grant_staff_role(&mut tx, pool, requesting_user_id, req, event_bus).await?;
        tx.commit().await?;
        Ok(resp)
    }

    async fn call_schedule_visit(
        pool:               &PgPool,
        requesting_user_id: UserId,
        req:                ScheduleVisitRequest,
        event_bus:          &EventBus,
    ) -> AppResult<StaffVisitResponse> {
        let mut tx = RlsTransaction::begin(pool, i32::from(requesting_user_id)).await?;
        let resp = schedule_visit(&mut tx, pool, requesting_user_id, req, event_bus).await?;
        tx.commit().await?;
        Ok(resp)
    }

    async fn call_arrive_at_visit(
        pool:               &PgPool,
        visit_id:           i32,
        requesting_user_id: UserId,
        req:                ArriveAtVisitRequest,
    ) -> AppResult<StaffVisitResponse> {
        let mut tx = RlsTransaction::begin(pool, i32::from(requesting_user_id)).await?;
        let resp = arrive_at_visit(&mut tx, pool, visit_id, requesting_user_id, req).await?;
        tx.commit().await?;
        Ok(resp)
    }

    async fn call_complete_visit(
        pool:               &PgPool,
        visit_id:           i32,
        requesting_user_id: UserId,
        req:                CompleteVisitRequest,
        event_bus:          &EventBus,
    ) -> AppResult<StaffVisitResponse> {
        let mut tx = RlsTransaction::begin(pool, i32::from(requesting_user_id)).await?;
        let resp = complete_visit(&mut tx, pool, visit_id, requesting_user_id, req, event_bus).await?;
        tx.commit().await?;
        Ok(resp)
    }

    async fn call_submit_quality_assessment(
        pool:               &PgPool,
        visit_id:           i32,
        requesting_user_id: UserId,
        req:                QualityAssessmentRequest,
        event_bus:          &EventBus,
    ) -> AppResult<QualityAssessmentRow> {
        let mut tx = RlsTransaction::begin(pool, i32::from(requesting_user_id)).await?;
        let resp = submit_quality_assessment(&mut tx, pool, visit_id, requesting_user_id, req, event_bus).await?;
        tx.commit().await?;
        Ok(resp)
    }

    // ── Tests 1–3: grant_staff_role ───────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn grant_role_succeeds_for_platform_admin(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let admin  = create_platform_admin(&pool, &SafeEmail().fake::<String>()).await;
        let target = create_user(&pool, &SafeEmail().fake::<String>()).await;
        let loc_id = create_location(&pool).await;
        let bus    = EventBus::new();

        let resp = call_grant_staff_role(
            &pool, admin,
            grant_req(i32::from(target), "delivery_staff", Some(loc_id)),
            &bus,
        ).await.expect("platform_admin must be able to grant roles");

        assert_eq!(resp.role, "delivery_staff");
        assert_eq!(resp.location_id, Some(loc_id));
        assert!(resp.is_active);
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn grant_role_fails_for_non_admin(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let non_admin = create_user(&pool, &SafeEmail().fake::<String>()).await;
        let target    = create_user(&pool, &SafeEmail().fake::<String>()).await;
        let loc_id    = create_location(&pool).await;
        let bus       = EventBus::new();

        let err = call_grant_staff_role(
            &pool, non_admin,
            grant_req(i32::from(target), "delivery_staff", Some(loc_id)),
            &bus,
        ).await.unwrap_err();
        assert!(matches!(err, DomainError::Forbidden));
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn grant_role_fails_delivery_staff_without_location(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let admin  = create_platform_admin(&pool, &SafeEmail().fake::<String>()).await;
        let target = create_user(&pool, &SafeEmail().fake::<String>()).await;
        let bus    = EventBus::new();

        let err = call_grant_staff_role(
            &pool, admin,
            grant_req(i32::from(target), "delivery_staff", None),
            &bus,
        ).await.unwrap_err();
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    // ── Hardening cleanup #1: platform_admin path consolidation ──────────────

    /// `grant_staff_role` must reject the `platform_admin` role string at
    /// the service layer, with an actionable error message that points
    /// callers at `users.is_platform_admin`. Backstop: even if the
    /// service check is bypassed, migration 008's CHECK constraint
    /// rejects the INSERT — verified in the second assertion.
    #[sqlx::test(migrations = "../server/migrations")]
    async fn grant_role_rejects_platform_admin_role(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let admin  = create_platform_admin(&pool, &SafeEmail().fake::<String>()).await;
        let target = create_user(&pool, &SafeEmail().fake::<String>()).await;
        let bus    = EventBus::new();

        // Service-layer rejection: actionable InvalidInput.
        let err = call_grant_staff_role(
            &pool, admin,
            grant_req(i32::from(target), "platform_admin", None),
            &bus,
        ).await.unwrap_err();
        match err {
            DomainError::InvalidInput(msg) => {
                assert!(
                    msg.contains("is_platform_admin"),
                    "error message must point at the boolean column, got: {msg}"
                );
            }
            other => panic!("expected InvalidInput, got {other:?}"),
        }

        // Defence-in-depth: the database CHECK constraint must also reject
        // a direct INSERT of role='platform_admin', so a raw-SQL bypass of
        // the service can't smuggle the role back in.
        let raw = sqlx::query(
            "INSERT INTO staff_roles (user_id, role, granted_by) VALUES ($1, 'platform_admin', $2)"
        )
        .bind(i32::from(target))
        .bind(i32::from(admin))
        .execute(&pool).await;
        assert!(
            raw.is_err(),
            "staff_roles_role_check CHECK constraint must reject role='platform_admin' \
             — got Ok({raw:?}); migration 008 may have regressed"
        );
    }

    /// Proves `users.is_platform_admin` is the **sole** enforcement path:
    /// flipping the boolean (with no `staff_roles` row at all) is
    /// sufficient to authorize an admin-only action; conversely, the
    /// absence of the boolean denies even a user that holds operational
    /// `staff_roles` rows.
    #[sqlx::test(migrations = "../server/migrations")]
    async fn platform_admin_check_uses_is_platform_admin_column(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let bus    = EventBus::new();
        let loc_id = create_location(&pool).await;
        let target = create_user(&pool, &SafeEmail().fake::<String>()).await;

        // Case A — boolean true, zero staff_roles rows: must authorize.
        let admin_via_boolean =
            create_platform_admin(&pool, &SafeEmail().fake::<String>()).await;
        let row_count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM staff_roles WHERE user_id = $1"
        )
        .bind(i32::from(admin_via_boolean))
        .fetch_one(&pool).await.unwrap();
        assert_eq!(row_count.0, 0, "fixture must not insert any staff_roles row");

        call_grant_staff_role(
            &pool, admin_via_boolean,
            grant_req(i32::from(target), "delivery_staff", Some(loc_id)),
            &bus,
        ).await.expect("boolean is_platform_admin alone must authorize");

        // Case B — boolean false but holds operational staff_roles rows:
        // must NOT authorize. Seed a delivery_staff and an
        // attestation_reviewer row directly so we don't re-use the service.
        let pseudo = create_user(&pool, &SafeEmail().fake::<String>()).await;
        let granter = create_platform_admin(&pool, &SafeEmail().fake::<String>()).await;
        sqlx::query(
            "INSERT INTO staff_roles (user_id, location_id, role, granted_by) \
             VALUES ($1, $2, 'delivery_staff', $3), ($1, NULL, 'attestation_reviewer', $3)"
        )
        .bind(i32::from(pseudo))
        .bind(loc_id)
        .bind(i32::from(granter))
        .execute(&pool).await.unwrap();

        let target2 = create_user(&pool, &SafeEmail().fake::<String>()).await;
        let err = call_grant_staff_role(
            &pool, pseudo,
            grant_req(i32::from(target2), "delivery_staff", Some(loc_id)),
            &bus,
        ).await.unwrap_err();
        assert!(
            matches!(err, DomainError::Forbidden),
            "operational staff_roles rows must NOT confer admin authority \
             — only users.is_platform_admin does; got {err:?}"
        );
    }

    // ── Tests 4–5: schedule_visit ─────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn schedule_visit_succeeds_for_delivery_staff(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let admin  = create_platform_admin(&pool, &SafeEmail().fake::<String>()).await;
        let staff  = create_user(&pool, &SafeEmail().fake::<String>()).await;
        let loc_id = create_location(&pool).await;
        let bus    = EventBus::new();

        call_grant_staff_role(&pool, admin, grant_req(i32::from(staff), "delivery_staff", Some(loc_id)), &bus).await.unwrap();

        let resp = call_schedule_visit(&pool, staff, schedule_req(loc_id), &bus)
            .await.expect("delivery_staff must be able to schedule visit");

        assert_eq!(resp.status, "scheduled");
        assert_eq!(resp.location_id, loc_id);
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn schedule_visit_fails_for_non_staff(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let non_staff = create_user(&pool, &SafeEmail().fake::<String>()).await;
        let loc_id    = create_location(&pool).await;
        let bus       = EventBus::new();

        let err = call_schedule_visit(&pool, non_staff, schedule_req(loc_id), &bus)
            .await.unwrap_err();
        assert!(matches!(err, DomainError::Forbidden));
    }

    // ── Tests 6–7: arrive_at_visit ────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn arrive_at_visit_updates_status_to_in_progress(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let admin  = create_platform_admin(&pool, &SafeEmail().fake::<String>()).await;
        let staff  = create_user(&pool, &SafeEmail().fake::<String>()).await;
        let loc_id = create_location(&pool).await;
        let bus    = EventBus::new();

        call_grant_staff_role(&pool, admin, grant_req(i32::from(staff), "delivery_staff", Some(loc_id)), &bus).await.unwrap();
        let visit = call_schedule_visit(&pool, staff, schedule_req(loc_id), &bus).await.unwrap();

        let resp = call_arrive_at_visit(
            &pool, visit.id, staff,
            ArriveAtVisitRequest { arrived_latitude: Some(53.5461), arrived_longitude: Some(-113.4938) },
        ).await.expect("arrive must succeed");

        assert_eq!(resp.status, "in_progress");
        assert!(resp.arrived_at.is_some());
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn arrive_at_visit_fails_for_wrong_staff_member(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let admin  = create_platform_admin(&pool, &SafeEmail().fake::<String>()).await;
        let staff  = create_user(&pool, &SafeEmail().fake::<String>()).await;
        let other  = create_user(&pool, &SafeEmail().fake::<String>()).await;
        let loc_id = create_location(&pool).await;
        let bus    = EventBus::new();

        call_grant_staff_role(&pool, admin, grant_req(i32::from(staff), "delivery_staff", Some(loc_id)), &bus).await.unwrap();
        let visit = call_schedule_visit(&pool, staff, schedule_req(loc_id), &bus).await.unwrap();

        let err = call_arrive_at_visit(
            &pool, visit.id, other,
            ArriveAtVisitRequest { arrived_latitude: None, arrived_longitude: None },
        ).await.unwrap_err();
        assert!(matches!(err, DomainError::Forbidden));
    }

    // ── Test 8: complete_visit ────────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn complete_visit_updates_status_to_completed(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let admin  = create_platform_admin(&pool, &SafeEmail().fake::<String>()).await;
        let staff  = create_user(&pool, &SafeEmail().fake::<String>()).await;
        let loc_id = create_location(&pool).await;
        let bus    = EventBus::new();

        call_grant_staff_role(&pool, admin, grant_req(i32::from(staff), "delivery_staff", Some(loc_id)), &bus).await.unwrap();
        let visit = call_schedule_visit(&pool, staff, schedule_req(loc_id), &bus).await.unwrap();
        call_arrive_at_visit(&pool, visit.id, staff, ArriveAtVisitRequest { arrived_latitude: None, arrived_longitude: None }).await.unwrap();

        let resp = call_complete_visit(
            &pool, visit.id, staff,
            CompleteVisitRequest { actual_box_count: 5, delivery_signature: None, evidence_hash: None, evidence_storage_uri: None },
            &bus,
        ).await.expect("complete_visit must succeed");

        assert_eq!(resp.status, "completed");
        assert_eq!(resp.actual_box_count, Some(5));
    }

    /// Hardening cleanup #5: a client supplying `evidence_storage_uri`
    /// without an accompanying `evidence_hash` must be rejected — without
    /// the hash there's no integrity anchor for the uploaded object.
    #[sqlx::test(migrations = "../server/migrations")]
    async fn complete_visit_rejects_uri_without_hash(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let admin  = create_platform_admin(&pool, &SafeEmail().fake::<String>()).await;
        let staff  = create_user(&pool, &SafeEmail().fake::<String>()).await;
        let loc_id = create_location(&pool).await;
        let bus    = EventBus::new();

        call_grant_staff_role(&pool, admin, grant_req(i32::from(staff), "delivery_staff", Some(loc_id)), &bus).await.unwrap();
        let visit = call_schedule_visit(&pool, staff, schedule_req(loc_id), &bus).await.unwrap();
        call_arrive_at_visit(&pool, visit.id, staff, ArriveAtVisitRequest { arrived_latitude: None, arrived_longitude: None }).await.unwrap();

        let err = call_complete_visit(
            &pool, visit.id, staff,
            CompleteVisitRequest {
                actual_box_count:     1,
                delivery_signature:   None,
                evidence_hash:        None,
                evidence_storage_uri: Some("evidence/visits/1/abc".to_owned()),
            },
            &bus,
        ).await.unwrap_err();
        match err {
            DomainError::InvalidInput(msg) =>
                assert!(msg.contains("evidence_hash"), "msg should mention evidence_hash, got: {msg}"),
            other => panic!("expected InvalidInput, got {other:?}"),
        }
    }

    /// Hardening cleanup #5: an `evidence_hash` that isn't a 64-char
    /// lowercase hex string is structurally invalid SHA-256 — reject it
    /// at the service layer rather than persisting garbage.
    #[sqlx::test(migrations = "../server/migrations")]
    async fn complete_visit_rejects_invalid_hash_format(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let admin  = create_platform_admin(&pool, &SafeEmail().fake::<String>()).await;
        let staff  = create_user(&pool, &SafeEmail().fake::<String>()).await;
        let loc_id = create_location(&pool).await;
        let bus    = EventBus::new();

        call_grant_staff_role(&pool, admin, grant_req(i32::from(staff), "delivery_staff", Some(loc_id)), &bus).await.unwrap();
        let visit = call_schedule_visit(&pool, staff, schedule_req(loc_id), &bus).await.unwrap();
        call_arrive_at_visit(&pool, visit.id, staff, ArriveAtVisitRequest { arrived_latitude: None, arrived_longitude: None }).await.unwrap();

        // Three flavours of badness — wrong length, uppercase, and non-hex.
        for bad in ["not_hex", "ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789", "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"] {
            let err = call_complete_visit(
                &pool, visit.id, staff,
                CompleteVisitRequest {
                    actual_box_count:     1,
                    delivery_signature:   None,
                    evidence_hash:        Some(bad.to_owned()),
                    evidence_storage_uri: None,
                },
                &bus,
            ).await.unwrap_err();
            assert!(
                matches!(err, DomainError::InvalidInput(ref m) if m.contains("64-character")),
                "input {bad:?} must be rejected as malformed SHA-256, got {err:?}"
            );
        }
    }

    // ── Tests 9–12: submit_quality_assessment ─────────────────────────────────

    async fn setup_staff_with_visit(pool: &PgPool) -> (UserId, UserId, i32, i32, i32) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let admin   = create_platform_admin(pool, &SafeEmail().fake::<String>()).await;
        let staff   = create_user(pool, &SafeEmail().fake::<String>()).await;
        let loc_id  = create_location(pool).await;
        let biz_id  = create_business_at_location(pool, i32::from(admin), loc_id).await;
        let bus     = EventBus::new();

        call_grant_staff_role(pool, admin, grant_req(i32::from(staff), "delivery_staff", Some(loc_id)), &bus).await.unwrap();
        let visit = call_schedule_visit(pool, staff, schedule_req(loc_id), &bus).await.unwrap();
        call_arrive_at_visit(pool, visit.id, staff, ArriveAtVisitRequest { arrived_latitude: None, arrived_longitude: None }).await.unwrap();

        (admin, staff, loc_id, biz_id, visit.id)
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn submit_quality_assessment_pass_records_history(pool: PgPool) {
        let (_, staff, _, biz_id, visit_id) = setup_staff_with_visit(&pool).await;
        let bus = EventBus::new();

        let assessment = call_submit_quality_assessment(&pool, visit_id, staff, quality_req(biz_id, true), &bus)
            .await.expect("quality assessment must succeed");

        assert!(assessment.overall_pass);

        let hist_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM business_assessment_history WHERE business_id = $1"
        ).bind(biz_id).fetch_one(&pool).await.unwrap();
        assert_eq!(hist_count, 1);
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn submit_quality_assessment_fail_increments_count(pool: PgPool) {
        let (_, staff, _, biz_id, visit_id) = setup_staff_with_visit(&pool).await;
        let bus = EventBus::new();

        call_submit_quality_assessment(&pool, visit_id, staff, quality_req(biz_id, false), &bus).await.unwrap();

        let fail_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM business_assessment_history WHERE business_id = $1 AND passed = false"
        ).bind(biz_id).fetch_one(&pool).await.unwrap();
        assert_eq!(fail_count, 1);
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn submit_quality_assessment_third_fail_suspends_beacon(pool: PgPool) {
        let (_, staff, _, biz_id, visit_id) = setup_staff_with_visit(&pool).await;
        let bus = EventBus::new();

        for _ in 0..3 {
            call_submit_quality_assessment(&pool, visit_id, staff, quality_req(biz_id, false), &bus).await.unwrap();
        }

        let suspended: bool = sqlx::query_scalar(
            "SELECT beacon_suspended FROM businesses WHERE id = $1"
        ).bind(biz_id).fetch_one(&pool).await.unwrap();
        assert!(suspended, "business.beacon_suspended must be true after 3 failing assessments");

        let ve_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM verification_events WHERE event_type = 'business_suspended'"
        ).fetch_one(&pool).await.unwrap();
        assert!(ve_count >= 1, "business_suspended verification_event must be written");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn submit_quality_assessment_second_fail_triggers_approaching(pool: PgPool) {
        let (_, staff, _, biz_id, visit_id) = setup_staff_with_visit(&pool).await;
        let bus = EventBus::new();

        for _ in 0..2 {
            call_submit_quality_assessment(&pool, visit_id, staff, quality_req(biz_id, false), &bus).await.unwrap();
        }

        let ve_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM verification_events WHERE event_type = 'business_approaching_suspension'"
        ).fetch_one(&pool).await.unwrap();
        assert!(ve_count >= 1, "business_approaching_suspension event must be written after 2nd failure");

        let still_active: bool = sqlx::query_scalar(
            "SELECT NOT beacon_suspended FROM businesses WHERE id = $1"
        ).bind(biz_id).fetch_one(&pool).await.unwrap();
        assert!(still_active, "business must NOT be suspended after only 2 failures");
    }

    // ── Adversarial tests ─────────────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_grant_role_without_admin_privileges(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let attacker = create_user(&pool, &SafeEmail().fake::<String>()).await;
        let target   = create_user(&pool, &SafeEmail().fake::<String>()).await;
        let loc_id   = create_location(&pool).await;
        let bus      = EventBus::new();

        let err = call_grant_staff_role(
            &pool, attacker,
            grant_req(i32::from(target), "delivery_staff", Some(loc_id)),
            &bus,
        ).await.unwrap_err();
        assert!(matches!(err, DomainError::Forbidden));
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_schedule_visit_at_different_location(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let admin      = create_platform_admin(&pool, &SafeEmail().fake::<String>()).await;
        let staff      = create_user(&pool, &SafeEmail().fake::<String>()).await;
        let their_loc  = create_location(&pool).await;
        let other_loc  = create_location(&pool).await;
        let bus        = EventBus::new();

        call_grant_staff_role(&pool, admin, grant_req(i32::from(staff), "delivery_staff", Some(their_loc)), &bus).await.unwrap();

        let err = call_schedule_visit(&pool, staff, schedule_req(other_loc), &bus).await.unwrap_err();
        assert!(matches!(err, DomainError::Forbidden),
            "delivery_staff must not schedule at a different location, got: {err:?}");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_arrive_at_another_staffs_visit(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let admin     = create_platform_admin(&pool, &SafeEmail().fake::<String>()).await;
        let staff     = create_user(&pool, &SafeEmail().fake::<String>()).await;
        let attacker  = create_user(&pool, &SafeEmail().fake::<String>()).await;
        let loc_id    = create_location(&pool).await;
        let bus       = EventBus::new();

        call_grant_staff_role(&pool, admin, grant_req(i32::from(staff), "delivery_staff", Some(loc_id)), &bus).await.unwrap();
        let visit = call_schedule_visit(&pool, staff, schedule_req(loc_id), &bus).await.unwrap();

        let err = call_arrive_at_visit(
            &pool, visit.id, attacker,
            ArriveAtVisitRequest { arrived_latitude: None, arrived_longitude: None },
        ).await.unwrap_err();
        assert!(matches!(err, DomainError::Forbidden));
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_complete_visit_not_in_progress(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let admin  = create_platform_admin(&pool, &SafeEmail().fake::<String>()).await;
        let staff  = create_user(&pool, &SafeEmail().fake::<String>()).await;
        let loc_id = create_location(&pool).await;
        let bus    = EventBus::new();

        call_grant_staff_role(&pool, admin, grant_req(i32::from(staff), "delivery_staff", Some(loc_id)), &bus).await.unwrap();
        let visit = call_schedule_visit(&pool, staff, schedule_req(loc_id), &bus).await.unwrap();
        // Visit is in 'scheduled' state — not in_progress.

        let err = call_complete_visit(
            &pool, visit.id, staff,
            CompleteVisitRequest { actual_box_count: 0, delivery_signature: None, evidence_hash: None, evidence_storage_uri: None },
            &bus,
        ).await.unwrap_err();
        assert!(matches!(err, DomainError::Conflict(_)),
            "completing a non-in_progress visit must be Conflict, got: {err:?}");
    }
}
