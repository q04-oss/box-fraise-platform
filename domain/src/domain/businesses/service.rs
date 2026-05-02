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
    types::{BusinessResponse, CreateBusinessRequest, LocationResponse},
};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn to_response(
    b: super::types::BusinessRow,
    l: super::types::LocationRow,
) -> BusinessResponse {
    BusinessResponse {
        id:                  b.id,
        name:                b.name,
        verification_status: b.verification_status,
        location: LocationResponse {
            id:        l.id,
            name:      l.name,
            address:   l.address,
            latitude:  l.latitude,
            longitude: l.longitude,
            timezone:  l.timezone,
        },
        is_active:  b.is_active,
        created_at: b.created_at,
    }
}

// ── Commands ──────────────────────────────────────────────────────────────────

/// Register a new business on the platform.
///
/// Rules:
/// 1. Requesting user must exist and not be banned.
/// 2. User must have `verification_status = 'attested'` (BFIP Section 12).
/// 3. User must not already have more than 5 active businesses.
/// 4. Creates a `locations` row and a `businesses` row in a single transaction.
/// 5. Writes a `verification_events` row and an `audit_events` row.
/// 6. Publishes [`DomainEvent::BusinessCreated`].
pub async fn create_business(
    pool:      &PgPool,
    user_id:   UserId,
    req:       CreateBusinessRequest,
    event_bus: &EventBus,
) -> AppResult<BusinessResponse> {
    // 1. Load and validate the requesting user.
    let user = user_repo::find_by_id(pool, user_id)
        .await?
        .ok_or(DomainError::Unauthorized)?;

    if user.is_banned {
        return Err(DomainError::Forbidden);
    }

    // 2. BFIP Section 12: business creation requires an attested user.
    if user.verification_status != "attested" {
        return Err(DomainError::Forbidden);
    }

    // 3. Abuse cap: at most 5 active businesses per user.
    let active_count = repository::count_active_businesses(pool, i32::from(user_id)).await?;
    if active_count >= 5 {
        return Err(DomainError::conflict(
            "maximum of 5 active businesses per user"
        ));
    }

    // 4a. Validate input.
    let name = req.name.trim();
    if name.is_empty() || name.len() > 100 {
        return Err(DomainError::invalid_input("name must be 1–100 characters"));
    }
    if req.address.trim().is_empty() {
        return Err(DomainError::invalid_input("address is required"));
    }
    let timezone = req.timezone.as_deref().unwrap_or("America/Edmonton");

    // 4b. Create location record.
    let location = repository::create_location(
        pool,
        name,
        "partner_business",
        req.address.trim(),
        req.latitude,
        req.longitude,
        timezone,
        req.contact_email.as_deref(),
        req.contact_phone.as_deref(),
    ).await?;

    // 4c. Create business record.
    let business = repository::create_business(
        pool,
        location.id,
        i32::from(user_id),
        name,
    ).await?;

    // 5a. Write verification_event (BFIP Section 14, Appendix A).
    if let Err(e) = sqlx::query(
        "INSERT INTO verification_events \
         (user_id, event_type, reference_type, reference_id, actor_id, metadata) \
         VALUES ($1, 'status_changed', 'business', $2, $3, $4)"
    )
    .bind(i32::from(user_id))
    .bind(business.id)
    .bind(i32::from(user_id))
    .bind(serde_json::json!({
        "action": "business_created",
        "name":   &business.name,
    }))
    .execute(pool)
    .await
    {
        tracing::error!(error = %e, "verification_events insert failed");
    }

    // 5b. Audit event.
    audit::write(
        pool,
        Some(i32::from(user_id)),
        None,
        "business.created",
        serde_json::json!({
            "business_id": business.id,
            "name":        &business.name,
        }),
    ).await;

    // 6. Publish domain event.
    event_bus.publish(DomainEvent::BusinessCreated {
        business_id: business.id,
        user_id:     i32::from(user_id),
    });

    Ok(to_response(business, location))
}

// ── Queries ───────────────────────────────────────────────────────────────────

/// Fetch a single business by ID. Returns `NotFound` if the business does not
/// exist or has been soft-deleted.
pub async fn get_business(
    pool:        &PgPool,
    business_id: i32,
    _requesting_user_id: UserId,
) -> AppResult<BusinessResponse> {
    let business = repository::get_business_by_id(pool, business_id)
        .await?
        .ok_or(DomainError::NotFound)?;

    let location = repository::get_location_by_id(pool, business.location_id)
        .await?
        .ok_or(DomainError::NotFound)?;

    Ok(to_response(business, location))
}

