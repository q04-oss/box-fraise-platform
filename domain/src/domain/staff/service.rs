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

// ── Commands ──────────────────────────────────────────────────────────────────

/// Grant a staff role to a user (BFIP Section 6.1).
///
/// Requires the requesting user to be a platform admin.
/// Platform admin grants additionally require a `confirmed_by` from a different admin.
pub async fn grant_staff_role(
    pool:               &PgPool,
    requesting_user_id: UserId,
    req:                GrantRoleRequest,
    event_bus:          &EventBus,
) -> AppResult<StaffRoleResponse> {
    let rid = i32::from(requesting_user_id);

    // 1. Requesting user must be platform_admin.
    let requester = user_repo::find_by_id(pool, requesting_user_id)
        .await?
        .ok_or(DomainError::Unauthorized)?;
    if requester.is_banned { return Err(DomainError::Forbidden); }
    if !requester.is_platform_admin {
        return Err(DomainError::Forbidden);
    }

    // 2. Platform_admin grants require confirmed_by from a different admin.
    if req.role == "platform_admin" {
        match req.confirmed_by {
            None => return Err(DomainError::invalid_input(
                "platform_admin grants require confirmed_by from a second admin",
            )),
            Some(cb) if cb == rid => return Err(DomainError::invalid_input(
                "confirmed_by must differ from the granting admin",
            )),
            _ => {}
        }
    }

    // 3. delivery_staff requires location_id.
    let allowed_roles = ["delivery_staff", "attestation_reviewer", "platform_admin"];
    if !allowed_roles.contains(&req.role.as_str()) {
        return Err(DomainError::invalid_input(
            "role must be one of: delivery_staff, attestation_reviewer, platform_admin",
        ));
    }
    if req.role == "delivery_staff" && req.location_id.is_none() {
        return Err(DomainError::invalid_input(
            "delivery_staff role requires location_id",
        ));
    }

    // 4. No active duplicate role at the same location.
    if let Some(existing) = repository::get_active_role(pool, req.user_id, &req.role).await? {
        if existing.location_id == req.location_id {
            return Err(DomainError::conflict(
                "user already has an active role of this type at this location",
            ));
        }
    }

    // 5. Create the role.
    let role = repository::grant_role(
        pool,
        req.user_id,
        req.location_id,
        &req.role,
        rid,
        req.confirmed_by,
        req.expires_at,
    ).await?;

    // 6. Audit event.
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
pub async fn get_my_roles(
    pool:    &PgPool,
    user_id: UserId,
) -> AppResult<Vec<StaffRoleResponse>> {
    let rows = repository::get_active_roles_by_user(pool, i32::from(user_id)).await?;
    Ok(rows.into_iter().map(to_role_response).collect())
}

/// Schedule a staff visit (BFIP Section 10).
///
/// Delivery staff can only schedule at their assigned location.
/// Platform admins can schedule at any location.
pub async fn schedule_visit(
    pool:               &PgPool,
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
        let role = repository::get_active_role(pool, uid, "delivery_staff")
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
        pool,
        req.location_id,
        uid,
        &req.visit_type,
        req.scheduled_at,
        req.window_hours.unwrap_or(4),
        req.support_booking_capacity.unwrap_or(0),
        req.expected_box_count.unwrap_or(0),
    ).await?;

    // 5. Audit event.
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
    pool:               &PgPool,
    visit_id:           i32,
    requesting_user_id: UserId,
    req:                ArriveAtVisitRequest,
) -> AppResult<StaffVisitResponse> {
    let uid = i32::from(requesting_user_id);

    // 1. Visit must exist and be scheduled.
    let visit = repository::get_visit_by_id(pool, visit_id)
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
        pool,
        visit_id,
        Utc::now(),
        req.arrived_latitude,
        req.arrived_longitude,
    ).await?;

    // 4. Audit event.
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
    pool:               &PgPool,
    visit_id:           i32,
    requesting_user_id: UserId,
    req:                CompleteVisitRequest,
    event_bus:          &EventBus,
) -> AppResult<StaffVisitResponse> {
    let uid = i32::from(requesting_user_id);

    // 1. Visit must be in_progress.
    let visit = repository::get_visit_by_id(pool, visit_id)
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

    // 3. Update to completed.
    let updated = repository::update_visit_completed(
        pool,
        visit_id,
        req.actual_box_count,
        req.delivery_signature.as_deref(),
        req.evidence_hash.as_deref(),
        req.evidence_storage_uri.as_deref(),
    ).await?;

    // 4. Audit event.
    audit::write(
        pool,
        Some(uid),
        None,
        "staff.visit_completed",
        serde_json::json!({ "visit_id": visit_id, "actual_box_count": req.actual_box_count }),
    ).await;

    // 5. Publish domain event.
    event_bus.publish(DomainEvent::VisitCompleted { visit_id });

    Ok(to_visit_response(updated))
}

