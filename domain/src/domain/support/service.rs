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
        CancelBookingRequest, CreateBookingRequest, ResolveBookingRequest,
        SupportBookingResponse, SupportBookingRow,
    },
};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn to_response(row: SupportBookingRow) -> SupportBookingResponse {
    SupportBookingResponse {
        id:                row.id,
        visit_id:          row.visit_id,
        priority:          row.priority,
        status:            row.status,
        issue_description: row.issue_description,
        gift_box_provided: row.gift_box_provided,
        attended_at:       row.attended_at,
        resolved_at:       row.resolved_at,
        created_at:        row.created_at,
    }
}

// ── Queries ───────────────────────────────────────────────────────────────────

/// Return all bookings for the authenticated user (BFIP Section 10).
pub async fn get_my_bookings(
    pool:    &PgPool,
    user_id: UserId,
) -> AppResult<Vec<SupportBookingResponse>> {
    let rows = repository::get_bookings_by_user(pool, i32::from(user_id)).await?;
    Ok(rows.into_iter().map(to_response).collect())
}

// ── Commands ──────────────────────────────────────────────────────────────────

/// Book a support slot at a scheduled or in-progress visit (BFIP Section 10.1).
pub async fn create_booking(
    pool:      &PgPool,
    user_id:   UserId,
    req:       CreateBookingRequest,
    event_bus: &EventBus,
) -> AppResult<SupportBookingResponse> {
    let uid = i32::from(user_id);

    // 1. User must exist and not be banned.
    let user = user_repo::find_by_id(pool, user_id)
        .await?
        .ok_or(DomainError::Unauthorized)?;
    if user.is_banned { return Err(DomainError::Forbidden); }

    // 2. Visit must exist with status scheduled or in_progress.
    let row: Option<(String, i32)> = sqlx::query_as(
        "SELECT status, support_booking_capacity FROM staff_visits WHERE id = $1"
    )
    .bind(req.visit_id)
    .fetch_optional(pool)
    .await
    .map_err(DomainError::Db)?;

    let (visit_status, capacity) = row.ok_or(DomainError::NotFound)?;
    if !["scheduled", "in_progress"].contains(&visit_status.as_str()) {
        return Err(DomainError::InvalidInput(
            "visit is not open for support bookings".to_string(),
        ));
    }

    // 3. Check capacity.
    let current_count = repository::active_booking_count_for_visit(pool, req.visit_id).await?;
    if current_count >= capacity as i64 {
        return Err(DomainError::InvalidInput("this visit is fully booked".to_string()));
    }

    // 4. Create booking (unique index handles duplicate gracefully).
    let priority = req.priority.as_deref().unwrap_or("standard");
    let booking = repository::create_booking(
        pool,
        req.visit_id,
        uid,
        req.issue_description.as_deref(),
        priority,
    ).await?;

    // 5. Mark confirmation sent (simulates push notification).
    let _ = repository::mark_confirmation_sent(pool, booking.id).await;

    // 6. Audit + event.
    audit::write(
        pool,
        Some(uid),
        None,
        "support.booking_created",
        serde_json::json!({
            "booking_id": booking.id,
            "visit_id":   req.visit_id,
            "priority":   priority,
        }),
    ).await;

    event_bus.publish(DomainEvent::SupportBookingCreated {
        booking_id: booking.id,
        user_id:    uid,
        visit_id:   req.visit_id,
    });

    Ok(to_response(booking))
}

/// Mark the user as having arrived for their support booking (BFIP Section 10.2).
///
/// Requesting user must be the delivery_staff member assigned to the visit.
pub async fn attend_booking(
    pool:               &PgPool,
    booking_id:         i32,
    requesting_user_id: UserId,
) -> AppResult<SupportBookingResponse> {
    let uid = i32::from(requesting_user_id);

    // 1. Booking must exist and be in 'booked' status.
    let booking = repository::get_booking_by_id(pool, booking_id)
        .await?
        .ok_or(DomainError::NotFound)?;

    if booking.status != "booked" {
        return Err(DomainError::Conflict(
            "booking must be in booked status to mark attended".to_string(),
        ));
    }

    // 2. Requesting user must be delivery_staff for this visit.
    let is_visit_staff: bool = sqlx::query_scalar(
        "SELECT staff_id = $1 FROM staff_visits WHERE id = $2"
    )
    .bind(uid)
    .bind(booking.visit_id)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)?;

    if !is_visit_staff {
        return Err(DomainError::Forbidden);
    }

    // 3. Mark attended.
    let updated = repository::attend_booking(pool, booking_id).await?;

    audit::write(
        pool,
        Some(uid),
        None,
        "support.booking_attended",
        serde_json::json!({
            "booking_id": booking_id,
            "user_id":    booking.user_id,
        }),
    ).await;

    Ok(to_response(updated))
}

