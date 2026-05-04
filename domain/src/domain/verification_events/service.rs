use chrono::Utc;
use sqlx::PgPool;

use crate::{
    audit,
    error::{AppResult, DomainError},
    types::UserId,
};
use crate::domain::auth::repository as user_repo;
use super::{
    repository,
    types::{
        AttestationSummary, AttestationTokenSummary, PresenceEventSummary,
        SoultokenSummary, UserAuditTrailResponse, VerificationEventResponse,
        VerificationEventRow,
    },
};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn to_event_response(row: VerificationEventRow) -> VerificationEventResponse {
    VerificationEventResponse {
        id:             row.id,
        event_type:     row.event_type,
        from_status:    row.from_status,
        to_status:      row.to_status,
        reference_type: row.reference_type,
        // actor_id and reference_id intentionally excluded — internal fields only.
        metadata:       row.metadata,
        created_at:     row.created_at,
    }
}

// ── Data assembly helpers ─────────────────────────────────────────────────────

async fn fetch_soultoken_history(pool: &PgPool, user_id: i32) -> AppResult<Vec<SoultokenSummary>> {
    // Exclude uuid — never returned in user-facing responses.
    let rows: Vec<(String, String, chrono::DateTime<Utc>, chrono::DateTime<Utc>, Option<chrono::DateTime<Utc>>, Option<String>)> = sqlx::query_as(
        "SELECT display_code, token_type, issued_at, expires_at, revoked_at, revocation_reason \
         FROM soultokens \
         WHERE holder_user_id = $1 \
         ORDER BY issued_at ASC"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(DomainError::Db)?;

    Ok(rows.into_iter().map(|(display_code, token_type, issued_at, expires_at, revoked_at, revocation_reason)| {
        SoultokenSummary { display_code, token_type, issued_at, expires_at, revoked_at, revocation_reason }
    }).collect())
}

async fn fetch_presence_history(pool: &PgPool, user_id: i32) -> AppResult<Vec<PresenceEventSummary>> {
    let rows: Vec<(String, i32, String, bool, Option<String>, chrono::DateTime<Utc>)> = sqlx::query_as(
        "SELECT event_type, business_id, \
                calendar_date::text AS calendar_date, \
                is_qualifying, rejection_reason, occurred_at \
         FROM presence_events \
         WHERE user_id = $1 \
         ORDER BY occurred_at ASC"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(DomainError::Db)?;

    Ok(rows.into_iter().map(|(event_type, business_id, calendar_date, is_qualifying, rejection_reason, occurred_at)| {
        PresenceEventSummary { event_type, business_id, calendar_date, is_qualifying, rejection_reason, occurred_at }
    }).collect())
}

async fn fetch_attestation_history(pool: &PgPool, user_id: i32) -> AppResult<Vec<AttestationSummary>> {
    let rows: Vec<(String, i32, i32, chrono::DateTime<Utc>)> = sqlx::query_as(
        "SELECT status, attempt_number, visit_id, created_at \
         FROM visit_attestations \
         WHERE user_id = $1 \
         ORDER BY created_at ASC"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(DomainError::Db)?;

    Ok(rows.into_iter().map(|(status, attempt_number, visit_id, created_at)| {
        AttestationSummary { status, attempt_number, visit_id, created_at }
    }).collect())
}

async fn fetch_token_history(pool: &PgPool, user_id: i32) -> AppResult<Vec<AttestationTokenSummary>> {
    // Exclude token_hash — never returned in user-facing responses.
    let rows: Vec<(String, chrono::DateTime<Utc>, chrono::DateTime<Utc>, Option<chrono::DateTime<Utc>>, Option<chrono::DateTime<Utc>>)> = sqlx::query_as(
        "SELECT scope, issued_at, expires_at, verified_at, revoked_at \
         FROM attestation_tokens \
         WHERE user_id = $1 \
         ORDER BY issued_at ASC"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(DomainError::Db)?;

    Ok(rows.into_iter().map(|(scope, issued_at, expires_at, verified_at, revoked_at)| {
        AttestationTokenSummary { scope, issued_at, expires_at, verified_at, revoked_at }
    }).collect())
}

// ── Service functions ─────────────────────────────────────────────────────────

/// Return the authenticated user's complete audit trail (BFIP Section 17.1).
///
/// Records the access request in `audit_request_log` for compliance.
/// Never exposes uuid, token_hash, actor_id, or reference_id.
pub async fn get_my_audit_trail(
    pool:    &PgPool,
    user_id: UserId,
) -> AppResult<UserAuditTrailResponse> {
    let uid = i32::from(user_id);

    // 1. User must exist and not be banned.
    let user = user_repo::find_by_id(pool, user_id)
        .await?
        .ok_or(DomainError::Unauthorized)?;
    if user.is_banned { return Err(DomainError::Forbidden); }

    // 2. Record audit request.
    repository::record_audit_request(pool, uid, uid, "in_app").await?;

    // 3–7. Assemble all history sections.
    let events        = repository::get_events_by_user(pool, uid).await?;
    let soultokens    = fetch_soultoken_history(pool, uid).await?;
    let presence      = fetch_presence_history(pool, uid).await?;
    let attestations  = fetch_attestation_history(pool, uid).await?;
    let tokens        = fetch_token_history(pool, uid).await?;

    // 8. Audit event.
    audit::write(
        pool,
        Some(uid),
        None,
        "audit.trail_requested",
        serde_json::json!({ "delivery_method": "in_app" }),
    ).await;

    Ok(UserAuditTrailResponse {
        user_id:              uid,
        verification_journey: events.into_iter().map(to_event_response).collect(),
        soultoken_history:    soultokens,
        presence_history:     presence,
        attestation_history:  attestations,
        token_history:        tokens,
        requested_at:         Utc::now(),
    })
}

/// Return any user's audit trail — platform_admin only (BFIP Section 17.2).
pub async fn get_admin_audit_trail(
    pool:               &PgPool,
    requesting_user_id: UserId,
    target_user_id:     i32,
) -> AppResult<UserAuditTrailResponse> {
    let rid = i32::from(requesting_user_id);

    // 1. Requesting user must be platform_admin.
    let requester = user_repo::find_by_id(pool, requesting_user_id)
        .await?
        .ok_or(DomainError::Unauthorized)?;
    if !requester.is_platform_admin {
        return Err(DomainError::Forbidden);
    }

    // 2. Record audit request (requested_by = admin).
    repository::record_audit_request(pool, target_user_id, rid, "in_app").await?;

    // 3–7. Assemble history for target user.
    let events       = repository::get_events_by_user(pool, target_user_id).await?;
    let soultokens   = fetch_soultoken_history(pool, target_user_id).await?;
    let presence     = fetch_presence_history(pool, target_user_id).await?;
    let attestations = fetch_attestation_history(pool, target_user_id).await?;
    let tokens       = fetch_token_history(pool, target_user_id).await?;

    // 4. Audit event.
    audit::write(
        pool,
        Some(rid),
        None,
        "audit.trail_requested_by_admin",
        serde_json::json!({
            "target_user_id": target_user_id,
            "requested_by":   rid,
        }),
    ).await;

    Ok(UserAuditTrailResponse {
        user_id:              target_user_id,
        verification_journey: events.into_iter().map(to_event_response).collect(),
        soultoken_history:    soultokens,
        presence_history:     presence,
        attestation_history:  attestations,
        token_history:        tokens,
        requested_at:         Utc::now(),
    })
}

/// Return the authenticated user's verification journey events only.
///
/// Lighter than `get_my_audit_trail` — for in-app status display.
pub async fn get_verification_journey(
    pool:    &PgPool,
    user_id: UserId,
) -> AppResult<Vec<VerificationEventResponse>> {
    let uid = i32::from(user_id);
    let _ = user_repo::find_by_id(pool, user_id)
        .await?
        .ok_or(DomainError::Unauthorized)?;
    let events = repository::get_events_by_user(pool, uid).await?;
    Ok(events.into_iter().map(to_event_response).collect())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::{
            soultokens::{service as soultoken_svc, types::IssueSoultokenRequest},
            attestation_tokens::{service as at_svc, types::IssueAttestationTokenRequest},
        },
        event_bus::EventBus,
        types::UserId,
    };
    use sqlx::PgPool;

    const TEST_HMAC_KEY:    &[u8] = b"test-soultoken-hmac-key-32bytes!!";
    const TEST_SIGNING_KEY: &[u8] = b"test-soultoken-sign-key-32bytes!!";

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

    /// Create a user with a soultoken and some verification events.
    /// Returns (user_id, soultoken_display_code).
    async fn setup_full_user(pool: &PgPool, email: &str) -> (UserId, String) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let bus = EventBus::new();

        let (uid,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified, verification_status) \
             VALUES ($1, true, 'presence_confirmed') RETURNING id",
        )
        .bind(email).fetch_one(pool).await.unwrap();

        let s_email: String = SafeEmail().fake();
        let r1_email: String = SafeEmail().fake();
        let r2_email: String = SafeEmail().fake();
        let (staff_id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id"
        ).bind(&s_email).fetch_one(pool).await.unwrap();
        let (r1_id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id"
        ).bind(&r1_email).fetch_one(pool).await.unwrap();
        let (r2_id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id"
        ).bind(&r2_email).fetch_one(pool).await.unwrap();

        sqlx::query(
            "INSERT INTO identity_credentials \
             (user_id, credential_type, verified_at, cooling_ends_at, cooling_completed_at) \
             VALUES ($1, 'stripe_identity', now(), now() + interval '7 days', now())"
        ).bind(uid).execute(pool).await.unwrap();

        let (loc_id,): (i32,) = sqlx::query_as(
            "INSERT INTO locations (name, location_type, address, timezone) \
             VALUES ('VE Store', 'box_fraise_store', '1 VE St', 'America/Edmonton') RETURNING id"
        ).fetch_one(pool).await.unwrap();

        let (biz_id,): (i32,) = sqlx::query_as(
            "INSERT INTO businesses (location_id, primary_holder_id, name, verification_status) \
             VALUES ($1, $2, 'VE Biz', 'active') RETURNING id"
        ).bind(loc_id).bind(uid).fetch_one(pool).await.unwrap();

        let (thresh_id,): (i32,) = sqlx::query_as(
            "INSERT INTO presence_thresholds \
             (user_id, business_id, event_count, days_count, threshold_met_at) \
             VALUES ($1, $2, 3, 3, now()) RETURNING id"
        ).bind(uid).bind(biz_id).fetch_one(pool).await.unwrap();

        let (visit_id,): (i32,) = sqlx::query_as(
            "INSERT INTO staff_visits (location_id, staff_id, visit_type, status, scheduled_at) \
             VALUES ($1, $2, 'combined', 'completed', now()) RETURNING id"
        ).bind(loc_id).bind(staff_id).fetch_one(pool).await.unwrap();

        let (attest_id,): (i32,) = sqlx::query_as(
            "INSERT INTO visit_attestations \
             (visit_id, user_id, staff_id, presence_threshold_id, \
              assigned_reviewer_1_id, assigned_reviewer_2_id, status) \
             VALUES ($1, $2, $3, $4, $5, $6, 'approved') RETURNING id"
        ).bind(visit_id).bind(uid).bind(staff_id)
         .bind(thresh_id).bind(r1_id).bind(r2_id)
         .fetch_one(pool).await.unwrap();

        sqlx::query(
            "UPDATE users SET verification_status = 'attested', attested_at = now() WHERE id = $1"
        ).bind(uid).execute(pool).await.unwrap();

        // Insert a couple of verification events.
        sqlx::query(
            "INSERT INTO verification_events (user_id, event_type, from_status, to_status) \
             VALUES ($1, 'identity_confirmed', 'registered', 'identity_confirmed'), \
                    ($1, 'attestation_approved', 'presence_confirmed', 'attested')"
        ).bind(uid).execute(pool).await.unwrap();

        // Insert a presence event.
        sqlx::query(
            "INSERT INTO presence_events \
             (user_id, business_id, event_type, is_qualifying, calendar_date, occurred_at) \
             VALUES ($1, $2, 'beacon_dwell', true, CURRENT_DATE, now())"
        ).bind(uid).bind(biz_id).execute(pool).await.unwrap();

        let user = UserId::from(uid);
        let st = soultoken_svc::issue_soultoken(
            pool, user,
            IssueSoultokenRequest { attestation_id: attest_id, token_type: "user".to_owned() },
            TEST_HMAC_KEY, TEST_SIGNING_KEY, &bus,
        ).await.expect("issue_soultoken must succeed");

        (user, st.display_code)
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn get_my_audit_trail_returns_complete_history(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let email: String = SafeEmail().fake();
        let (user, _) = setup_full_user(&pool, &email).await;
        let uid = i32::from(user);

        let trail = get_my_audit_trail(&pool, user)
            .await.expect("get_my_audit_trail must succeed");

        assert_eq!(trail.user_id, uid);
        assert!(!trail.verification_journey.is_empty(), "journey must have events");
        assert!(!trail.soultoken_history.is_empty(), "soultoken_history must be populated");
        assert!(!trail.presence_history.is_empty(), "presence_history must be populated");
        assert!(!trail.attestation_history.is_empty(), "attestation_history must be populated");

        // Audit request logged.
        let log_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_request_log WHERE user_id = $1 AND requested_by = $1"
        ).bind(uid).fetch_one(&pool).await.unwrap();
        assert_eq!(log_count, 1, "audit_request_log must have one record");

        // Audit event written.
        let ae_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_events WHERE event_kind = 'audit.trail_requested'"
        ).fetch_one(&pool).await.unwrap();
        assert!(ae_count >= 1, "audit.trail_requested event must be written");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn get_my_audit_trail_excludes_sensitive_fields(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let email: String = SafeEmail().fake();
        let (user, soultoken_display_code) = setup_full_user(&pool, &email).await;

        // Issue an attestation token to ensure token_history is populated.
        let bus = EventBus::new();
        let at_resp = at_svc::issue_token(&pool, user,
            IssueAttestationTokenRequest {
                scope: "presence.verified".to_owned(),
                requesting_business_soultoken_id: None,
                user_device_id: None,
                presentation_latitude: None,
                presentation_longitude: None,
            }, &bus,
        ).await.expect("issue_token must succeed");

        let trail = get_my_audit_trail(&pool, user).await.unwrap();
        let json  = serde_json::to_string(&trail).unwrap();

        // uuid must not appear in response.
        let uuid_re = regex::Regex::new(
            r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}"
        ).unwrap();
        assert!(!uuid_re.is_match(&json),
            "uuid must not appear in audit trail response");

        // token_hash must not appear.
        assert!(!json.contains("token_hash"),
            "token_hash field must not appear in audit trail");

        // raw_token must not appear.
        assert!(!json.contains(&at_resp.raw_token),
            "raw_token value must not appear in audit trail");

        // actor_id must not appear.
        assert!(!json.contains("actor_id"),
            "actor_id field must not appear in audit trail");

        // reference_id must not appear.
        assert!(!json.contains("reference_id"),
            "reference_id field must not appear in audit trail");

        // But soultoken display_code IS present.
        assert!(json.contains(&soultoken_display_code),
            "display_code must be present in audit trail");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn get_my_audit_trail_is_chronological(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let email: String = SafeEmail().fake();
        let (user, _) = setup_full_user(&pool, &email).await;

        let trail = get_my_audit_trail(&pool, user).await.unwrap();

        let journey = &trail.verification_journey;
        if journey.len() > 1 {
            for w in journey.windows(2) {
                assert!(w[0].created_at <= w[1].created_at,
                    "verification_journey must be ASC by created_at");
            }
        }
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn get_admin_audit_trail_requires_platform_admin(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let email1: String = SafeEmail().fake();
        let email2: String = SafeEmail().fake();
        let non_admin = create_user(&pool, &email1).await;
        let target    = create_user(&pool, &email2).await;

        let err = get_admin_audit_trail(&pool, non_admin, i32::from(target))
            .await.unwrap_err();
        assert!(matches!(err, DomainError::Forbidden));
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn get_verification_journey_returns_events_in_order(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let email: String = SafeEmail().fake();
        let (user, _) = setup_full_user(&pool, &email).await;

        let journey = get_verification_journey(&pool, user).await.unwrap();
        assert!(!journey.is_empty());

        for w in journey.windows(2) {
            assert!(w[0].created_at <= w[1].created_at);
        }
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn audit_request_always_logged(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        // User with no events.
        let email: String = SafeEmail().fake();
        let user = create_user(&pool, &email).await;
        let uid  = i32::from(user);

        let _ = get_my_audit_trail(&pool, user).await.unwrap();

        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_request_log WHERE user_id = $1"
        ).bind(uid).fetch_one(&pool).await.unwrap();
        assert_eq!(count, 1, "audit_request_log must be written even with no events");
    }

    // ── Adversarial tests ─────────────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_view_another_users_audit_trail(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let email_a: String = SafeEmail().fake();
        let email_b: String = SafeEmail().fake();
        let user_a  = create_user(&pool, &email_a).await;
        let (user_b, _) = setup_full_user(&pool, &email_b).await;

        // user_a calls get_my_audit_trail — must only see their own data.
        let trail = get_my_audit_trail(&pool, user_a).await.unwrap();

        assert_eq!(trail.user_id, i32::from(user_a),
            "audit trail must only contain the requesting user's data");
        assert!(trail.verification_journey.is_empty(),
            "user_a must not see user_b's verification events");
        assert!(trail.soultoken_history.is_empty(),
            "user_a must not see user_b's soultokens");

        let _ = user_b;
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_retrieve_uuid_via_audit_trail(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let email: String = SafeEmail().fake();
        let (user, _) = setup_full_user(&pool, &email).await;

        // Fetch the actual soultoken uuid from DB.
        let uuid_val: String = sqlx::query_scalar(
            "SELECT uuid::text FROM soultokens WHERE holder_user_id = $1 LIMIT 1"
        ).bind(i32::from(user)).fetch_one(&pool).await.unwrap();

        let trail = get_my_audit_trail(&pool, user).await.unwrap();
        let json  = serde_json::to_string(&trail).unwrap();

        assert!(!json.contains(&uuid_val),
            "uuid must not appear anywhere in audit trail response");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_retrieve_token_hash_via_audit_trail(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let email: String = SafeEmail().fake();
        let (user, _) = setup_full_user(&pool, &email).await;

        let bus = EventBus::new();
        let _ = at_svc::issue_token(&pool, user,
            IssueAttestationTokenRequest {
                scope: "presence.verified".to_owned(),
                requesting_business_soultoken_id: None,
                user_device_id: None,
                presentation_latitude: None,
                presentation_longitude: None,
            }, &bus,
        ).await.unwrap();

        // Fetch actual hash from DB.
        let stored_hash: String = sqlx::query_scalar(
            "SELECT token_hash FROM attestation_tokens WHERE user_id = $1 LIMIT 1"
        ).bind(i32::from(user)).fetch_one(&pool).await.unwrap();

        let trail = get_my_audit_trail(&pool, user).await.unwrap();
        let json  = serde_json::to_string(&trail).unwrap();

        assert!(!json.contains(&stored_hash),
            "token_hash must not appear in audit trail response");
    }
}
