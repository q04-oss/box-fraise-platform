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
use crate::domain::identity_credentials::repository as ic_repo;
use super::{
    repository,
    types::{
        BackgroundCheckResponse, BackgroundCheckRow, BackgroundCheckStatusResponse,
        CheckWebhookPayload, InitiateCheckRequest,
    },
};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn to_response(row: BackgroundCheckRow) -> BackgroundCheckResponse {
    let is_expired = row.expires_at
        .map(|e| e < Utc::now())
        .unwrap_or(false);
    BackgroundCheckResponse {
        id:         row.id,
        check_type: row.check_type,
        provider:   row.provider,
        status:     row.status,
        checked_at: row.checked_at,
        expires_at: row.expires_at,
        is_expired,
    }
}

/// HMAC-SHA256 of `data` using `key`, returned as a lowercase hex string.
fn hmac_hex(key: &str, data: &[u8]) -> String {
    let k   = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, key.as_bytes());
    let tag = ring::hmac::sign(&k, data);
    hex::encode(tag.as_ref())
}

/// Returns true when the check has status 'passed' and is not expired.
fn is_valid_pass(row: &BackgroundCheckRow) -> bool {
    row.status == "passed"
        && row.expires_at.map(|e| e > Utc::now()).unwrap_or(true)
}

// ── Commands ──────────────────────────────────────────────────────────────────

/// Initiate a background check for the authenticated user (BFIP Section 3b).
///
/// Requires:
///   - Identity verification complete (status != 'registered')
///   - Most recent identity credential has cooling_completed_at set
///   - For criminal checks: sanctions AND identity_fraud must already be passed
///   - No pending check of the same type already exists
pub async fn initiate_check(
    pool:      &PgPool,
    user_id:   UserId,
    req:       InitiateCheckRequest,
    event_bus: &EventBus,
) -> AppResult<BackgroundCheckResponse> {
    let uid = i32::from(user_id);

    // 1. User must exist and not be banned.
    let user = user_repo::find_by_id(pool, user_id)
        .await?
        .ok_or(DomainError::Unauthorized)?;
    if user.is_banned { return Err(DomainError::Forbidden); }

    // 2. Identity verification must be complete.
    if user.verification_status == "registered" {
        return Err(DomainError::Forbidden);
    }

    // 3. Latest identity credential must have cooling complete.
    let cred = ic_repo::get_latest_credential_by_user(pool, uid)
        .await?
        .ok_or(DomainError::Forbidden)?;
    if cred.cooling_completed_at.is_none() {
        return Err(DomainError::Forbidden);
    }

    // 4. Validate check_type and enforce ordering constraints.
    let allowed_types = ["sanctions", "identity_fraud", "criminal"];
    if !allowed_types.contains(&req.check_type.as_str()) {
        return Err(DomainError::invalid_input(
            "check_type must be one of: sanctions, identity_fraud, criminal",
        ));
    }
    let allowed_providers = ["comply_advantage", "refinitiv", "lexisnexis", "socure"];
    if !allowed_providers.contains(&req.provider.as_str()) {
        return Err(DomainError::invalid_input(
            "provider must be one of: comply_advantage, refinitiv, lexisnexis, socure",
        ));
    }

    if req.check_type == "criminal" {
        let sanctions = repository::get_latest_check_by_type(pool, uid, "sanctions").await?;
        let fraud     = repository::get_latest_check_by_type(pool, uid, "identity_fraud").await?;
        let sanctions_ok = sanctions.as_ref().map(is_valid_pass).unwrap_or(false);
        let fraud_ok     = fraud.as_ref().map(is_valid_pass).unwrap_or(false);
        if !sanctions_ok || !fraud_ok {
            return Err(DomainError::Forbidden);
        }
    }

    // 5. No pending check of this type already in flight.
    let pending: Option<i32> = sqlx::query_scalar(
        "SELECT id FROM background_checks \
         WHERE user_id = $1 AND check_type = $2 AND status = 'pending' \
         LIMIT 1"
    )
    .bind(uid)
    .bind(&req.check_type)
    .fetch_optional(pool)
    .await
    .map_err(DomainError::Db)?;

    if pending.is_some() {
        return Err(DomainError::conflict("a pending check of this type already exists"));
    }

    // 6. Create the check record.
    let check = repository::create_check(
        pool,
        uid,
        cred.id,
        &req.provider,
        &req.check_type,
    ).await?;

    // 7. Audit event.
    audit::write(
        pool,
        Some(uid),
        None,
        "background_check.initiated",
        serde_json::json!({ "check_type": &check.check_type, "provider": &check.provider }),
    ).await;

    // 7b. Verification event.
    if let Err(e) = sqlx::query(
        "INSERT INTO verification_events \
         (user_id, event_type, actor_id, metadata) \
         VALUES ($1, 'background_check_initiated', $1, $2)"
    )
    .bind(uid)
    .bind(serde_json::json!({ "check_id": check.id, "check_type": &check.check_type }))
    .execute(pool)
    .await
    {
        tracing::error!(error = %e, "verification_events (background_check_initiated) insert failed");
    }

    // 8. Publish domain event.
    event_bus.publish(DomainEvent::BackgroundCheckInitiated {
        user_id:    uid,
        check_id:   check.id,
        check_type: check.check_type.clone(),
    });

    Ok(to_response(check))
}