/// Resolve a support booking with resolution notes and optional gift box (BFIP Section 10.3).
///
/// Requesting user must be the delivery_staff member assigned to the visit.
/// Gift box eligibility determines whether the platform or user covers the cost.
pub async fn resolve_booking(
    pool:               &PgPool,
    booking_id:         i32,
    requesting_user_id: UserId,
    req:                ResolveBookingRequest,
    event_bus:          &EventBus,
) -> AppResult<SupportBookingResponse> {
    let uid = i32::from(requesting_user_id);

    // 1. Booking must exist and be in 'attended' status.
    let booking = repository::get_booking_by_id(pool, booking_id)
        .await?
        .ok_or(DomainError::NotFound)?;

    if booking.status != "attended" {
        return Err(DomainError::Conflict(
            "booking must be in attended status to resolve".to_string(),
        ));
    }

    // 2. Requesting user must be delivery_staff AND the visit's assigned staff.
    let has_role: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM staff_roles \
         WHERE user_id = $1 AND role = 'delivery_staff' \
           AND revoked_at IS NULL \
           AND (expires_at IS NULL OR expires_at > now())"
    )
    .bind(uid)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)?;

    if has_role == 0 {
        return Err(DomainError::Forbidden);
    }

    let is_visit_staff: bool = sqlx::query_scalar(
        "SELECT staff_id = $1 FROM staff_visits WHERE id = $2"
    )
    .bind(uid)
    .bind(booking.visit_id)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)?;

    if !is_visit_staff {
        return Err(DomainError::Forbidden);
    }

    // 3. Resolve booking.
    let resolved = repository::resolve_booking(
        pool,
        booking_id,
        &req.resolution_description,
        &req.resolution_signature,
        uid,
        req.gift_box_provided,
    ).await?;

    // 4. Gift box logic.
    if req.gift_box_provided {
        let eligible = repository::check_platform_gift_eligible(pool, booking.user_id).await?;
        let covered_by = if eligible { "platform" } else { "user" };

        repository::record_gift_box(
            pool,
            booking.user_id,
            booking.visit_id,
            req.gift_box_id,
            "support_interaction",
            covered_by,
        ).await?;

        if eligible {
            repository::update_platform_gift_eligible_after(pool, booking.user_id).await?;

            audit::write(
                pool,
                Some(uid),
                None,
                "support.platform_gift_issued",
                serde_json::json!({
                    "booking_id": booking_id,
                    "user_id":    booking.user_id,
                    "box_id":     req.gift_box_id,
                }),
            ).await;
        }
    }

    // 5. Audit + event.
    audit::write(
        pool,
        Some(uid),
        None,
        "support.booking_resolved",
        serde_json::json!({
            "booking_id":        booking_id,
            "user_id":           booking.user_id,
            "gift_box_provided": req.gift_box_provided,
        }),
    ).await;

    event_bus.publish(DomainEvent::SupportBookingResolved {
        booking_id,
        user_id: booking.user_id,
    });

    Ok(to_response(resolved))
}

/// Cancel a support booking (BFIP Section 10.4).
///
/// The booking owner or a platform admin may cancel.
/// Bookings in 'attended' or later status cannot be cancelled.
pub async fn cancel_booking(
    pool:               &PgPool,
    booking_id:         i32,
    requesting_user_id: UserId,
    req:                CancelBookingRequest,
) -> AppResult<SupportBookingResponse> {
    let uid = i32::from(requesting_user_id);

    // 1. Booking must exist.
    let booking = repository::get_booking_by_id(pool, booking_id)
        .await?
        .ok_or(DomainError::NotFound)?;

    // 2. Must belong to user OR requesting user is platform_admin.
    if booking.user_id != uid {
        let user = user_repo::find_by_id(pool, requesting_user_id)
            .await?
            .ok_or(DomainError::Unauthorized)?;
        if !user.is_platform_admin {
            return Err(DomainError::Forbidden);
        }
    }

    // 3. Must be in 'booked' status.
    if booking.status != "booked" {
        return Err(DomainError::Conflict(
            "only bookings in booked status can be cancelled".to_string(),
        ));
    }

    // 4. Cancel.
    let cancelled = repository::cancel_booking(pool, booking_id, &req.cancellation_reason).await?;

    audit::write(
        pool,
        Some(uid),
        None,
        "support.booking_cancelled",
        serde_json::json!({
            "booking_id": booking_id,
            "reason":     &req.cancellation_reason,
        }),
    ).await;

    Ok(to_response(cancelled))
}