/// Submit a quality assessment for a business during a staff visit (BFIP Section 12.3).
pub async fn submit_quality_assessment(
    pool:               &PgPool,
    visit_id:           i32,
    requesting_user_id: UserId,
    req:                QualityAssessmentRequest,
    event_bus:          &EventBus,
) -> AppResult<QualityAssessmentRow> {
    let uid = i32::from(requesting_user_id);

    // 1. Visit must be in_progress or completed.
    let visit = repository::get_visit_by_id(pool, visit_id)
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
        pool,
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
        pool,
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
            .execute(pool).await
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
            .execute(pool).await
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
pub async fn list_visits(
    pool:    &PgPool,
    user_id: UserId,
) -> AppResult<Vec<StaffVisitResponse>> {
    let uid  = i32::from(user_id);
    let user = user_repo::find_by_id(pool, user_id).await?.ok_or(DomainError::Unauthorized)?;

    let rows = if user.is_platform_admin {
        repository::get_all_visits(pool).await?
    } else {
        repository::get_visits_by_staff(pool, uid).await?
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

    // ── Tests 1–3: grant_staff_role ───────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn grant_role_succeeds_for_platform_admin(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let admin  = create_platform_admin(&pool, &SafeEmail().fake::<String>()).await;
        let target = create_user(&pool, &SafeEmail().fake::<String>()).await;
        let loc_id = create_location(&pool).await;
        let bus    = EventBus::new();

        let resp = grant_staff_role(
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

        let err = grant_staff_role(
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

        let err = grant_staff_role(
            &pool, admin,
            grant_req(i32::from(target), "delivery_staff", None),
            &bus,
        ).await.unwrap_err();
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    // ── Tests 4–5: schedule_visit ─────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn schedule_visit_succeeds_for_delivery_staff(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let admin  = create_platform_admin(&pool, &SafeEmail().fake::<String>()).await;
        let staff  = create_user(&pool, &SafeEmail().fake::<String>()).await;
        let loc_id = create_location(&pool).await;
        let bus    = EventBus::new();

        grant_staff_role(&pool, admin, grant_req(i32::from(staff), "delivery_staff", Some(loc_id)), &bus).await.unwrap();

        let resp = schedule_visit(&pool, staff, schedule_req(loc_id), &bus)
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

        let err = schedule_visit(&pool, non_staff, schedule_req(loc_id), &bus)
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

        grant_staff_role(&pool, admin, grant_req(i32::from(staff), "delivery_staff", Some(loc_id)), &bus).await.unwrap();
        let visit = schedule_visit(&pool, staff, schedule_req(loc_id), &bus).await.unwrap();

        let resp = arrive_at_visit(
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

        grant_staff_role(&pool, admin, grant_req(i32::from(staff), "delivery_staff", Some(loc_id)), &bus).await.unwrap();
        let visit = schedule_visit(&pool, staff, schedule_req(loc_id), &bus).await.unwrap();

        let err = arrive_at_visit(
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

        grant_staff_role(&pool, admin, grant_req(i32::from(staff), "delivery_staff", Some(loc_id)), &bus).await.unwrap();
        let visit = schedule_visit(&pool, staff, schedule_req(loc_id), &bus).await.unwrap();
        arrive_at_visit(&pool, visit.id, staff, ArriveAtVisitRequest { arrived_latitude: None, arrived_longitude: None }).await.unwrap();

        let resp = complete_visit(
            &pool, visit.id, staff,
            CompleteVisitRequest { actual_box_count: 5, delivery_signature: None, evidence_hash: None, evidence_storage_uri: None },
            &bus,
        ).await.expect("complete_visit must succeed");

        assert_eq!(resp.status, "completed");
        assert_eq!(resp.actual_box_count, Some(5));
    }

    // ── Tests 9–12: submit_quality_assessment ─────────────────────────────────

    async fn setup_staff_with_visit(pool: &PgPool) -> (UserId, UserId, i32, i32, i32) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let admin   = create_platform_admin(pool, &SafeEmail().fake::<String>()).await;
        let staff   = create_user(pool, &SafeEmail().fake::<String>()).await;
        let loc_id  = create_location(pool).await;
        let biz_id  = create_business_at_location(pool, i32::from(admin), loc_id).await;
        let bus     = EventBus::new();

        grant_staff_role(pool, admin, grant_req(i32::from(staff), "delivery_staff", Some(loc_id)), &bus).await.unwrap();
        let visit = schedule_visit(pool, staff, schedule_req(loc_id), &bus).await.unwrap();
        arrive_at_visit(pool, visit.id, staff, ArriveAtVisitRequest { arrived_latitude: None, arrived_longitude: None }).await.unwrap();

        (admin, staff, loc_id, biz_id, visit.id)
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn submit_quality_assessment_pass_records_history(pool: PgPool) {
        let (_, staff, _, biz_id, visit_id) = setup_staff_with_visit(&pool).await;
        let bus = EventBus::new();

        let assessment = submit_quality_assessment(&pool, visit_id, staff, quality_req(biz_id, true), &bus)
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

        submit_quality_assessment(&pool, visit_id, staff, quality_req(biz_id, false), &bus).await.unwrap();

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
            submit_quality_assessment(&pool, visit_id, staff, quality_req(biz_id, false), &bus).await.unwrap();
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
            submit_quality_assessment(&pool, visit_id, staff, quality_req(biz_id, false), &bus).await.unwrap();
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

        let err = grant_staff_role(
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

        grant_staff_role(&pool, admin, grant_req(i32::from(staff), "delivery_staff", Some(their_loc)), &bus).await.unwrap();

        let err = schedule_visit(&pool, staff, schedule_req(other_loc), &bus).await.unwrap_err();
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

        grant_staff_role(&pool, admin, grant_req(i32::from(staff), "delivery_staff", Some(loc_id)), &bus).await.unwrap();
        let visit = schedule_visit(&pool, staff, schedule_req(loc_id), &bus).await.unwrap();

        let err = arrive_at_visit(
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

        grant_staff_role(&pool, admin, grant_req(i32::from(staff), "delivery_staff", Some(loc_id)), &bus).await.unwrap();
        let visit = schedule_visit(&pool, staff, schedule_req(loc_id), &bus).await.unwrap();
        // Visit is in 'scheduled' state — not in_progress.

        let err = complete_visit(
            &pool, visit.id, staff,
            CompleteVisitRequest { actual_box_count: 0, delivery_signature: None, evidence_hash: None, evidence_storage_uri: None },
            &bus,
        ).await.unwrap_err();
        assert!(matches!(err, DomainError::Conflict(_)),
            "completing a non-in_progress visit must be Conflict, got: {err:?}");
    }
}
