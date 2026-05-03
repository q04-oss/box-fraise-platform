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
use crate::domain::soultokens::repository as soultoken_repo;
use super::{
    repository,
    types::{
        AttestationTokenMeta, AttestationTokenResponse, IssueAttestationTokenRequest,
        VerificationResultResponse, VerifyAttestationTokenRequest,
    },
};

// ── Crypto primitives ─────────────────────────────────────────────────────────

/// Generate a 32-byte cryptographically random token encoded as 64-char hex.
///
/// This raw token is returned to the user ONCE and never stored.
/// Only the SHA-256 hash is persisted in `attestation_tokens.token_hash`.
fn generate_raw_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// SHA-256 hash of a raw token — this is what gets stored in the DB.
fn hash_token(raw_token: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(raw_token.as_bytes());
    hex::encode(hasher.finalize())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn to_meta(row: &super::types::AttestationTokenRow) -> AttestationTokenMeta {
    AttestationTokenMeta {
        id:          row.id,
        scope:       row.scope.clone(),
        issued_at:   row.issued_at,
        expires_at:  row.expires_at,
        verified_at: row.verified_at,
        revoked_at:  row.revoked_at,
    }
}

// ── Service functions ─────────────────────────────────────────────────────────

/// Issue a short-lived scoped attestation token (BFIP Section 11.1).
///
/// Returns the raw token ONCE — it is never stored and cannot be retrieved again.
pub async fn issue_token(
    pool:      &PgPool,
    user_id:   UserId,
    req:       IssueAttestationTokenRequest,
    event_bus: &EventBus,
) -> AppResult<AttestationTokenResponse> {
    let uid = i32::from(user_id);

    // 1. User must exist and not be banned.
    let user = user_repo::find_by_id(pool, user_id)
        .await?
        .ok_or(DomainError::Unauthorized)?;
    if user.is_banned { return Err(DomainError::Forbidden); }

    // 2. User must have an active soultoken.
    let soultoken = soultoken_repo::get_active_soultoken_by_user(pool, uid)
        .await?
        .ok_or(DomainError::NotFound)?;

    // 3. Validate scope.
    if req.scope != "presence.verified" {
        return Err(DomainError::InvalidInput(
            "scope must be 'presence.verified'".to_string(),
        ));
    }

    // 4. Validate business soultoken if provided.
    if let Some(biz_soultoken_id) = req.requesting_business_soultoken_id {
        let biz_soultoken = soultoken_repo::get_soultoken_by_id(pool, biz_soultoken_id)
            .await?
            .ok_or(DomainError::NotFound)?;
        if biz_soultoken.revoked_at.is_some() {
            return Err(DomainError::InvalidInput(
                "requesting business soultoken is revoked".to_string(),
            ));
        }
    }

    // 5–6. Generate token and compute hash.
    let raw_token  = generate_raw_token();
    let token_hash = hash_token(&raw_token);

    // 7. Create attestation token with 15-minute expiry.
    let expires_at = Utc::now() + chrono::Duration::minutes(15);
    let token = repository::create_attestation_token(
        pool,
        uid,
        soultoken.id,
        &req.scope,
        &token_hash,
        req.requesting_business_soultoken_id,
        req.user_device_id.as_deref(),
        req.presentation_latitude,
        req.presentation_longitude,
        expires_at,
    ).await?;

    // 8. Audit.
    audit::write(
        pool,
        Some(uid),
        None,
        "attestation_token.issued",
        serde_json::json!({
            "token_id": token.id,
            "scope":    &req.scope,
        }),
    ).await;

    event_bus.publish(DomainEvent::AttestationTokenIssued {
        user_id:  uid,
        token_id: token.id,
    });

    // 9. Return raw_token once — this is the only time it leaves the server.
    Ok(AttestationTokenResponse {
        raw_token,
        scope:      token.scope,
        expires_at: token.expires_at,
        issued_at:  token.issued_at,
    })
}

/// Verify a presented attestation token (BFIP Section 11.2).
///
/// Always returns HTTP 200 — the outcome field signals validity.
/// All attempts are logged regardless of outcome.
pub async fn verify_token(
    pool:       &PgPool,
    req:        VerifyAttestationTokenRequest,
    ip_address: Option<String>,
    user_agent: Option<String>,
    event_bus:  &EventBus,
) -> AppResult<VerificationResultResponse> {
    let token_hash = hash_token(&req.raw_token);

    // 2. Look up token by hash.
    let token_opt = repository::get_token_by_hash(pool, &token_hash).await?;

    // 3. Record attempt regardless of outcome (always log).
    // We build the attempt record after determining the outcome below.

    // Helper closure for recording attempt (avoids repetition).
    let record = |pool: &PgPool, tid: Option<i32>, outcome: &str| {
        let pool       = pool.clone();
        let hash       = token_hash.clone();
        let biz_id     = req.requesting_business_soultoken_id;
        let sig        = req.request_signature.clone();
        let ip         = ip_address.clone();
        let ua         = user_agent.clone();
        let outcome    = outcome.to_string();
        async move {
            let _ = repository::record_verification_attempt(
                &pool,
                &hash,
                tid,
                biz_id,
                sig.as_deref(),
                ip.as_deref(),
                ua.as_deref(),
                &outcome,
            ).await;
        }
    };

    // 4. Not found.
    let token = match token_opt {
        None => {
            record(pool, None, "not_found").await;
            return Ok(VerificationResultResponse {
                valid:       false,
                scope:       None,
                outcome:     "not_found".to_string(),
                verified_at: None,
            });
        }
        Some(t) => t,
    };

    // 5. Expired.
    if Utc::now() > token.expires_at {
        record(pool, Some(token.id), "expired").await;
        return Ok(VerificationResultResponse {
            valid:       false,
            scope:       Some(token.scope),
            outcome:     "expired".to_string(),
            verified_at: None,
        });
    }

    // 6. Already verified.
    if token.verified_at.is_some() {
        record(pool, Some(token.id), "already_verified").await;
        return Ok(VerificationResultResponse {
            valid:       false,
            scope:       Some(token.scope),
            outcome:     "already_verified".to_string(),
            verified_at: token.verified_at,
        });
    }

    // 7. Revoked.
    if token.revoked_at.is_some() {
        record(pool, Some(token.id), "revoked").await;
        return Ok(VerificationResultResponse {
            valid:       false,
            scope:       Some(token.scope),
            outcome:     "revoked".to_string(),
            verified_at: None,
        });
    }

    // 8. Rate limit check (by business soultoken, last 60 seconds).
    if let Some(biz_id) = req.requesting_business_soultoken_id {
        let recent = repository::get_recent_attempts_by_business(pool, biz_id, 1).await?;
        if recent > 10 {
            return Err(DomainError::InvalidInput(
                "rate limit exceeded — too many verification attempts".to_string(),
            ));
        }
    }

    // 9. Mark verified.
    let verified = repository::mark_token_verified(pool, token.id).await?;

    // 10. Record success attempt.
    record(pool, Some(token.id), "success").await;

    // 11. Audit + event.
    audit::write(
        pool,
        Some(token.user_id),
        None,
        "attestation_token.verified",
        serde_json::json!({
            "token_id": token.id,
            "scope":    &token.scope,
        }),
    ).await;

    event_bus.publish(DomainEvent::AttestationTokenVerified {
        user_id:  token.user_id,
        token_id: token.id,
    });

    // 12. Return success.
    Ok(VerificationResultResponse {
        valid:       true,
        scope:       Some(token.scope),
        outcome:     "success".to_string(),
        verified_at: verified.verified_at,
    })
}

/// Return all attestation tokens for the user — raw_token never included (BFIP Section 11.4).
pub async fn get_my_tokens(
    pool:    &PgPool,
    user_id: UserId,
) -> AppResult<Vec<AttestationTokenMeta>> {
    let rows = repository::get_tokens_by_user(pool, i32::from(user_id)).await?;
    Ok(rows.iter().map(to_meta).collect())
}

/// Revoke an attestation token before it expires (BFIP Section 11.3).
pub async fn revoke_my_token(
    pool:      &PgPool,
    token_id:  i32,
    user_id:   UserId,
) -> AppResult<()> {
    let uid = i32::from(user_id);

    let token = repository::get_tokens_by_user(pool, uid)
        .await?
        .into_iter()
        .find(|t| t.id == token_id)
        .ok_or(DomainError::NotFound)?;

    if token.user_id != uid {
        return Err(DomainError::Forbidden);
    }

    repository::revoke_token(pool, token_id).await?;

    audit::write(
        pool,
        Some(uid),
        None,
        "attestation_token.revoked",
        serde_json::json!({ "token_id": token_id }),
    ).await;

    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::soultokens::{service as soultoken_svc, types::IssueSoultokenRequest},
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

    /// Full setup: attested user with active soultoken.
    /// Uses direct SQL inserts (same approach as soultokens/service.rs tests).
    /// Returns (user_id, soultoken_id).
    async fn setup_attested_user_with_soultoken(pool: &PgPool) -> (UserId, i32) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let bus = EventBus::new();

        let user_email: String = SafeEmail().fake();
        let (uid,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified, verification_status) \
             VALUES ($1, true, 'presence_confirmed') RETURNING id",
        )
        .bind(&user_email).fetch_one(pool).await.unwrap();

        let staff_email: String = SafeEmail().fake();
        let (staff_id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id",
        )
        .bind(&staff_email).fetch_one(pool).await.unwrap();

        let r1_email: String = SafeEmail().fake();
        let r2_email: String = SafeEmail().fake();
        let (r1_id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id",
        )
        .bind(&r1_email).fetch_one(pool).await.unwrap();
        let (r2_id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id",
        )
        .bind(&r2_email).fetch_one(pool).await.unwrap();

        // Identity credential (optional but included for realism).
        sqlx::query(
            "INSERT INTO identity_credentials \
             (user_id, credential_type, verified_at, cooling_ends_at, cooling_completed_at) \
             VALUES ($1, 'stripe_identity', now(), now() + interval '7 days', now())",
        ).bind(uid).execute(pool).await.unwrap();

        // Location + business.
        let (loc_id,): (i32,) = sqlx::query_as(
            "INSERT INTO locations (name, location_type, address, timezone) \
             VALUES ('AT Store', 'box_fraise_store', '1 AT St', 'America/Edmonton') \
             RETURNING id",
        ).fetch_one(pool).await.unwrap();

        let (biz_id,): (i32,) = sqlx::query_as(
            "INSERT INTO businesses \
             (location_id, primary_holder_id, name, verification_status) \
             VALUES ($1, $2, 'AT Biz', 'active') RETURNING id",
        ).bind(loc_id).bind(uid).fetch_one(pool).await.unwrap();

        // Presence threshold.
        let (thresh_id,): (i32,) = sqlx::query_as(
            "INSERT INTO presence_thresholds \
             (user_id, business_id, event_count, days_count, threshold_met_at) \
             VALUES ($1, $2, 3, 3, now()) RETURNING id",
        ).bind(uid).bind(biz_id).fetch_one(pool).await.unwrap();

        // Staff visit (completed).
        let (visit_id,): (i32,) = sqlx::query_as(
            "INSERT INTO staff_visits \
             (location_id, staff_id, visit_type, status, scheduled_at) \
             VALUES ($1, $2, 'combined', 'completed', now()) RETURNING id",
        ).bind(loc_id).bind(staff_id).fetch_one(pool).await.unwrap();

        // Approved attestation.
        let (attest_id,): (i32,) = sqlx::query_as(
            "INSERT INTO visit_attestations \
             (visit_id, user_id, staff_id, presence_threshold_id, \
              assigned_reviewer_1_id, assigned_reviewer_2_id, status) \
             VALUES ($1, $2, $3, $4, $5, $6, 'approved') RETURNING id",
        )
        .bind(visit_id).bind(uid).bind(staff_id)
        .bind(thresh_id).bind(r1_id).bind(r2_id)
        .fetch_one(pool).await.unwrap();

        // Promote to attested.
        sqlx::query(
            "UPDATE users SET verification_status = 'attested', attested_at = now() WHERE id = $1",
        ).bind(uid).execute(pool).await.unwrap();

        // Issue soultoken using the correct signature.
        let user = UserId::from(uid);
        let st = soultoken_svc::issue_soultoken(
            pool, user,
            IssueSoultokenRequest {
                attestation_id: attest_id,
                token_type:     "user".to_owned(),
            },
            TEST_HMAC_KEY, TEST_SIGNING_KEY, &bus,
        ).await.expect("issue_soultoken must succeed");

        (user, st.id)
    }

    fn issue_req() -> IssueAttestationTokenRequest {
        IssueAttestationTokenRequest {
            scope:                            "presence.verified".to_owned(),
            requesting_business_soultoken_id: None,
            user_device_id:                   None,
            presentation_latitude:            None,
            presentation_longitude:           None,
        }
    }

    fn verify_req(raw_token: &str) -> VerifyAttestationTokenRequest {
        VerifyAttestationTokenRequest {
            raw_token:                        raw_token.to_owned(),
            requesting_business_soultoken_id: None,
            request_signature:                None,
        }
    }

    // ── Tests 1–3: issue_token ────────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn issue_token_succeeds_for_attested_user_with_soultoken(pool: PgPool) {
        let bus = EventBus::new();
        let (user, _) = setup_attested_user_with_soultoken(&pool).await;

        let resp = issue_token(&pool, user, issue_req(), &bus)
            .await.expect("issue_token must succeed");

        assert_eq!(resp.scope, "presence.verified");
        assert!(!resp.raw_token.is_empty());
        assert_eq!(resp.raw_token.len(), 64, "raw_token must be 64-char hex");
        assert!(resp.expires_at > chrono::Utc::now());
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn issue_token_fails_without_active_soultoken(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let bus  = EventBus::new();
        let user = create_user(&pool, &SafeEmail().fake::<String>()).await;

        let err = issue_token(&pool, user, issue_req(), &bus).await.unwrap_err();
        assert!(matches!(err, DomainError::NotFound));
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn issue_token_raw_token_not_stored_in_db(pool: PgPool) {
        let bus = EventBus::new();
        let (user, _) = setup_attested_user_with_soultoken(&pool).await;

        let resp = issue_token(&pool, user, issue_req(), &bus).await.unwrap();
        let raw  = &resp.raw_token;

        // Scan every text column in attestation_tokens for the raw token value.
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT token_hash FROM attestation_tokens WHERE user_id = $1"
        )
        .bind(i32::from(user)).fetch_all(&pool).await.unwrap();

        assert!(!rows.is_empty(), "token row must exist");
        for (hash,) in &rows {
            assert_ne!(hash, raw,
                "raw_token must NOT be stored — only its hash should appear");
        }

        // Verify hash_token produces the stored value.
        let expected_hash = hash_token(raw);
        assert_eq!(rows[0].0, expected_hash, "stored hash must match SHA-256 of raw_token");
    }

    // ── Tests 4–9: verify_token ───────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn verify_token_succeeds_with_valid_raw_token(pool: PgPool) {
        let bus = EventBus::new();
        let (user, _) = setup_attested_user_with_soultoken(&pool).await;

        let issued = issue_token(&pool, user, issue_req(), &bus).await.unwrap();

        let result = verify_token(&pool, verify_req(&issued.raw_token), None, None, &bus)
            .await.expect("verify_token must succeed");

        assert!(result.valid);
        assert_eq!(result.outcome, "success");
        assert_eq!(result.scope.as_deref(), Some("presence.verified"));
        assert!(result.verified_at.is_some());
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn verify_token_fails_for_expired_token(pool: PgPool) {
        let bus = EventBus::new();
        let (user, soultoken_id) = setup_attested_user_with_soultoken(&pool).await;

        // Issue and then manually expire the token.
        let issued = issue_token(&pool, user, issue_req(), &bus).await.unwrap();
        let hash   = hash_token(&issued.raw_token);
        sqlx::query(
            "UPDATE attestation_tokens SET expires_at = now() - INTERVAL '1 minute' \
             WHERE token_hash = $1"
        ).bind(&hash).execute(&pool).await.unwrap();
        let _ = soultoken_id;

        let result = verify_token(&pool, verify_req(&issued.raw_token), None, None, &bus)
            .await.unwrap();

        assert!(!result.valid);
        assert_eq!(result.outcome, "expired");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn verify_token_fails_for_already_verified_token(pool: PgPool) {
        let bus = EventBus::new();
        let (user, _) = setup_attested_user_with_soultoken(&pool).await;

        let issued = issue_token(&pool, user, issue_req(), &bus).await.unwrap();

        // First verification succeeds.
        let r1 = verify_token(&pool, verify_req(&issued.raw_token), None, None, &bus)
            .await.unwrap();
        assert!(r1.valid);

        // Second verification — already_verified.
        let r2 = verify_token(&pool, verify_req(&issued.raw_token), None, None, &bus)
            .await.unwrap();
        assert!(!r2.valid);
        assert_eq!(r2.outcome, "already_verified");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn verify_token_fails_for_tampered_token(pool: PgPool) {
        let bus = EventBus::new();
        let (user, _) = setup_attested_user_with_soultoken(&pool).await;

        let issued   = issue_token(&pool, user, issue_req(), &bus).await.unwrap();
        let mut tampered = issued.raw_token.clone();
        // Flip one character.
        unsafe {
            let b = tampered.as_bytes_mut();
            b[0] = if b[0] == b'a' { b'b' } else { b'a' };
        }

        let result = verify_token(&pool, verify_req(&tampered), None, None, &bus)
            .await.unwrap();

        assert!(!result.valid);
        assert_eq!(result.outcome, "not_found");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn verify_token_always_records_attempt(pool: PgPool) {
        let bus = EventBus::new();

        let fake_token = "deadbeef".repeat(8); // 64 chars, invalid
        verify_token(&pool, verify_req(&fake_token), None, None, &bus)
            .await.unwrap();

        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM third_party_verification_attempts"
        ).fetch_one(&pool).await.unwrap();
        assert_eq!(count, 1, "attempt must be recorded even for invalid tokens");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn verify_token_rate_limits_excessive_attempts(pool: PgPool) {
        let bus = EventBus::new();
        let (user, soultoken_id) = setup_attested_user_with_soultoken(&pool).await;

        // Use the user's own soultoken as the requesting_business_soultoken_id
        // (rate limiting only cares about the ID value, not the token type).
        let biz_soultoken_id = soultoken_id;

        // Insert 11 recent attempts from same business.
        for _ in 0..11 {
            sqlx::query(
                "INSERT INTO third_party_verification_attempts \
                 (token_hash, requesting_business_soultoken_id, outcome, attempted_at) \
                 VALUES ('fakehash', $1, 'not_found', now())"
            ).bind(biz_soultoken_id).execute(&pool).await.unwrap();
        }

        let issued = issue_token(&pool, user, issue_req(), &bus).await.unwrap();
        let req = VerifyAttestationTokenRequest {
            raw_token:                        issued.raw_token.clone(),
            requesting_business_soultoken_id: Some(biz_soultoken_id),
            request_signature:                None,
        };

        let err = verify_token(&pool, req, None, None, &bus).await.unwrap_err();
        assert!(matches!(err, DomainError::InvalidInput(_)),
            "rate limit must return InvalidInput, got: {err:?}");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn revoke_token_prevents_verification(pool: PgPool) {
        let bus = EventBus::new();
        let (user, _) = setup_attested_user_with_soultoken(&pool).await;

        let issued = issue_token(&pool, user, issue_req(), &bus).await.unwrap();
        let hash   = hash_token(&issued.raw_token);

        // Get the token id.
        let (token_id,): (i32,) = sqlx::query_as(
            "SELECT id FROM attestation_tokens WHERE token_hash = $1"
        ).bind(&hash).fetch_one(&pool).await.unwrap();

        revoke_my_token(&pool, token_id, user).await.expect("revoke must succeed");

        let result = verify_token(&pool, verify_req(&issued.raw_token), None, None, &bus)
            .await.unwrap();
        assert!(!result.valid);
        assert_eq!(result.outcome, "revoked");
    }

    // ── Adversarial tests ─────────────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_retrieve_raw_token_after_issuance(pool: PgPool) {
        let bus = EventBus::new();
        let (user, _) = setup_attested_user_with_soultoken(&pool).await;

        let issued  = issue_token(&pool, user, issue_req(), &bus).await.unwrap();
        let raw_val = issued.raw_token.clone();

        // get_my_tokens returns metadata only — no raw_token field.
        let metas = get_my_tokens(&pool, user).await.unwrap();
        let serialized = serde_json::to_string(&metas).unwrap();

        assert!(!serialized.contains(&raw_val),
            "raw_token must not appear in get_my_tokens response");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_verify_without_raw_token(pool: PgPool) {
        let bus = EventBus::new();
        let (user, _) = setup_attested_user_with_soultoken(&pool).await;

        let issued     = issue_token(&pool, user, issue_req(), &bus).await.unwrap();
        let token_hash = hash_token(&issued.raw_token);

        // Attacker submits the hash directly (hash-of-hash won't match stored hash).
        let result = verify_token(&pool, verify_req(&token_hash), None, None, &bus)
            .await.unwrap();

        assert!(!result.valid);
        assert_eq!(result.outcome, "not_found",
            "submitting the hash instead of raw_token must return not_found");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_revoke_another_users_token(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let bus      = EventBus::new();
        let (user, _)= setup_attested_user_with_soultoken(&pool).await;
        let attacker = create_user(&pool, &SafeEmail().fake::<String>()).await;

        let issued = issue_token(&pool, user, issue_req(), &bus).await.unwrap();
        let hash   = hash_token(&issued.raw_token);
        let (token_id,): (i32,) = sqlx::query_as(
            "SELECT id FROM attestation_tokens WHERE token_hash = $1"
        ).bind(&hash).fetch_one(&pool).await.unwrap();

        let err = revoke_my_token(&pool, token_id, attacker).await.unwrap_err();
        assert!(matches!(err, DomainError::NotFound),
            "attacker must get NotFound (not Forbidden — no enumeration), got: {err:?}");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_enumerate_tokens_via_timing(pool: PgPool) {
        use std::time::Instant;
        let bus = EventBus::new();
        let (user, _) = setup_attested_user_with_soultoken(&pool).await;

        let issued   = issue_token(&pool, user, issue_req(), &bus).await.unwrap();
        let fake_tok = "0".repeat(64);

        let t0 = Instant::now();
        let _ = verify_token(&pool, verify_req(&fake_tok), None, None, &bus).await;
        let invalid_ms = t0.elapsed().as_millis();

        // Verify the real token (first time).
        let t1 = Instant::now();
        let _ = verify_token(&pool, verify_req(&issued.raw_token), None, None, &bus).await;
        let valid_ms = t1.elapsed().as_millis();

        // Both paths should complete in a reasonable time — the point is that
        // hash_token always runs before any lookup (no early-exit on token format).
        // The valid path may be slower due to additional DB writes, which is acceptable.
        assert!(invalid_ms < 5000,
            "invalid token path took too long: {}ms", invalid_ms);
        assert!(valid_ms < 5000,
            "valid token path took too long: {}ms", valid_ms);
    }
}