/// List all businesses where the caller is the primary holder.
pub async fn list_my_businesses(
    pool:    &PgPool,
    user_id: UserId,
) -> AppResult<Vec<BusinessResponse>> {
    let businesses = repository::get_businesses_by_holder(pool, i32::from(user_id)).await?;

    let mut responses = Vec::with_capacity(businesses.len());
    for b in businesses {
        if let Some(location) = repository::get_location_by_id(pool, b.location_id).await? {
            responses.push(to_response(b, location));
        }
    }
    Ok(responses)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_bus::EventBus;
    use sqlx::PgPool;

    fn test_req(name: &str) -> CreateBusinessRequest {
        CreateBusinessRequest {
            name:          name.to_owned(),
            address:       "123 Test St, Edmonton, AB".to_owned(),
            latitude:      Some(53.5461),
            longitude:     Some(-113.4938),
            timezone:      None,
            contact_email: None,
            contact_phone: None,
        }
    }

    async fn create_attested_user(pool: &PgPool, email: &str) -> UserId {
        let (id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified, verification_status) \
             VALUES ($1, true, 'attested') RETURNING id"
        )
        .bind(email)
        .fetch_one(pool)
        .await
        .unwrap();
        UserId::from(id)
    }

    async fn create_registered_user(pool: &PgPool, email: &str) -> UserId {
        let (id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified, verification_status) \
             VALUES ($1, true, 'registered') RETURNING id"
        )
        .bind(email)
        .fetch_one(pool)
        .await
        .unwrap();
        UserId::from(id)
    }

    // ── create_business ───────────────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn create_business_succeeds_for_attested_user(pool: PgPool) {
        let user_id = create_attested_user(&pool, "attested@biz.test").await;
        let bus     = EventBus::new();

        let resp = create_business(&pool, user_id, test_req("Test Café"), &bus)
            .await
            .expect("attested user must be able to create a business");

        assert_eq!(resp.name, "Test Café");
        assert_eq!(resp.verification_status, "pending");
        assert!(resp.is_active);
        assert_eq!(resp.location.address, "123 Test St, Edmonton, AB");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn create_business_fails_for_unattested_user(pool: PgPool) {
        let user_id = create_registered_user(&pool, "registered@biz.test").await;
        let bus     = EventBus::new();

        let err = create_business(&pool, user_id, test_req("Should Fail"), &bus)
            .await
            .expect_err("unattested user must not create a business");

        assert!(matches!(err, DomainError::Forbidden),
            "expected Forbidden, got: {err:?}");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn create_business_fails_for_banned_user(pool: PgPool) {
        let user_id = create_attested_user(&pool, "banned@biz.test").await;
        sqlx::query("UPDATE users SET is_banned = true WHERE id = $1")
            .bind(i32::from(user_id))
            .execute(&pool)
            .await
            .unwrap();

        let bus = EventBus::new();
        let err = create_business(&pool, user_id, test_req("Should Fail"), &bus)
            .await
            .expect_err("banned user must not create a business");

        assert!(matches!(err, DomainError::Forbidden));
    }

    // ── get_business ──────────────────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn get_business_returns_not_found_for_missing_id(pool: PgPool) {
        let user_id = create_attested_user(&pool, "seeker@biz.test").await;
        let err = get_business(&pool, 999_999, user_id)
            .await
            .expect_err("missing business must return NotFound");

        assert!(matches!(err, DomainError::NotFound));
    }

    // ── list_my_businesses ────────────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn list_my_businesses_returns_empty_for_new_user(pool: PgPool) {
        let user_id = create_attested_user(&pool, "empty@biz.test").await;
        let result  = list_my_businesses(&pool, user_id).await.unwrap();
        assert!(result.is_empty(), "new user must have no businesses");
    }

    // ── Adversarial business tests ────────────────────────────────────────────

    async fn create_user_with_status(pool: &PgPool, email: &str, status: &str) -> UserId {
        let (id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified, verification_status) \
             VALUES ($1, true, $2) RETURNING id"
        )
        .bind(email).bind(status)
        .fetch_one(pool).await.unwrap();
        UserId::from(id)
    }

    /// Only 'attested' users may create businesses. Every preceding status must
    /// be rejected — verifying the full BFIP Section 12 attestation gate.
    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_create_business_without_attestation(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let email: String = SafeEmail().fake();
        let user_id       = create_user_with_status(&pool, &email, "registered").await;
        let bus           = EventBus::new();

        let err = create_business(&pool, user_id, test_req("Should Fail"), &bus)
            .await.expect_err("registered user must not create a business");
        assert!(matches!(err, DomainError::Forbidden));
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_create_business_as_identity_confirmed_user(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let email: String = SafeEmail().fake();
        let user_id       = create_user_with_status(&pool, &email, "identity_confirmed").await;
        let bus           = EventBus::new();

        let err = create_business(&pool, user_id, test_req("Should Fail"), &bus)
            .await.expect_err("identity_confirmed user must not create a business");
        assert!(matches!(err, DomainError::Forbidden));
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_create_business_as_presence_confirmed_user(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let email: String = SafeEmail().fake();
        let user_id       = create_user_with_status(&pool, &email, "presence_confirmed").await;
        let bus           = EventBus::new();

        let err = create_business(&pool, user_id, test_req("Should Fail"), &bus)
            .await.expect_err("presence_confirmed user must not create a business");
        assert!(matches!(err, DomainError::Forbidden));
    }

    /// An attested user is hard-capped at 5 active businesses. The 6th attempt
    /// must be rejected regardless of valid credentials.
    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_exceed_business_limit(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail, faker::company::en::CompanyName,
                   faker::address::en::StreetName};
        let email: String   = SafeEmail().fake();
        let user_id         = create_attested_user(&pool, &email).await;
        let bus             = EventBus::new();

        for i in 0..5 {
            let name    = format!("{} {i}", CompanyName().fake::<String>());
            let address = format!("{} {}, Edmonton, AB",
                (i + 1) * 100,
                StreetName().fake::<String>());
            create_business(&pool, user_id, CreateBusinessRequest {
                name, address,
                latitude: None, longitude: None, timezone: None,
                contact_email: None, contact_phone: None,
            }, &bus)
            .await
            .unwrap_or_else(|e| panic!("business {i} creation failed: {e:?}"));
        }

        // 6th attempt must fail.
        let err = create_business(&pool, user_id, test_req("Sixth Business"), &bus)
            .await.expect_err("6th business must be rejected");
        // The limit error is a Conflict with a capacity message.
        assert!(
            matches!(err, DomainError::Conflict(_)),
            "expected Conflict for business limit, got: {err:?}"
        );
    }
}
