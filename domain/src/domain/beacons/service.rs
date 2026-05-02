use chrono::{NaiveDate, Utc};
use ring::hmac as ring_hmac;
use sqlx::PgPool;

use crate::{
    audit,
    error::{AppResult, DomainError},
    event_bus::EventBus,
    events::DomainEvent,
    types::UserId,
};
use crate::domain::auth::repository as user_repo;
use crate::domain::businesses::repository as business_repo;
use super::{
    repository,
    types::{
        BeaconResponse, BeaconRow, BeaconSummaryRow, CreateBeaconRequest, DailyUuidResponse,
    },
};

// ── Crypto primitives ─────────────────────────────────────────────────────────

/// Derive a daily UUID for a beacon using BFIP Section 8 formula.
///
/// input  = `{business_id}:{YYYY-MM-DD}` (UTC date)
/// output = HMAC-SHA256(secret_key, input), first 16 bytes formatted as UUID
pub fn derive_daily_uuid(secret_key: &str, business_id: i32, date: NaiveDate) -> String {
    let input   = format!("{}:{}", business_id, date.format("%Y-%m-%d"));
    let key     = ring_hmac::Key::new(ring_hmac::HMAC_SHA256, secret_key.as_bytes());
    let tag     = ring_hmac::sign(&key, input.as_bytes());
    let hex     = hex::encode(&tag.as_ref()[..16]); // first 16 bytes → 32 hex chars
    // Insert hyphens at positions 8, 12, 16, 20 for UUID format (8-4-4-4-12)
    format!("{}-{}-{}-{}-{}",
        &hex[0..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..32])
}

