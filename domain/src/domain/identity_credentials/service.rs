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
        CoolingStatusResponse, IdentityCredentialResponse, IdentityCredentialRow,
        InitiateVerificationRequest, RecordAppOpenRequest,
    },
};

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Constant-time comparison — prevents timing oracle on HMAC tags.
fn hmac_eq(a: &str, b: &str) -> bool {
    let ab = a.as_bytes();
    let bb = b.as_bytes();
    if ab.len() != bb.len() {
        return false;
    }
    ab.iter().zip(bb.iter()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn to_credential_response(c: IdentityCredentialRow) -> IdentityCredentialResponse {
    IdentityCredentialResponse {
        id:                         c.id,
        credential_type:            c.credential_type,
        external_session_id:        c.external_session_id,
        verified_at:                c.verified_at,
        cooling_ends_at:            c.cooling_ends_at,
        cooling_app_opens_required: c.cooling_app_opens_required,
        cooling_completed_at:       c.cooling_completed_at,
        raw_verification_status:    c.raw_verification_status,
        created_at:                 c.created_at,
    }
}

fn to_cooling_response(cred: &IdentityCredentialRow, days_completed: i64) -> CoolingStatusResponse {
    CoolingStatusResponse {
        credential_id:        cred.id,
        days_completed,
        days_required:        cred.cooling_app_opens_required,
        cooling_ends_at:      cred.cooling_ends_at,
        cooling_completed_at: cred.cooling_completed_at,
        is_complete:          cred.cooling_completed_at.is_some(),
    }
}

// ── Commands ──────────────────────────────────────────────────────────────────

/// Record a successful Stripe Identity verification session.
///
/// Called by the iOS app after Stripe confirms identity on the client side.
/// Creates an `identity_credentials` row, transitions the user to
/// `identity_confirmed`, and starts the 7-day cooling period.
pub async fn initiate_verification(
    pool:      &PgPool,
    user_id:   UserId,
    req:       InitiateVerificationRequest,
    event_bus: &EventBus,
) -> AppResult<IdentityCredentialResponse> {
    let uid = i32::from(user_id);

    let user = user_repo::find_by_id(pool, user_id).await?.ok_or(DomainError::Unauthorized)?;
    if user.is_banned { return Err(DomainError::Forbidden); }

    // Only 'registered' users start identity verification.
    if user.verification_status != "registered" {
        return Err(DomainError::conflict("identity already verified or in progress"));
    }

    // Idempotency: reject duplicate Stripe sessions.
    if repository::get_identity_credential_by_session(pool, &req.stripe_session_id).await?.is_some() {
        return Err(DomainError::conflict("stripe session already recorded"));
    }

    let now             = Utc::now();
    let cooling_ends_at = now + chrono::Duration::days(7);

    let cred = repository::create_identity_credential(
        pool,
        uid,
        "stripe_identity",
        Some(&req.stripe_session_id),
        now,
        cooling_ends_at,
    ).await?;

    // Advance user to identity_confirmed.
    sqlx::query(
        "UPDATE users SET verification_status = 'identity_confirmed' \
         WHERE id = $1 AND verification_status = 'registered'"
    )
    .bind(uid)
    .execute(pool)
    .await
    .map_err(DomainError::Db)?;

    // Verification events — failures are logged but never fatal.
    if let Err(e) = sqlx::query(
        "INSERT INTO verification_events \
         (user_id, event_type, reference_type, reference_id, actor_id) \
         VALUES ($1, 'identity_confirmed', 'identity_credential', $2, $1)"
    )
    .bind(uid).bind(cred.id).execute(pool).await
    {
        tracing::error!(error = %e, "verification_events (identity_confirmed) insert failed");
    }

    if let Err(e) = sqlx::query(
        "INSERT INTO verification_events \
         (user_id, event_type, reference_type, reference_id, actor_id) \
         VALUES ($1, 'cooling_period_started', 'identity_credential', $2, $1)"
    )
    .bind(uid).bind(cred.id).execute(pool).await
    {
        tracing::error!(error = %e, "verification_events (cooling_period_started) insert failed");
    }

    if let Err(e) = sqlx::query(
        "INSERT INTO verification_events \
         (user_id, event_type, from_status, to_status, actor_id) \
         VALUES ($1, 'status_changed', 'registered', 'identity_confirmed', $1)"
    )
    .bind(uid).execute(pool).await
    {
        tracing::error!(error = %e, "verification_events (status_changed) insert failed");
    }

    audit::write(
        pool,
        Some(uid),
        None,
        "identity.verification_initiated",
        serde_json::json!({ "credential_id": cred.id }),
    ).await;

    event_bus.publish(DomainEvent::IdentityVerificationInitiated {
        user_id:       uid,
        credential_id: cred.id,
    });

    Ok(to_credential_response(cred))
}

/// Process a Stripe Identity webhook callback.
///
/// Validates the Stripe-Signature HMAC, then updates the credential row
/// with the raw verification status and response hash. Silently ignores
/// payloads for sessions that are not in the database (multi-env safety).
pub async fn handle_stripe_webhook(
    pool:             &PgPool,
    payload:          &[u8],
    stripe_signature: &str,
    webhook_secret:   &str,
) -> AppResult<()> {
    // Stripe-Signature header format: "t=<unix_ts>,v1=<hex_hmac>[,v0=<deprecated>]"
    let timestamp = stripe_signature
        .split(',')
        .find(|p| p.starts_with("t="))
        .and_then(|p| p.strip_prefix("t="))
        .ok_or_else(|| DomainError::invalid_input("missing timestamp in Stripe-Signature"))?;

    let expected_hex = stripe_signature
        .split(',')
        .find(|p| p.starts_with("v1="))
        .and_then(|p| p.strip_prefix("v1="))
        .ok_or_else(|| DomainError::invalid_input("missing v1 in Stripe-Signature"))?;

    // Signed payload: "<timestamp>.<raw_body>"
    let signed = format!("{}.{}", timestamp, String::from_utf8_lossy(payload));
    let key    = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, webhook_secret.as_bytes());
    let tag    = ring::hmac::sign(&key, signed.as_bytes());
    let computed_hex = hex::encode(tag.as_ref());

    if !hmac_eq(&computed_hex, expected_hex) {
        return Err(DomainError::Unauthorized);
    }

    let event: serde_json::Value = serde_json::from_slice(payload)
        .map_err(|_| DomainError::invalid_input("invalid JSON in Stripe webhook payload"))?;

    let session_id = event["data"]["object"]["id"]
        .as_str()
        .ok_or_else(|| DomainError::invalid_input("missing session id in webhook payload"))?;

    let raw_status = event["data"]["object"]["status"].as_str().unwrap_or("unknown");
    let report_id  = event["data"]["object"]["last_verification_report"].as_str();

    let Some(cred) = repository::get_identity_credential_by_session(pool, session_id).await? else {
        tracing::warn!(session_id, "stripe webhook for unknown session — ignoring");
        return Ok(());
    };

    repository::update_stripe_webhook(
        pool,
        cred.id,
        report_id,
        Some(raw_status),
        Some(&computed_hex),
    ).await?;

    Ok(())
}

/// Record a cooling-period app open for the authenticated user.
///
/// One qualifying open per calendar day (enforced by DB UNIQUE constraint).
/// Idempotent within the same day. When both conditions are met
/// (`now() >= cooling_ends_at` AND `distinct_days >= required`), marks
/// `cooling_completed_at` and publishes [`DomainEvent::CoolingPeriodCompleted`].
pub async fn record_app_open(
    pool:      &PgPool,
    user_id:   UserId,
    req:       RecordAppOpenRequest,
    event_bus: &EventBus,
) -> AppResult<CoolingStatusResponse> {
    let uid = i32::from(user_id);

    let user = user_repo::find_by_id(pool, user_id).await?.ok_or(DomainError::Unauthorized)?;
    if user.is_banned { return Err(DomainError::Forbidden); }

    let cred = repository::get_identity_credential_by_id(pool, req.credential_id)
        .await?
        .ok_or(DomainError::NotFound)?;

    if cred.user_id != uid {
        return Err(DomainError::Forbidden);
    }

    // Already complete — return current state without writing anything.
    if cred.cooling_completed_at.is_some() {
        let days = repository::count_cooling_days(pool, uid, cred.id).await?;
        return Ok(to_cooling_response(&cred, days));
    }

    let today    = Utc::now().date_naive();
    let inserted = repository::insert_cooling_event(
        pool,
        uid,
        cred.id,
        req.device_identifier.as_deref(),
        req.app_attest_assertion.as_deref(),
        today,
    ).await?;

    let days = repository::count_cooling_days(pool, uid, cred.id).await?;

    if inserted {
        if let Err(e) = sqlx::query(
            "INSERT INTO verification_events \
             (user_id, event_type, reference_type, reference_id, actor_id, metadata) \
             VALUES ($1, 'cooling_app_open_recorded', 'identity_credential', $2, $1, $3)"
        )
        .bind(uid)
        .bind(cred.id)
        .bind(serde_json::json!({ "days_completed": days }))
        .execute(pool)
        .await
        {
            tracing::error!(error = %e, "verification_events (cooling_app_open_recorded) insert failed");
        }

        event_bus.publish(DomainEvent::CoolingAppOpenRecorded {
            user_id:       uid,
            credential_id: cred.id,
            days_completed: days,
        });
    }

    let now                    = Utc::now();
    let cooling_window_elapsed = now >= cred.cooling_ends_at;
    let enough_days            = days >= i64::from(cred.cooling_app_opens_required);

    if cooling_window_elapsed && enough_days {
        let updated = repository::complete_cooling(pool, cred.id).await?;

        if let Err(e) = sqlx::query(
            "INSERT INTO verification_events \
             (user_id, event_type, reference_type, reference_id, actor_id, metadata) \
             VALUES ($1, 'cooling_period_completed', 'identity_credential', $2, $1, $3)"
        )
        .bind(uid)
        .bind(cred.id)
        .bind(serde_json::json!({ "days_completed": days }))
        .execute(pool)
        .await
        {
            tracing::error!(error = %e, "verification_events (cooling_period_completed) insert failed");
        }

        audit::write(
            pool,
            Some(uid),
            None,
            "identity.cooling_completed",
            serde_json::json!({ "credential_id": cred.id, "days_completed": days }),
        ).await;

        event_bus.publish(DomainEvent::CoolingPeriodCompleted {
            user_id:       uid,
            credential_id: cred.id,
        });

        return Ok(to_cooling_response(&updated, days));
    }

    Ok(to_cooling_response(&cred, days))
}

// ── Queries ───────────────────────────────────────────────────────────────────

/// Return the current cooling status for the most-recent credential of a user.
///
/// Returns `NotFound` if the user has not initiated identity verification.
pub async fn get_cooling_status(
    pool:    &PgPool,
    user_id: UserId,
) -> AppResult<CoolingStatusResponse> {
    let uid  = i32::from(user_id);
    let cred = repository::get_latest_credential_by_user(pool, uid)
        .await?
        .ok_or(DomainError::NotFound)?;
    let days = repository::count_cooling_days(pool, uid, cred.id).await?;
    Ok(to_cooling_response(&cred, days))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{event_bus::EventBus, types::UserId};
    use chrono::Duration;
    use sqlx::PgPool;

    // ── Fixtures ──────────────────────────────────────────────────────────────

    async fn create_registered_user(pool: &PgPool, email: &str) -> UserId {
        let (id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified, verification_status) \
             VALUES ($1, true, 'registered') RETURNING id"
        )
        .bind(email).fetch_one(pool).await.unwrap();
        UserId::from(id)
    }

    async fn create_identity_confirmed_user(pool: &PgPool, email: &str) -> UserId {
        let (id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified, verification_status) \
             VALUES ($1, true, 'identity_confirmed') RETURNING id"
        )
        .bind(email).fetch_one(pool).await.unwrap();
        UserId::from(id)
    }

    async fn create_attested_user(pool: &PgPool, email: &str) -> UserId {
        let (id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified, verification_status) \
             VALUES ($1, true, 'attested') RETURNING id"
        )
        .bind(email).fetch_one(pool).await.unwrap();
        UserId::from(id)
    }

    /// Create an identity credential with a back-dated cooling window so
    /// the cooling check (`now() >= cooling_ends_at`) already passes.
    async fn create_past_cooling_credential(pool: &PgPool, user_id: i32) -> i32 {
        let verified_at    = Utc::now() - Duration::days(10);
        let cooling_ends_at = verified_at + Duration::days(7); // 3 days ago
        let (id,): (i32,) = sqlx::query_as(
            "INSERT INTO identity_credentials \
             (user_id, credential_type, verified_at, cooling_ends_at) \
             VALUES ($1, 'stripe_identity', $2, $3) RETURNING id"
        )
        .bind(user_id)
        .bind(verified_at)
        .bind(cooling_ends_at)
        .fetch_one(pool).await.unwrap();
        id
    }

    /// Insert a cooling event on a specific offset from today (negative = past days).
    async fn insert_cooling_event_on_day(pool: &PgPool, user_id: i32, cred_id: i32, days_offset: i64) {
        let date = (Utc::now() + Duration::days(days_offset)).date_naive();
        sqlx::query(
            "INSERT INTO cooling_period_events (user_id, credential_id, calendar_date) \
             VALUES ($1, $2, $3) ON CONFLICT DO NOTHING"
        )
        .bind(user_id).bind(cred_id).bind(date)
        .execute(pool).await.unwrap();
    }

    fn make_webhook_signature(secret: &str, payload: &[u8]) -> String {
        let ts  = "1700000000";
        let msg = format!("{}.{}", ts, String::from_utf8_lossy(payload));
        let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, secret.as_bytes());
        let tag = ring::hmac::sign(&key, msg.as_bytes());
        format!("t={},v1={}", ts, hex::encode(tag.as_ref()))
    }

    // ── Tests 1–4: initiate_verification ─────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn initiate_verification_creates_credential_and_sets_identity_confirmed(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let uid = create_registered_user(&pool, &SafeEmail().fake::<String>()).await;
        let bus = EventBus::new();

        let resp = initiate_verification(
            &pool, uid,
            InitiateVerificationRequest { stripe_session_id: "vs_test_abc123".into() },
            &bus,
        ).await.expect("initiate_verification must succeed");

        assert_eq!(resp.credential_type, "stripe_identity");
        assert_eq!(resp.external_session_id.as_deref(), Some("vs_test_abc123"));
        assert!(resp.cooling_completed_at.is_none());

        let status: String = sqlx::query_scalar(
            "SELECT verification_status FROM users WHERE id = $1"
        )
        .bind(i32::from(uid))
        .fetch_one(&pool).await.unwrap();
        assert_eq!(status, "identity_confirmed");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn initiate_verification_rejects_duplicate_session(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let uid  = create_registered_user(&pool, &SafeEmail().fake::<String>()).await;
        let bus  = EventBus::new();
        let sess = "vs_test_dup999";

        initiate_verification(
            &pool, uid,
            InitiateVerificationRequest { stripe_session_id: sess.into() },
            &bus,
        ).await.unwrap();

        // Registering the same session again must be rejected.
        let uid2 = create_registered_user(&pool, &SafeEmail().fake::<String>()).await;
        let err  = initiate_verification(
            &pool, uid2,
            InitiateVerificationRequest { stripe_session_id: sess.into() },
            &bus,
        ).await.unwrap_err();
        assert!(matches!(err, DomainError::Conflict(_)));
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn initiate_verification_rejects_already_confirmed_user(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let uid = create_identity_confirmed_user(&pool, &SafeEmail().fake::<String>()).await;
        let bus = EventBus::new();

        let err = initiate_verification(
            &pool, uid,
            InitiateVerificationRequest { stripe_session_id: "vs_test_late".into() },
            &bus,
        ).await.unwrap_err();
        assert!(matches!(err, DomainError::Conflict(_)));
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn initiate_verification_rejects_unknown_user(pool: PgPool) {
        let bus = EventBus::new();
        let err = initiate_verification(
            &pool,
            UserId::from(999_999_i32),
            InitiateVerificationRequest { stripe_session_id: "vs_ghost".into() },
            &bus,
        ).await.unwrap_err();
        assert!(matches!(err, DomainError::Unauthorized));
    }

    // ── Tests 5–6: handle_stripe_webhook ─────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn handle_stripe_webhook_updates_credential(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let uid = create_registered_user(&pool, &SafeEmail().fake::<String>()).await;
        let bus = EventBus::new();

        let resp = initiate_verification(
            &pool, uid,
            InitiateVerificationRequest { stripe_session_id: "vs_webhook_test".into() },
            &bus,
        ).await.unwrap();

        let payload = serde_json::to_vec(&serde_json::json!({
            "type": "identity.verification_session.verified",
            "data": {
                "object": {
                    "id": "vs_webhook_test",
                    "status": "verified",
                    "last_verification_report": "vr_report_001"
                }
            }
        })).unwrap();

        let secret = "whsec_test_secret";
        let sig    = make_webhook_signature(secret, &payload);

        handle_stripe_webhook(&pool, &payload, &sig, secret).await
            .expect("valid webhook must succeed");

        let (raw_status, report_id): (Option<String>, Option<String>) = sqlx::query_as(
            "SELECT raw_verification_status, stripe_identity_report_id \
             FROM identity_credentials WHERE id = $1"
        )
        .bind(resp.id)
        .fetch_one(&pool).await.unwrap();

        assert_eq!(raw_status.as_deref(), Some("verified"));
        assert_eq!(report_id.as_deref(), Some("vr_report_001"));
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn handle_stripe_webhook_rejects_invalid_signature(pool: PgPool) {
        let payload = b"{}";
        let err = handle_stripe_webhook(
            &pool, payload,
            "t=1234567890,v1=0000000000000000000000000000000000000000000000000000000000000000",
            "whsec_real_secret",
        ).await.unwrap_err();
        assert!(matches!(err, DomainError::Unauthorized));
    }

    // ── Tests 7–9: record_app_open ────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn record_app_open_records_event_and_returns_status(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let uid    = create_identity_confirmed_user(&pool, &SafeEmail().fake::<String>()).await;
        let cred_id = create_past_cooling_credential(&pool, i32::from(uid)).await;
        let bus    = EventBus::new();

        let resp = record_app_open(
            &pool, uid,
            RecordAppOpenRequest { credential_id: cred_id, device_identifier: None, app_attest_assertion: None },
            &bus,
        ).await.expect("record_app_open must succeed");

        assert_eq!(resp.days_completed, 1);
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn record_app_open_same_day_is_idempotent(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let uid     = create_identity_confirmed_user(&pool, &SafeEmail().fake::<String>()).await;
        let cred_id = create_past_cooling_credential(&pool, i32::from(uid)).await;
        let bus     = EventBus::new();

        let req = || RecordAppOpenRequest {
            credential_id:        cred_id,
            device_identifier:    None,
            app_attest_assertion: None,
        };

        let r1 = record_app_open(&pool, uid, req(), &bus).await.unwrap();
        let r2 = record_app_open(&pool, uid, req(), &bus).await.unwrap();

        assert_eq!(r1.days_completed, 1, "first open counts as day 1");
        assert_eq!(r2.days_completed, 1, "same-day repeat must not advance count");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn record_app_open_completes_cooling_when_conditions_met(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let uid     = create_identity_confirmed_user(&pool, &SafeEmail().fake::<String>()).await;
        let cred_id = create_past_cooling_credential(&pool, i32::from(uid)).await;
        let bus     = EventBus::new();

        // Seed 2 past days so the third open (today) hits the threshold of 3.
        insert_cooling_event_on_day(&pool, i32::from(uid), cred_id, -2).await;
        insert_cooling_event_on_day(&pool, i32::from(uid), cred_id, -1).await;

        let resp = record_app_open(
            &pool, uid,
            RecordAppOpenRequest { credential_id: cred_id, device_identifier: None, app_attest_assertion: None },
            &bus,
        ).await.unwrap();

        assert_eq!(resp.days_completed, 3);
        assert!(resp.is_complete);
        assert!(resp.cooling_completed_at.is_some());
    }

    // ── Test 10: get_cooling_status ───────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn get_cooling_status_returns_not_found_without_credential(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let uid = create_registered_user(&pool, &SafeEmail().fake::<String>()).await;

        let err = get_cooling_status(&pool, uid).await.unwrap_err();
        assert!(matches!(err, DomainError::NotFound));
    }

    // ── Adversarial tests ─────────────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn record_app_open_forbidden_for_wrong_user(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let owner    = create_identity_confirmed_user(&pool, &SafeEmail().fake::<String>()).await;
        let attacker = create_attested_user(&pool, &SafeEmail().fake::<String>()).await;
        let cred_id  = create_past_cooling_credential(&pool, i32::from(owner)).await;
        let bus      = EventBus::new();

        let err = record_app_open(
            &pool, attacker,
            RecordAppOpenRequest { credential_id: cred_id, device_identifier: None, app_attest_assertion: None },
            &bus,
        ).await.unwrap_err();
        assert!(matches!(err, DomainError::Forbidden));
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn record_app_open_already_complete_returns_status_without_writing(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let uid     = create_identity_confirmed_user(&pool, &SafeEmail().fake::<String>()).await;
        let cred_id = create_past_cooling_credential(&pool, i32::from(uid)).await;
        let bus     = EventBus::new();

        // Complete the cooling.
        insert_cooling_event_on_day(&pool, i32::from(uid), cred_id, -2).await;
        insert_cooling_event_on_day(&pool, i32::from(uid), cred_id, -1).await;
        record_app_open(
            &pool, uid,
            RecordAppOpenRequest { credential_id: cred_id, device_identifier: None, app_attest_assertion: None },
            &bus,
        ).await.unwrap();

        // Get count before calling again.
        let count_before: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM cooling_period_events WHERE credential_id = $1"
        )
        .bind(cred_id).fetch_one(&pool).await.unwrap();

        // Second call after completion must succeed and not insert a new event.
        let resp = record_app_open(
            &pool, uid,
            RecordAppOpenRequest { credential_id: cred_id, device_identifier: None, app_attest_assertion: None },
            &bus,
        ).await.unwrap();
        assert!(resp.is_complete);

        let count_after: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM cooling_period_events WHERE credential_id = $1"
        )
        .bind(cred_id).fetch_one(&pool).await.unwrap();

        assert_eq!(count_before, count_after, "no new event after cooling is complete");
    }
}