/// Process a background check webhook from a provider (BFIP Section 3b).
///
/// Looks up the check by external_check_id, stamps result + response_hash,
/// writes verification and audit events, and advances status. Returns Ok(())
/// for unknown external_check_ids (multi-env safety).
pub async fn handle_webhook(
    pool:               &PgPool,
    payload:            CheckWebhookPayload,
    raw_payload_bytes:  &[u8],
    webhook_hmac_key:   &str,
    event_bus:          &EventBus,
) -> AppResult<()> {
    // 1. Look up the check.
    let Some(check) = repository::get_check_by_external_id(pool, &payload.external_check_id).await? else {
        tracing::warn!(
            external_check_id = %payload.external_check_id,
            "background check webhook for unknown external_check_id — ignoring"
        );
        return Ok(());
    };

    let uid = check.user_id;

    // 2. Compute HMAC-SHA256 of raw payload as response_hash.
    let response_hash = hmac_hex(webhook_hmac_key, raw_payload_bytes);

    // 3. expires_at = now + 12 months (BFIP Section 3b default).
    let expires_at = Utc::now() + chrono::Duration::days(365);

    // 4. Update the check.
    let updated = repository::update_check_result(
        pool,
        check.id,
        &payload.status,
        Some(&payload.external_check_id),
        Some(&response_hash),
        Some(Utc::now()),
        Some(expires_at),
    ).await?;

    // 5–7. Write events based on outcome.
    match payload.status.as_str() {
        "passed" => {
            if let Err(e) = sqlx::query(
                "INSERT INTO verification_events \
                 (user_id, event_type, actor_id, metadata) \
                 VALUES ($1, 'background_check_passed', $1, $2)"
            )
            .bind(uid)
            .bind(serde_json::json!({
                "check_id":   updated.id,
                "check_type": &updated.check_type,
            }))
            .execute(pool)
            .await
            {
                tracing::error!(error = %e, "verification_events (background_check_passed) insert failed");
            }

            // Record cleared_status_granted for criminal checks (BFIP Section 7b).
            if updated.check_type == "criminal" {
                if let Err(e) = sqlx::query(
                    "INSERT INTO verification_events \
                     (user_id, event_type, actor_id, metadata) \
                     VALUES ($1, 'cleared_status_granted', $1, $2)"
                )
                .bind(uid)
                .bind(serde_json::json!({ "check_id": updated.id }))
                .execute(pool)
                .await
                {
                    tracing::error!(error = %e, "verification_events (cleared_status_granted) insert failed");
                }
            }

            audit::write(
                pool,
                Some(uid),
                None,
                "background_check.passed",
                serde_json::json!({ "check_id": updated.id, "check_type": &updated.check_type }),
            ).await;

            event_bus.publish(DomainEvent::BackgroundCheckPassed {
                user_id:    uid,
                check_id:   updated.id,
                check_type: updated.check_type.clone(),
            });
        }

        "failed" => {
            if let Err(e) = sqlx::query(
                "INSERT INTO verification_events \
                 (user_id, event_type, actor_id, metadata) \
                 VALUES ($1, 'background_check_failed', $1, $2)"
            )
            .bind(uid)
            .bind(serde_json::json!({
                "check_id":   updated.id,
                "check_type": &updated.check_type,
            }))
            .execute(pool)
            .await
            {
                tracing::error!(error = %e, "verification_events (background_check_failed) insert failed");
            }

            audit::write(
                pool,
                Some(uid),
                None,
                "background_check.failed",
                serde_json::json!({ "check_id": updated.id, "check_type": &updated.check_type }),
            ).await;

            event_bus.publish(DomainEvent::BackgroundCheckFailed {
                user_id:    uid,
                check_id:   updated.id,
                check_type: updated.check_type.clone(),
            });
        }

        "review_required" => {
            if let Err(e) = sqlx::query(
                "INSERT INTO verification_events \
                 (user_id, event_type, actor_id, metadata) \
                 VALUES ($1, 'background_check_review_required', $1, $2)"
            )
            .bind(uid)
            .bind(serde_json::json!({
                "check_id":   updated.id,
                "check_type": &updated.check_type,
            }))
            .execute(pool)
            .await
            {
                tracing::error!(error = %e, "verification_events (background_check_review_required) insert failed");
            }

            audit::write(
                pool,
                Some(uid),
                None,
                "background_check.review_required",
                serde_json::json!({ "check_id": updated.id, "check_type": &updated.check_type }),
            ).await;
        }

        other => {
            tracing::warn!(status = other, "background check webhook with unknown status — ignoring");
        }
    }

    Ok(())
}