/// Derive a witness HMAC for a presence event (BFIP Section 8 / cryptography.md Section 2).
///
/// input  = `{business_id}:{YYYY-MM-DD}:{user_id}`
/// output = HMAC-SHA256(secret_key, input), full 32 bytes as hex string
pub fn derive_witness_hmac(
    secret_key:  &str,
    business_id: i32,
    date:        NaiveDate,
    user_id:     i32,
) -> String {
    let input = format!("{}:{}:{}", business_id, date.format("%Y-%m-%d"), user_id);
    let key   = ring_hmac::Key::new(ring_hmac::HMAC_SHA256, secret_key.as_bytes());
    let tag   = ring_hmac::sign(&key, input.as_bytes());
    hex::encode(tag.as_ref())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Generate a cryptographically random 32-byte secret key as a hex string.
fn generate_secret_key() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn to_response(row: BeaconRow) -> BeaconResponse {
    BeaconResponse {
        id:                     row.id,
        business_id:            row.business_id,
        location_id:            row.location_id,
        minimum_rssi_threshold: row.minimum_rssi_threshold,
        is_active:              row.is_active,
        last_seen_at:           row.last_seen_at,
        created_at:             row.created_at,
    }
}

fn to_response_summary(row: BeaconSummaryRow) -> BeaconResponse {
    BeaconResponse {
        id:                     row.id,
        business_id:            row.business_id,
        location_id:            row.location_id,
        minimum_rssi_threshold: row.minimum_rssi_threshold,
        is_active:              row.is_active,
        last_seen_at:           row.last_seen_at,
        created_at:             row.created_at,
    }
}

/// Hash a UUID string with SHA-256 for storage in beacon_rotation_log.
fn sha256_hex(input: &str) -> String {
    use sha2::{Sha256, Digest};
    hex::encode(Sha256::digest(input.as_bytes()))
}

// ── Commands ──────────────────────────────────────────────────────────────────

/// Register a new beacon at a business location.
///
/// Rules:
/// 1. Requesting user must be attested.
/// 2. Business must exist, not deleted, and user must be the primary holder.
/// 3. Location must exist and be active.
/// 4. Generates a cryptographically random 32-byte secret key (hex).
/// 5. Records today's UUID in beacon_rotation_log.
/// 6. Writes verification_event and audit_event.
/// 7. Publishes [`DomainEvent::BeaconCreated`].
pub async fn create_beacon(
    pool:      &PgPool,
    user_id:   UserId,
    req:       CreateBeaconRequest,
    event_bus: &EventBus,
) -> AppResult<BeaconResponse> {
    // 1. Validate requesting user.
    let user = user_repo::find_by_id(pool, user_id)
        .await?
        .ok_or(DomainError::Unauthorized)?;

    if user.is_banned {
        return Err(DomainError::Forbidden);
    }
    if user.verification_status != "attested" {
        return Err(DomainError::Forbidden);
    }

    // 2. Validate business ownership.
    let business = business_repo::get_business_by_id(pool, req.business_id)
        .await?
        .ok_or(DomainError::NotFound)?;

    if !business.is_active {
        return Err(DomainError::NotFound);
    }
    if business.primary_holder_id != i32::from(user_id) {
        return Err(DomainError::Forbidden);
    }

    // 3. Validate location.
    let location = business_repo::get_location_by_id(pool, req.location_id)
        .await?
        .ok_or(DomainError::NotFound)?;

    if !location.is_active {
        return Err(DomainError::NotFound);
    }

    // 4. Generate secret key.
    let secret_key             = generate_secret_key();
    let minimum_rssi_threshold = req.minimum_rssi_threshold.unwrap_or(-70);

    // 5. Create beacon.
    let beacon = repository::create_beacon(
        pool,
        location.id,
        business.id,
        &secret_key,
        minimum_rssi_threshold,
    ).await?;

    // 6. Record today's rotation log entry.
    let today      = Utc::now().date_naive();
    let daily_uuid = derive_daily_uuid(&secret_key, business.id, today);
    let uuid_hash  = sha256_hex(&daily_uuid);
    repository::record_rotation(pool, beacon.id, today, &uuid_hash).await.ok();

    // 7. Write verification_event.
    if let Err(e) = sqlx::query(
        "INSERT INTO verification_events \
         (user_id, event_type, reference_type, reference_id, actor_id, metadata) \
         VALUES ($1, 'status_changed', 'beacon', $2, $3, $4)"
    )
    .bind(i32::from(user_id))
    .bind(beacon.id)
    .bind(i32::from(user_id))
    .bind(serde_json::json!({
        "action":      "beacon_created",
        "business_id": business.id,
        "location_id": location.id,
    }))
    .execute(pool)
    .await
    {
        tracing::error!(error = %e, "verification_events insert failed for beacon");
    }

    // 8. Audit event.
    audit::write(
        pool,
        Some(i32::from(user_id)),
        None,
        "beacon.created",
        serde_json::json!({
            "beacon_id":   beacon.id,
            "business_id": business.id,
        }),
    ).await;

    // 9. Publish domain event.
    event_bus.publish(DomainEvent::BeaconCreated {
        beacon_id:   beacon.id,
        business_id: business.id,
        user_id:     i32::from(user_id),
    });

    Ok(to_response(beacon))
}

/// Rotate the secret key for a beacon.
///
/// Rules:
/// 1. Requesting user must be primary holder of the business or platform admin.
/// 2. Generates a new 32-byte random secret key.
/// 3. Old key is preserved as previous_secret_key for 24-hour grace period.
/// 4. Records today's new UUID in the rotation log.
/// 5. Writes audit_event "beacon.key_rotated".
pub async fn rotate_key(
    pool:      &PgPool,
    beacon_id: i32,
    user_id:   UserId,
    event_bus: &EventBus,
) -> AppResult<BeaconResponse> {
    let (beacon, _) = get_beacon_authorized(pool, beacon_id, user_id).await?;

    let new_secret = generate_secret_key();
    let updated    = repository::rotate_secret_key(pool, beacon_id, &new_secret).await?;

    // Record new daily UUID for today.
    if let Some(bid) = updated.business_id {
        let today     = Utc::now().date_naive();
        let new_uuid  = derive_daily_uuid(&new_secret, bid, today);
        let uuid_hash = sha256_hex(&new_uuid);
        // ON CONFLICT DO NOTHING — if already recorded today, this is fine.
        repository::record_rotation(pool, beacon_id, today, &uuid_hash).await.ok();
    }

    audit::write(
        pool,
        Some(i32::from(user_id)),
        None,
        "beacon.key_rotated",
        serde_json::json!({ "beacon_id": beacon.id }),
    ).await;

    event_bus.publish(DomainEvent::BeaconKeyRotated {
        beacon_id,
        user_id: i32::from(user_id),
    });

    Ok(to_response(updated))
}

// ── Queries ───────────────────────────────────────────────────────────────────

/// Return today's derived UUID for a beacon.
///
/// Privileged: only the business owner or a platform admin can call this.
/// The UUID is never stored — it is derived at request time from the secret key.
pub async fn get_daily_uuid(
    pool:      &PgPool,
    beacon_id: i32,
    user_id:   UserId,
) -> AppResult<DailyUuidResponse> {
    let (beacon, _) = get_beacon_authorized(pool, beacon_id, user_id).await?;

    let business_id = beacon.business_id.ok_or(DomainError::NotFound)?;
    let today       = Utc::now().date_naive();
    let uuid        = derive_daily_uuid(&beacon.secret_key, business_id, today);

    // Record rotation log entry (idempotent — ON CONFLICT DO NOTHING).
    let uuid_hash = sha256_hex(&uuid);
    repository::record_rotation(pool, beacon_id, today, &uuid_hash).await.ok();

    // valid_until = end of today in UTC (midnight starting tomorrow)
    let valid_until = today
        .succ_opt()
        .unwrap_or(today)
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        .to_rfc3339();

    Ok(DailyUuidResponse {
        beacon_id,
        calendar_date: today.format("%Y-%m-%d").to_string(),
        uuid,
        valid_until,
    })
}

/// List all active beacons for a business.
///
/// Privileged: only the business owner or a platform admin can call this.
pub async fn list_beacons(
    pool:        &PgPool,
    business_id: i32,
    user_id:     UserId,
) -> AppResult<Vec<BeaconResponse>> {
    // Authorization: user must own the business or be a platform admin.
    let user = user_repo::find_by_id(pool, user_id)
        .await?
        .ok_or(DomainError::Unauthorized)?;

    if !user.is_platform_admin {
        let business = business_repo::get_business_by_id(pool, business_id)
            .await?
            .ok_or(DomainError::NotFound)?;
        if business.primary_holder_id != i32::from(user_id) {
            return Err(DomainError::Forbidden);
        }
    }

    let rows = repository::get_beacons_by_business(pool, business_id).await?;
    Ok(rows.into_iter().map(to_response_summary).collect())
}

// ── Auth helper ───────────────────────────────────────────────────────────────

/// Fetch a beacon and verify the requesting user is authorized.
/// Returns `(BeaconRow, business_id)` on success.
async fn get_beacon_authorized(
    pool:      &PgPool,
    beacon_id: i32,
    user_id:   UserId,
) -> AppResult<(BeaconRow, i32)> {
    let beacon = repository::get_beacon_by_id(pool, beacon_id)
        .await?
        .ok_or(DomainError::NotFound)?;

    let business_id = beacon.business_id.ok_or(DomainError::NotFound)?;

    // Platform admin bypasses ownership check.
    let user = user_repo::find_by_id(pool, user_id)
        .await?
        .ok_or(DomainError::Unauthorized)?;

    if !user.is_platform_admin {
        let business = business_repo::get_business_by_id(pool, business_id)
            .await?
            .ok_or(DomainError::NotFound)?;
        if business.primary_holder_id != i32::from(user_id) {
            return Err(DomainError::Forbidden);
        }
    }

    Ok((beacon, business_id))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_bus::EventBus;
    use sqlx::PgPool;

    // ── Fixtures ──────────────────────────────────────────────────────────────

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
            "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id"
        )
        .bind(email)
        .fetch_one(pool)
        .await
        .unwrap();
        UserId::from(id)
    }

    /// Create a location and business owned by `user_id`. Returns (business_id, location_id).
    async fn create_business_with_location(pool: &PgPool, user_id: UserId) -> (i32, i32) {
        let (loc_id,): (i32,) = sqlx::query_as(
            "INSERT INTO locations (name, location_type, address, timezone) \
             VALUES ('Test Location', 'partner_business', '123 Test St', 'America/Edmonton') \
             RETURNING id"
        )
        .fetch_one(pool)
        .await
        .unwrap();

        let (biz_id,): (i32,) = sqlx::query_as(
            "INSERT INTO businesses (location_id, primary_holder_id, name, verification_status) \
             VALUES ($1, $2, 'Test Biz', 'pending') RETURNING id"
        )
        .bind(loc_id)
        .bind(i32::from(user_id))
        .fetch_one(pool)
        .await
        .unwrap();

        (biz_id, loc_id)
    }

    fn beacon_req(business_id: i32, location_id: i32) -> CreateBeaconRequest {
        CreateBeaconRequest {
            business_id,
            location_id,
            minimum_rssi_threshold: Some(-70),
        }
    }

    // ── create_beacon ─────────────────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn create_beacon_succeeds_for_business_owner(pool: PgPool) {
        let user_id               = create_attested_user(&pool, "owner@beacon.test").await;
        let (biz_id, loc_id) = create_business_with_location(&pool, user_id).await;
        let bus                   = EventBus::new();

        let resp = create_beacon(&pool, user_id, beacon_req(biz_id, loc_id), &bus)
            .await
            .expect("owner must be able to create a beacon");

        assert_eq!(resp.business_id, Some(biz_id));
        assert_eq!(resp.location_id, loc_id);
        assert_eq!(resp.minimum_rssi_threshold, -70);
        assert!(resp.is_active);
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn create_beacon_fails_for_non_owner(pool: PgPool) {
        let owner_id             = create_attested_user(&pool, "owner2@beacon.test").await;
        let other_id             = create_attested_user(&pool, "other@beacon.test").await;
        let (biz_id, loc_id) = create_business_with_location(&pool, owner_id).await;
        let bus                  = EventBus::new();

        let err = create_beacon(&pool, other_id, beacon_req(biz_id, loc_id), &bus)
            .await
            .expect_err("non-owner must not create a beacon");

        assert!(matches!(err, DomainError::Forbidden), "expected Forbidden, got: {err:?}");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn create_beacon_fails_for_unattested_user(pool: PgPool) {
        let unattested_id        = create_registered_user(&pool, "unattested@beacon.test").await;
        let owner_id             = create_attested_user(&pool, "owner3@beacon.test").await;
        let (biz_id, loc_id) = create_business_with_location(&pool, owner_id).await;
        let bus                  = EventBus::new();

        let err = create_beacon(&pool, unattested_id, beacon_req(biz_id, loc_id), &bus)
            .await
            .expect_err("unattested user must not create a beacon");

        assert!(matches!(err, DomainError::Forbidden));
    }

    // ── get_daily_uuid ────────────────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn get_daily_uuid_returns_valid_uuid_for_owner(pool: PgPool) {
        let user_id              = create_attested_user(&pool, "uuid_owner@beacon.test").await;
        let (biz_id, loc_id) = create_business_with_location(&pool, user_id).await;
        let bus                  = EventBus::new();

        let beacon = create_beacon(&pool, user_id, beacon_req(biz_id, loc_id), &bus)
            .await.unwrap();

        let resp = get_daily_uuid(&pool, beacon.id, user_id)
            .await
            .expect("owner must get daily UUID");

        assert_eq!(resp.beacon_id, beacon.id);
        // UUID format: 8-4-4-4-12 (36 chars with hyphens)
        assert_eq!(resp.uuid.len(), 36);
        assert_eq!(resp.uuid.chars().filter(|&c| c == '-').count(), 4);
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn get_daily_uuid_fails_for_non_owner(pool: PgPool) {
        let owner_id             = create_attested_user(&pool, "uuid_owner2@beacon.test").await;
        let other_id             = create_attested_user(&pool, "uuid_other@beacon.test").await;
        let (biz_id, loc_id) = create_business_with_location(&pool, owner_id).await;
        let bus                  = EventBus::new();

        let beacon = create_beacon(&pool, owner_id, beacon_req(biz_id, loc_id), &bus)
            .await.unwrap();

        let err = get_daily_uuid(&pool, beacon.id, other_id)
            .await
            .expect_err("non-owner must not get daily UUID");

        assert!(matches!(err, DomainError::Forbidden));
    }

    // ── rotate_key ────────────────────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn rotate_key_generates_new_key_and_preserves_previous(pool: PgPool) {
        let user_id              = create_attested_user(&pool, "rotate@beacon.test").await;
        let (biz_id, loc_id) = create_business_with_location(&pool, user_id).await;
        let bus                  = EventBus::new();

        create_beacon(&pool, user_id, beacon_req(biz_id, loc_id), &bus)
            .await.unwrap();

        // Fetch the beacon to get original secret_key.
        let (beacon_id,): (i32,) = sqlx::query_as(
            "SELECT id FROM beacons WHERE business_id = $1 LIMIT 1"
        )
        .bind(biz_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        let original_key: String = sqlx::query_scalar(
            "SELECT secret_key FROM beacons WHERE id = $1"
        )
        .bind(beacon_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        // Rotate the key.
        rotate_key(&pool, beacon_id, user_id, &bus).await.unwrap();

        let (new_key, prev_key): (String, Option<String>) = sqlx::query_as(
            "SELECT secret_key, previous_secret_key FROM beacons WHERE id = $1"
        )
        .bind(beacon_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_ne!(new_key, original_key, "new key must differ from original");
        assert_eq!(prev_key.as_deref(), Some(original_key.as_str()), "original key must be preserved");
    }

    // ── Crypto unit tests (no DB) ─────────────────────────────────────────────

    #[test]
    fn derive_daily_uuid_is_deterministic() {
        let date = NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();
        let a    = derive_daily_uuid("test-secret-key", 42, date);
        let b    = derive_daily_uuid("test-secret-key", 42, date);
        assert_eq!(a, b, "same inputs must always produce the same UUID");
    }

    #[test]
    fn derive_daily_uuid_differs_by_date() {
        let d1 = NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();
        let d2 = NaiveDate::from_ymd_opt(2026, 5, 2).unwrap();
        let a  = derive_daily_uuid("test-secret-key", 42, d1);
        let b  = derive_daily_uuid("test-secret-key", 42, d2);
        assert_ne!(a, b, "different dates must produce different UUIDs");
    }

    #[test]
    fn derive_daily_uuid_is_uuid_format() {
        let date = NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();
        let uuid = derive_daily_uuid("test-secret-key", 42, date);
        assert_eq!(uuid.len(), 36);
        let parts: Vec<&str> = uuid.split('-').collect();
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[0].len(), 8);
        assert_eq!(parts[1].len(), 4);
        assert_eq!(parts[2].len(), 4);
        assert_eq!(parts[3].len(), 4);
        assert_eq!(parts[4].len(), 12);
    }

    #[test]
    fn derive_witness_hmac_differs_by_user() {
        let date = NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();
        let a    = derive_witness_hmac("test-secret-key", 42, date, 1);
        let b    = derive_witness_hmac("test-secret-key", 42, date, 2);
        assert_ne!(a, b, "different users must produce different witness HMACs");
    }

    #[test]
    fn derive_witness_hmac_is_deterministic() {
        let date = NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();
        let a    = derive_witness_hmac("test-secret-key", 42, date, 99);
        let b    = derive_witness_hmac("test-secret-key", 42, date, 99);
        assert_eq!(a, b);
    }

    // ── Adversarial beacon tests ──────────────────────────────────────────────

    /// User B must not be able to create a beacon on behalf of User A's business.
    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_create_beacon_for_another_users_business(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let owner_id             = create_attested_user(&pool, SafeEmail().fake::<String>().as_str()).await;
        let attacker_id          = create_attested_user(&pool, SafeEmail().fake::<String>().as_str()).await;
        let (biz_id, loc_id) = create_business_with_location(&pool, owner_id).await;
        let bus                  = EventBus::new();

        let err = create_beacon(&pool, attacker_id, beacon_req(biz_id, loc_id), &bus)
            .await.expect_err("attacker must not create beacon on another user's business");
        assert!(matches!(err, DomainError::Forbidden));
    }

    /// User B must not be able to retrieve the daily UUID for User A's beacon.
    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_retrieve_daily_uuid_for_another_users_beacon(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let owner_id             = create_attested_user(&pool, SafeEmail().fake::<String>().as_str()).await;
        let attacker_id          = create_attested_user(&pool, SafeEmail().fake::<String>().as_str()).await;
        let (biz_id, loc_id) = create_business_with_location(&pool, owner_id).await;
        let bus                  = EventBus::new();

        let beacon = create_beacon(&pool, owner_id, beacon_req(biz_id, loc_id), &bus)
            .await.unwrap();

        let err = get_daily_uuid(&pool, beacon.id, attacker_id)
            .await.expect_err("attacker must not get daily UUID for another user's beacon");
        assert!(matches!(err, DomainError::Forbidden));
    }

    /// User B must not be able to rotate the secret key for User A's beacon.
    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_rotate_another_users_beacon_key(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let owner_id             = create_attested_user(&pool, SafeEmail().fake::<String>().as_str()).await;
        let attacker_id          = create_attested_user(&pool, SafeEmail().fake::<String>().as_str()).await;
        let (biz_id, loc_id) = create_business_with_location(&pool, owner_id).await;
        let bus                  = EventBus::new();

        let beacon = create_beacon(&pool, owner_id, beacon_req(biz_id, loc_id), &bus)
            .await.unwrap();

        let err = rotate_key(&pool, beacon.id, attacker_id, &bus)
            .await.expect_err("attacker must not rotate another user's beacon key");
        assert!(matches!(err, DomainError::Forbidden));
    }

    /// The list_beacons response must never contain secret_key or previous_secret_key.
    /// This is both a type-level guarantee (BeaconResponse has no secret fields)
    /// and a serialization-level guarantee tested here.
    #[sqlx::test(migrations = "../server/migrations")]
    async fn secret_key_never_appears_in_list_response(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let owner_id             = create_attested_user(&pool, SafeEmail().fake::<String>().as_str()).await;
        let (biz_id, loc_id) = create_business_with_location(&pool, owner_id).await;
        let bus                  = EventBus::new();

        // Create beacon — get back BeaconResponse (no secret field).
        let beacon = create_beacon(&pool, owner_id, beacon_req(biz_id, loc_id), &bus)
            .await.unwrap();

        // Fetch the actual secret_key from the DB for the "does the value appear?" check.
        let actual_secret: String = sqlx::query_scalar(
            "SELECT secret_key FROM beacons WHERE id = $1"
        )
        .bind(beacon.id)
        .fetch_one(&pool).await.unwrap();

        // List beacons — returns Vec<BeaconResponse>.
        let list = list_beacons(&pool, biz_id, owner_id).await.unwrap();
        let json = serde_json::to_string(&list).unwrap();

        assert!(
            !json.contains("secret_key"),
            "JSON must not contain the field name 'secret_key'"
        );
        assert!(
            !json.contains(&actual_secret),
            "JSON must not contain the actual secret key value"
        );
    }
}