/// List all bookings for a visit — delivery_staff only (BFIP Section 10.5).
pub async fn list_bookings_for_visit(
    pool:               &PgPool,
    visit_id:           i32,
    requesting_user_id: UserId,
) -> AppResult<Vec<SupportBookingResponse>> {
    let uid = i32::from(requesting_user_id);

    let has_role: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM staff_roles \
         WHERE user_id = $1 AND role = 'delivery_staff' \
           AND revoked_at IS NULL \
           AND (expires_at IS NULL OR expires_at > now())"
    )
    .bind(uid)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)?;

    if has_role == 0 {
        return Err(DomainError::Forbidden);
    }

    let rows = repository::get_bookings_by_visit(pool, visit_id).await?;
    Ok(rows.into_iter().map(to_response).collect())
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

    // ── Fixtures ──────────────────────────────────────────────────────────────

    async fn create_user(pool: &PgPool, email: &str) -> UserId {
        let (id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id",
        )
        .bind(email).fetch_one(pool).await.unwrap();
        UserId::from(id)
    }

    async fn create_admin(pool: &PgPool, email: &str) -> UserId {
        let (id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified, is_platform_admin) \
             VALUES ($1, true, true) RETURNING id",
        )
        .bind(email).fetch_one(pool).await.unwrap();
        UserId::from(id)
    }

    async fn create_location(pool: &PgPool) -> i32 {
        let (id,): (i32,) = sqlx::query_as(
            "INSERT INTO locations (name, location_type, address, timezone) \
             VALUES ('Support Store', 'box_fraise_store', '1 Support St', 'America/Edmonton') \
             RETURNING id",
        )
        .fetch_one(pool).await.unwrap();
        id
    }

    /// Full setup: admin, delivery_staff + role, location, in-progress visit with capacity.
    /// Returns (admin, staff, loc_id, visit_id).
    async fn setup_with_support_visit(
        pool:     &PgPool,
        capacity: i32,
    ) -> (UserId, UserId, i32, i32) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let bus = EventBus::new();

        let admin = create_admin(pool, &SafeEmail().fake::<String>()).await;
        let staff = create_user(pool, &SafeEmail().fake::<String>()).await;
        let loc   = create_location(pool).await;

        staff_svc::grant_staff_role(
            pool, admin,
            GrantRoleRequest {
                user_id:      i32::from(staff),
                role:         "delivery_staff".to_owned(),
                location_id:  Some(loc),
                expires_at:   None,
                confirmed_by: None,
            },
            &bus,
        ).await.unwrap();

        let visit = staff_svc::schedule_visit(
            pool, staff,
            ScheduleVisitRequest {
                location_id:              loc,
                visit_type:               "support".to_owned(),
                scheduled_at:             chrono::Utc::now() + chrono::Duration::hours(1),
                window_hours:             Some(4),
                support_booking_capacity: Some(capacity),
                expected_box_count:       Some(0),
            },
            &bus,
        ).await.unwrap();

        staff_svc::arrive_at_visit(
            pool, visit.id, staff,
            ArriveAtVisitRequest { arrived_latitude: None, arrived_longitude: None },
        ).await.unwrap();

        (admin, staff, loc, visit.id)
    }

    fn booking_req(visit_id: i32) -> CreateBookingRequest {
        CreateBookingRequest {
            visit_id,
            issue_description: Some("Need help with account".to_owned()),
            priority: None,
        }
    }

    fn resolve_req(gift: bool) -> ResolveBookingRequest {
        ResolveBookingRequest {
            resolution_description: "Issue resolved in person".to_owned(),
            resolution_signature:   "staff-sig-resolve".to_owned(),
            gift_box_provided:      gift,
            gift_box_id:            None,
        }
    }

    fn cancel_req() -> CancelBookingRequest {
        CancelBookingRequest {
            cancellation_reason: "User no longer available".to_owned(),
        }
    }

    // ── Tests 1–4: create_booking ─────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn create_booking_succeeds_for_valid_user(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let bus  = EventBus::new();
        let (_, _, _, visit_id) = setup_with_support_visit(&pool, 5).await;
        let user = create_user(&pool, &SafeEmail().fake::<String>()).await;

        let resp = create_booking(&pool, user, booking_req(visit_id), &bus)
            .await.expect("create_booking must succeed");

        assert_eq!(resp.status, "booked");
        assert_eq!(resp.visit_id, visit_id);
        assert_eq!(resp.priority, "standard");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn create_booking_fails_when_visit_at_capacity(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let bus = EventBus::new();
        // capacity = 1
        let (_, _, _, visit_id) = setup_with_support_visit(&pool, 1).await;

        let user1 = create_user(&pool, &SafeEmail().fake::<String>()).await;
        let user2 = create_user(&pool, &SafeEmail().fake::<String>()).await;

        create_booking(&pool, user1, booking_req(visit_id), &bus)
            .await.expect("first booking must succeed");

        let err = create_booking(&pool, user2, booking_req(visit_id), &bus)
            .await.unwrap_err();

        assert!(matches!(err, DomainError::InvalidInput(_)),
            "at-capacity visit must be InvalidInput, got: {err:?}");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn create_booking_fails_for_duplicate_active_booking(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let bus  = EventBus::new();
        let (_, _, _, visit_id) = setup_with_support_visit(&pool, 5).await;
        let user = create_user(&pool, &SafeEmail().fake::<String>()).await;

        create_booking(&pool, user, booking_req(visit_id), &bus)
            .await.expect("first booking must succeed");

        let err = create_booking(&pool, user, booking_req(visit_id), &bus)
            .await.unwrap_err();

        assert!(matches!(err, DomainError::Conflict(_)),
            "duplicate booking must be Conflict, got: {err:?}");
    }

    // ── Tests 4–5: attend_booking ─────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn attend_booking_succeeds_for_visit_staff(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let bus    = EventBus::new();
        let (_, staff, _, visit_id) = setup_with_support_visit(&pool, 5).await;
        let user   = create_user(&pool, &SafeEmail().fake::<String>()).await;

        let booking = create_booking(&pool, user, booking_req(visit_id), &bus).await.unwrap();
        let resp = attend_booking(&pool, booking.id, staff).await.expect("attend must succeed");

        assert_eq!(resp.status, "attended");
        assert!(resp.attended_at.is_some());
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn attend_booking_fails_for_wrong_staff(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let bus      = EventBus::new();
        let (_, _, _, visit_id) = setup_with_support_visit(&pool, 5).await;
        let user     = create_user(&pool, &SafeEmail().fake::<String>()).await;
        let imposter = create_user(&pool, &SafeEmail().fake::<String>()).await;

        let booking = create_booking(&pool, user, booking_req(visit_id), &bus).await.unwrap();
        let err = attend_booking(&pool, booking.id, imposter).await.unwrap_err();

        assert!(matches!(err, DomainError::Forbidden));
    }

    // ── Tests 6–8: resolve_booking ────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn resolve_booking_succeeds_with_gift_box(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let bus  = EventBus::new();
        let (_, staff, _, visit_id) = setup_with_support_visit(&pool, 5).await;
        let user = create_user(&pool, &SafeEmail().fake::<String>()).await;

        let booking = create_booking(&pool, user, booking_req(visit_id), &bus).await.unwrap();
        attend_booking(&pool, booking.id, staff).await.unwrap();

        let resp = resolve_booking(&pool, booking.id, staff, resolve_req(true), &bus)
            .await.expect("resolve must succeed");

        assert_eq!(resp.status, "resolved");
        assert!(resp.resolved_at.is_some());
        assert!(resp.gift_box_provided);
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn resolve_booking_platform_gift_records_history(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let bus  = EventBus::new();
        let (_, staff, _, visit_id) = setup_with_support_visit(&pool, 5).await;
        let user = create_user(&pool, &SafeEmail().fake::<String>()).await;
        let uid  = i32::from(user);

        let booking = create_booking(&pool, user, booking_req(visit_id), &bus).await.unwrap();
        attend_booking(&pool, booking.id, staff).await.unwrap();
        resolve_booking(&pool, booking.id, staff, resolve_req(true), &bus).await.unwrap();

        // Gift box history row created as platform-covered.
        let covered_by: String = sqlx::query_scalar(
            "SELECT covered_by FROM gift_box_history WHERE user_id = $1 LIMIT 1"
        ).bind(uid).fetch_one(&pool).await.unwrap();
        assert_eq!(covered_by, "platform");

        // platform_gift_eligible_after set ~6 months from now.
        let eligible_after: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
            "SELECT platform_gift_eligible_after FROM users WHERE id = $1"
        ).bind(uid).fetch_one(&pool).await.unwrap();
        assert!(eligible_after > chrono::Utc::now() + chrono::Duration::days(150),
            "platform_gift_eligible_after must be ~6 months in future");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn resolve_booking_respects_6_month_gift_limit(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let bus  = EventBus::new();
        let (_, staff, _, visit_id) = setup_with_support_visit(&pool, 5).await;
        let user = create_user(&pool, &SafeEmail().fake::<String>()).await;
        let uid  = i32::from(user);

        // Force platform_gift_eligible_after into the future.
        sqlx::query(
            "UPDATE users SET platform_gift_eligible_after = now() + INTERVAL '5 months' \
             WHERE id = $1"
        ).bind(uid).execute(&pool).await.unwrap();

        let booking = create_booking(&pool, user, booking_req(visit_id), &bus).await.unwrap();
        attend_booking(&pool, booking.id, staff).await.unwrap();
        resolve_booking(&pool, booking.id, staff, resolve_req(true), &bus).await.unwrap();

        // Gift should be recorded as user-covered, not platform.
        let covered_by: String = sqlx::query_scalar(
            "SELECT covered_by FROM gift_box_history WHERE user_id = $1 LIMIT 1"
        ).bind(uid).fetch_one(&pool).await.unwrap();
        assert_eq!(covered_by, "user",
            "gift within 6-month window must be user-covered, not platform-covered");
    }

    // ── Tests 9–10: cancel_booking ────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn cancel_booking_succeeds_for_owner(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let bus  = EventBus::new();
        let (_, _, _, visit_id) = setup_with_support_visit(&pool, 5).await;
        let user = create_user(&pool, &SafeEmail().fake::<String>()).await;

        let booking = create_booking(&pool, user, booking_req(visit_id), &bus).await.unwrap();
        let resp = cancel_booking(&pool, booking.id, user, cancel_req())
            .await.expect("cancel must succeed for owner");

        assert_eq!(resp.status, "cancelled");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn cancel_booking_fails_for_attended_booking(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let bus    = EventBus::new();
        let (_, staff, _, visit_id) = setup_with_support_visit(&pool, 5).await;
        let user   = create_user(&pool, &SafeEmail().fake::<String>()).await;

        let booking = create_booking(&pool, user, booking_req(visit_id), &bus).await.unwrap();
        attend_booking(&pool, booking.id, staff).await.unwrap();

        let err = cancel_booking(&pool, booking.id, user, cancel_req()).await.unwrap_err();
        assert!(matches!(err, DomainError::Conflict(_)),
            "cancelling attended booking must be Conflict, got: {err:?}");
    }

    // ── Adversarial tests ─────────────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_cancel_another_users_booking(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let bus      = EventBus::new();
        let (_, _, _, visit_id) = setup_with_support_visit(&pool, 5).await;
        let owner    = create_user(&pool, &SafeEmail().fake::<String>()).await;
        let attacker = create_user(&pool, &SafeEmail().fake::<String>()).await;

        let booking = create_booking(&pool, owner, booking_req(visit_id), &bus).await.unwrap();
        let err = cancel_booking(&pool, booking.id, attacker, cancel_req()).await.unwrap_err();

        assert!(matches!(err, DomainError::Forbidden));
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_resolve_without_staff_role(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let bus      = EventBus::new();
        let (_, staff, _, visit_id) = setup_with_support_visit(&pool, 5).await;
        let user     = create_user(&pool, &SafeEmail().fake::<String>()).await;
        let attacker = create_user(&pool, &SafeEmail().fake::<String>()).await;

        let booking = create_booking(&pool, user, booking_req(visit_id), &bus).await.unwrap();
        attend_booking(&pool, booking.id, staff).await.unwrap();

        let err = resolve_booking(&pool, booking.id, attacker, resolve_req(false), &bus)
            .await.unwrap_err();
        assert!(matches!(err, DomainError::Forbidden));
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_book_at_capacity_visit(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let bus      = EventBus::new();
        // capacity = 1
        let (_, _, _, visit_id) = setup_with_support_visit(&pool, 1).await;
        let user1    = create_user(&pool, &SafeEmail().fake::<String>()).await;
        let attacker = create_user(&pool, &SafeEmail().fake::<String>()).await;

        create_booking(&pool, user1, booking_req(visit_id), &bus).await.unwrap();
        let err = create_booking(&pool, attacker, booking_req(visit_id), &bus)
            .await.unwrap_err();

        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_double_book_same_visit(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let bus  = EventBus::new();
        let (_, _, _, visit_id) = setup_with_support_visit(&pool, 5).await;
        let user = create_user(&pool, &SafeEmail().fake::<String>()).await;

        create_booking(&pool, user, booking_req(visit_id), &bus).await.unwrap();
        let err = create_booking(&pool, user, booking_req(visit_id), &bus)
            .await.unwrap_err();

        assert!(matches!(err, DomainError::Conflict(_)));
    }
}