// ── Queries ───────────────────────────────────────────────────────────────────

/// Return the aggregate background check status for a user.
pub async fn get_status(
    pool:    &PgPool,
    user_id: UserId,
) -> AppResult<BackgroundCheckStatusResponse> {
    let uid    = i32::from(user_id);
    let checks = repository::get_checks_by_user(pool, uid).await?;

    // Latest of each type (list is DESC by created_at, so first match = latest).
    let sanctions_row = checks.iter().find(|c| c.check_type == "sanctions");
    let fraud_row     = checks.iter().find(|c| c.check_type == "identity_fraud");
    let criminal_row  = checks.iter().find(|c| c.check_type == "criminal");

    let sanctions_passed      = sanctions_row.map(is_valid_pass).unwrap_or(false);
    let identity_fraud_passed = fraud_row.map(is_valid_pass).unwrap_or(false);
    let criminal_passed       = criminal_row.map(is_valid_pass).unwrap_or(false);

    let all_required_passed = sanctions_passed && identity_fraud_passed;
    let cleared_eligible    = all_required_passed && criminal_passed;

    Ok(BackgroundCheckStatusResponse {
        user_id:               uid,
        sanctions_status:      sanctions_row.map(|c| c.status.clone()),
        identity_fraud_status: fraud_row.map(|c| c.status.clone()),
        criminal_status:       criminal_row.map(|c| c.status.clone()),
        all_required_passed,
        cleared_eligible,
        checks: checks.into_iter().map(to_response).collect(),
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{event_bus::EventBus, types::UserId};
    use chrono::Duration;
    use sqlx::PgPool;

    // ── Fixtures ──────────────────────────────────────────────────────────────

    async fn create_identity_confirmed_user(pool: &PgPool, email: &str) -> UserId {
        let (id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified, verification_status) \
             VALUES ($1, true, 'identity_confirmed') RETURNING id"
        )
        .bind(email).fetch_one(pool).await.unwrap();
        UserId::from(id)
    }

    async fn create_registered_user(pool: &PgPool, email: &str) -> UserId {
        let (id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified, verification_status) \
             VALUES ($1, true, 'registered') RETURNING id"
        )
        .bind(email).fetch_one(pool).await.unwrap();
        UserId::from(id)
    }

    /// Create an identity credential with cooling already complete.
    async fn create_completed_credential(pool: &PgPool, user_id: i32) -> i32 {
        let verified_at     = Utc::now() - Duration::days(10);
        let cooling_ends_at = verified_at + Duration::days(7);
        let (id,): (i32,) = sqlx::query_as(
            "INSERT INTO identity_credentials \
             (user_id, credential_type, verified_at, cooling_ends_at, cooling_completed_at) \
             VALUES ($1, 'stripe_identity', $2, $3, now()) RETURNING id"
        )
        .bind(user_id).bind(verified_at).bind(cooling_ends_at)
        .fetch_one(pool).await.unwrap();
        id
    }

    /// Create an identity credential WITHOUT cooling complete.
    async fn create_incomplete_credential(pool: &PgPool, user_id: i32) -> i32 {
        let verified_at     = Utc::now() - Duration::days(2);
        let cooling_ends_at = verified_at + Duration::days(7);
        let (id,): (i32,) = sqlx::query_as(
            "INSERT INTO identity_credentials \
             (user_id, credential_type, verified_at, cooling_ends_at) \
             VALUES ($1, 'stripe_identity', $2, $3) RETURNING id"
        )
        .bind(user_id).bind(verified_at).bind(cooling_ends_at)
        .fetch_one(pool).await.unwrap();
        id
    }

    fn req(check_type: &str) -> InitiateCheckRequest {
        InitiateCheckRequest {
            check_type: check_type.to_owned(),
            provider:   "comply_advantage".to_owned(),
        }
    }

    fn webhook(external_id: &str, status: &str) -> CheckWebhookPayload {
        CheckWebhookPayload {
            external_check_id: external_id.to_owned(),
            status:            status.to_owned(),
            provider:          "comply_advantage".to_owned(),
            raw_response:      serde_json::json!({ "result": status }),
        }
    }

    // ── Tests 1–4: initiate_check ─────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn initiate_check_succeeds_for_identity_confirmed_user_with_cooling_complete(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let uid = create_identity_confirmed_user(&pool, &SafeEmail().fake::<String>()).await;
        create_completed_credential(&pool, i32::from(uid)).await;
        let bus = EventBus::new();

        let resp = initiate_check(&pool, uid, req("sanctions"), &bus)
            .await.expect("initiate_check must succeed");

        assert_eq!(resp.check_type, "sanctions");
        assert_eq!(resp.status,     "pending");
        assert!(!resp.is_expired);
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn initiate_check_fails_if_cooling_not_complete(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let uid = create_identity_confirmed_user(&pool, &SafeEmail().fake::<String>()).await;
        create_incomplete_credential(&pool, i32::from(uid)).await;
        let bus = EventBus::new();

        let err = initiate_check(&pool, uid, req("sanctions"), &bus)
            .await.unwrap_err();
        assert!(matches!(err, DomainError::Forbidden), "expected Forbidden, got: {err:?}");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn initiate_check_fails_if_not_identity_confirmed(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let uid = create_registered_user(&pool, &SafeEmail().fake::<String>()).await;
        let bus = EventBus::new();

        let err = initiate_check(&pool, uid, req("sanctions"), &bus)
            .await.unwrap_err();
        assert!(matches!(err, DomainError::Forbidden), "expected Forbidden, got: {err:?}");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn initiate_check_fails_if_criminal_requested_before_required_checks_pass(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let uid = create_identity_confirmed_user(&pool, &SafeEmail().fake::<String>()).await;
        create_completed_credential(&pool, i32::from(uid)).await;
        let bus = EventBus::new();

        let err = initiate_check(&pool, uid, InitiateCheckRequest {
            check_type: "criminal".to_owned(),
            provider:   "comply_advantage".to_owned(),
        }, &bus).await.unwrap_err();

        assert!(matches!(err, DomainError::Forbidden),
            "criminal check before required checks must be Forbidden, got: {err:?}");
    }

    // ── Tests 5–7: handle_webhook ─────────────────────────────────────────────

    async fn seed_pending_check_with_external_id(
        pool: &PgPool,
        uid: UserId,
        check_type: &str,
        external_id: &str,
    ) -> i32 {
        let cred_id = create_completed_credential(&pool, i32::from(uid)).await;
        let check = repository::create_check(
            &pool, i32::from(uid), cred_id, "comply_advantage", check_type,
        ).await.unwrap();
        sqlx::query(
            "UPDATE background_checks SET external_check_id = $1 WHERE id = $2"
        )
        .bind(external_id).bind(check.id)
        .execute(pool).await.unwrap();
        check.id
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn handle_webhook_passed_updates_status_and_writes_events(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let uid = create_identity_confirmed_user(&pool, &SafeEmail().fake::<String>()).await;
        let check_id = seed_pending_check_with_external_id(
            &pool, uid, "sanctions", "ext-pass-001",
        ).await;
        let bus = EventBus::new();

        let payload = webhook("ext-pass-001", "passed");
        let raw     = serde_json::to_vec(&payload).unwrap();

        handle_webhook(&pool, payload, &raw, "test-key", &bus).await
            .expect("webhook must succeed");

        let status: String = sqlx::query_scalar(
            "SELECT status FROM background_checks WHERE id = $1"
        ).bind(check_id).fetch_one(&pool).await.unwrap();
        assert_eq!(status, "passed");

        let ve_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM verification_events \
             WHERE user_id = $1 AND event_type = 'background_check_passed'"
        ).bind(i32::from(uid)).fetch_one(&pool).await.unwrap();
        assert_eq!(ve_count, 1, "verification_event must be written");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn handle_webhook_failed_updates_status_and_writes_events(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let uid = create_identity_confirmed_user(&pool, &SafeEmail().fake::<String>()).await;
        seed_pending_check_with_external_id(&pool, uid, "sanctions", "ext-fail-001").await;
        let bus = EventBus::new();

        let payload = webhook("ext-fail-001", "failed");
        let raw     = serde_json::to_vec(&payload).unwrap();

        handle_webhook(&pool, payload, &raw, "test-key", &bus).await
            .expect("webhook must succeed");

        let (status, ve_count): (String, i64) = sqlx::query_as(
            "SELECT bc.status, COUNT(ve.id) \
             FROM background_checks bc \
             LEFT JOIN verification_events ve \
               ON ve.user_id = bc.user_id AND ve.event_type = 'background_check_failed' \
             WHERE bc.external_check_id = 'ext-fail-001' \
             GROUP BY bc.status"
        ).fetch_one(&pool).await.unwrap();

        assert_eq!(status, "failed");
        assert_eq!(ve_count, 1);
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn handle_webhook_computes_response_hash_correctly(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let uid = create_identity_confirmed_user(&pool, &SafeEmail().fake::<String>()).await;
        seed_pending_check_with_external_id(&pool, uid, "sanctions", "ext-hash-001").await;
        let bus = EventBus::new();

        let payload = webhook("ext-hash-001", "passed");
        let raw     = serde_json::to_vec(&payload).unwrap();

        // Compute expected hash before consuming payload.
        let expected_hash = hmac_hex("hmac-test-key", &raw);

        handle_webhook(&pool, payload, &raw, "hmac-test-key", &bus).await.unwrap();

        let stored_hash: Option<String> = sqlx::query_scalar(
            "SELECT response_hash FROM background_checks WHERE external_check_id = 'ext-hash-001'"
        ).fetch_one(&pool).await.unwrap();

        assert_eq!(stored_hash.as_deref(), Some(expected_hash.as_str()),
            "stored response_hash must match HMAC-SHA256 of raw payload");
    }

    // ── Tests 8–10: get_status ────────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn get_status_returns_all_required_passed_when_both_checks_pass(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let uid = create_identity_confirmed_user(&pool, &SafeEmail().fake::<String>()).await;
        let bus = EventBus::new();

        for check_type in ["sanctions", "identity_fraud"] {
            let ext_id = format!("ext-{check_type}-ok");
            seed_pending_check_with_external_id(&pool, uid, check_type, &ext_id).await;
            let payload = webhook(&ext_id, "passed");
            let raw = serde_json::to_vec(&payload).unwrap();
            handle_webhook(&pool, payload, &raw, "k", &bus).await.unwrap();
        }

        let status = get_status(&pool, uid).await.unwrap();
        assert!(status.all_required_passed, "both checks passed → all_required_passed must be true");
        assert!(!status.cleared_eligible,   "no criminal check → cleared_eligible must be false");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn get_status_returns_cleared_eligible_when_criminal_also_passes(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let uid = create_identity_confirmed_user(&pool, &SafeEmail().fake::<String>()).await;
        let bus = EventBus::new();

        // Pass required checks first.
        for check_type in ["sanctions", "identity_fraud"] {
            let ext_id = format!("ext-{check_type}-clr");
            seed_pending_check_with_external_id(&pool, uid, check_type, &ext_id).await;
            let payload = webhook(&ext_id, "passed");
            let raw = serde_json::to_vec(&payload).unwrap();
            handle_webhook(&pool, payload, &raw, "k", &bus).await.unwrap();
        }

        // Now initiate criminal check (should be allowed since required checks passed).
        let criminal = initiate_check(&pool, uid, InitiateCheckRequest {
            check_type: "criminal".to_owned(),
            provider:   "comply_advantage".to_owned(),
        }, &bus).await.expect("criminal check must be initiatable after required checks pass");

        // Set external_check_id on the criminal check.
        sqlx::query("UPDATE background_checks SET external_check_id = 'ext-criminal-clr' WHERE id = $1")
            .bind(criminal.id).execute(&pool).await.unwrap();

        let payload = webhook("ext-criminal-clr", "passed");
        let raw = serde_json::to_vec(&payload).unwrap();
        handle_webhook(&pool, payload, &raw, "k", &bus).await.unwrap();

        let status = get_status(&pool, uid).await.unwrap();
        assert!(status.all_required_passed, "all_required_passed must be true");
        assert!(status.cleared_eligible,    "cleared_eligible must be true after criminal passes");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn get_status_marks_expired_checks_correctly(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let uid = create_identity_confirmed_user(&pool, &SafeEmail().fake::<String>()).await;
        let bus = EventBus::new();

        for check_type in ["sanctions", "identity_fraud"] {
            let ext_id = format!("ext-{check_type}-exp");
            seed_pending_check_with_external_id(&pool, uid, check_type, &ext_id).await;
            let payload = webhook(&ext_id, "passed");
            let raw = serde_json::to_vec(&payload).unwrap();
            handle_webhook(&pool, payload, &raw, "k", &bus).await.unwrap();
        }

        // Force expires_at into the past.
        sqlx::query(
            "UPDATE background_checks \
             SET expires_at = now() - interval '1 day' \
             WHERE user_id = $1"
        )
        .bind(i32::from(uid)).execute(&pool).await.unwrap();

        let status = get_status(&pool, uid).await.unwrap();
        assert!(
            !status.all_required_passed,
            "expired checks must not count — all_required_passed must be false"
        );
        let any_expired = status.checks.iter().any(|c| c.is_expired);
        assert!(any_expired, "at least one check must be marked is_expired = true");
    }

    // ── Adversarial tests ─────────────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_initiate_criminal_check_before_required_checks(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let uid = create_identity_confirmed_user(&pool, &SafeEmail().fake::<String>()).await;
        create_completed_credential(&pool, i32::from(uid)).await;
        let bus = EventBus::new();

        let err = initiate_check(&pool, uid, InitiateCheckRequest {
            check_type: "criminal".to_owned(),
            provider:   "comply_advantage".to_owned(),
        }, &bus).await.unwrap_err();
        assert!(matches!(err, DomainError::Forbidden));
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_initiate_duplicate_pending_check(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let uid = create_identity_confirmed_user(&pool, &SafeEmail().fake::<String>()).await;
        create_completed_credential(&pool, i32::from(uid)).await;
        let bus = EventBus::new();

        initiate_check(&pool, uid, req("sanctions"), &bus).await.unwrap();

        let err = initiate_check(&pool, uid, req("sanctions"), &bus).await.unwrap_err();
        assert!(matches!(err, DomainError::Conflict(_)),
            "duplicate pending check must be Conflict, got: {err:?}");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_initiate_check_without_completed_cooling(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let uid = create_identity_confirmed_user(&pool, &SafeEmail().fake::<String>()).await;
        create_incomplete_credential(&pool, i32::from(uid)).await;
        let bus = EventBus::new();

        let err = initiate_check(&pool, uid, req("sanctions"), &bus).await.unwrap_err();
        assert!(matches!(err, DomainError::Forbidden));
    }
}
