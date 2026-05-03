use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

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
        IssueSoultokenRequest, RenewSoultokenRequest, RevokeSoultokenRequest,
        SoultokenResponse, SoultokenRenewalResponse, SoultokenRow,
        SurrenderSoultokenRequest,
    },
};

// ── Crypto primitives ─────────────────────────────────────────────────────────

/// BFIP cryptography.md Section 3 — Display code derivation.
///
/// input       = uuid.as_bytes() (16 raw bytes)
/// hmac_output = HMAC-SHA256(hmac_key, input)
/// base36      = base36_encode(hmac_output[0..9])
/// display_code = formatted as XXXX-XXXX-XXXX
pub fn derive_display_code(uuid: &Uuid, hmac_key: &[u8], _key_version: i32) -> String {
    use ring::hmac as ring_hmac;
    let key = ring_hmac::Key::new(ring_hmac::HMAC_SHA256, hmac_key);
    let tag = ring_hmac::sign(&key, uuid.as_bytes());
    let code = base36_encode(&tag.as_ref()[..9]);
    format!("{}-{}-{}", &code[0..4], &code[4..8], &code[8..12])
}

/// Encode `bytes` as a 12-character base-36 string (digits 0-9, letters A-Z).
fn base36_encode(bytes: &[u8]) -> String {
    const CHARS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let mut n: u128 = 0;
    for &b in bytes {
        n = (n << 8) | b as u128;
    }
    let mut digits = [0u8; 12];
    for i in (0..12).rev() {
        digits[i] = (n % 36) as u8;
        n /= 36;
    }
    digits.iter().map(|&d| CHARS[d as usize] as char).collect()
}

/// BFIP cryptography.md Section 4 — Soultoken signature.
///
/// Payload: uuid_str|holder_user_id|issued_at_rfc3339|expires_at_rfc3339|display_code
/// Signed with HMAC-SHA256 (Ed25519 reserved for future PKI pass).
pub fn sign_soultoken(
    uuid:           &Uuid,
    holder_user_id: i32,
    issued_at:      &chrono::DateTime<Utc>,
    expires_at:     &chrono::DateTime<Utc>,
    display_code:   &str,
    signing_key:    &[u8],
) -> String {
    use ring::hmac as ring_hmac;
    let payload = format!(
        "{}|{}|{}|{}|{}",
        uuid,
        holder_user_id,
        issued_at.to_rfc3339(),
        expires_at.to_rfc3339(),
        display_code,
    );
    let key = ring_hmac::Key::new(ring_hmac::HMAC_SHA256, signing_key);
    let tag = ring_hmac::sign(&key, payload.as_bytes());
    hex::encode(tag.as_ref())
}

/// Admin reference string — generated at display time only, never stored.
///
/// Format: "FRS-" + first 8 chars of uuid.to_string().to_uppercase()
pub fn admin_reference(uuid: &Uuid) -> String {
    let upper = uuid.to_string().to_uppercase();
    format!("FRS-{}", &upper[..8])
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn to_response(row: SoultokenRow) -> SoultokenResponse {
    let now = Utc::now();
    let is_expired = row.expires_at < now;
    let is_active  = row.revoked_at.is_none() && !is_expired;
    SoultokenResponse {
        id:              row.id,
        display_code:    row.display_code,
        token_type:      row.token_type,
        holder_user_id:  row.holder_user_id,
        issued_at:       row.issued_at,
        expires_at:      row.expires_at,
        last_renewed_at: row.last_renewed_at,
        revoked_at:      row.revoked_at,
        is_expired,
        is_active,
    }
}

fn to_renewal_response(row: super::types::SoultokenRenewalRow) -> SoultokenRenewalResponse {
    SoultokenRenewalResponse {
        soultoken_id:        row.soultoken_id,
        renewal_type:        row.renewal_type,
        previous_expires_at: row.previous_expires_at,
        new_expires_at:      row.new_expires_at,
        renewed_at:          row.renewed_at,
    }
}

// ── Service functions ─────────────────────────────────────────────────────────

/// Issue a soultoken after attestation is approved (BFIP Section 7).
///
/// The user must have `verification_status = 'attested'`.
/// Builds the full verification chain: identity_credential + presence_threshold + attestation.
pub async fn issue_soultoken(
    pool:        &PgPool,
    user_id:     UserId,
    req:         IssueSoultokenRequest,
    hmac_key:    &[u8],
    signing_key: &[u8],
    event_bus:   &EventBus,
) -> AppResult<SoultokenResponse> {
    let uid = i32::from(user_id);

    // 1. User must be attested.
    let user = user_repo::find_by_id(pool, user_id)
        .await?
        .ok_or(DomainError::Unauthorized)?;
    if user.verification_status != "attested" {
        return Err(DomainError::InvalidInput(
            "user must have verification_status 'attested' to receive a soultoken".to_string(),
        ));
    }

    // 2. Check user does not already have an active soultoken.
    if repository::get_active_soultoken_by_user(pool, uid).await?.is_some() {
        return Err(DomainError::Conflict(
            "user already has an active soultoken".to_string(),
        ));
    }

    // 3. Look up attestation — must be approved.
    let (attest_presence_threshold_id,): (i32,) = sqlx::query_as(
        "SELECT presence_threshold_id FROM visit_attestations \
         WHERE id = $1 AND status = 'approved'"
    )
    .bind(req.attestation_id)
    .fetch_optional(pool)
    .await
    .map_err(DomainError::Db)?
    .ok_or_else(|| DomainError::InvalidInput(
        "attestation not found or not approved".to_string(),
    ))?;

    // 4. Look up identity_credential for this user (most recent).
    let identity_credential_id: Option<i32> = sqlx::query_scalar(
        "SELECT id FROM identity_credentials \
         WHERE user_id = $1 \
         ORDER BY created_at DESC LIMIT 1"
    )
    .bind(uid)
    .fetch_optional(pool)
    .await
    .map_err(DomainError::Db)?;

    // 5. Generate UUID and derive display_code.
    let token_uuid  = Uuid::new_v4();
    let display_code = derive_display_code(&token_uuid, hmac_key, 1);
    let issued_at   = Utc::now();
    let expires_at  = issued_at + chrono::Duration::days(365);

    // 6. Sign the soultoken.
    let signature = sign_soultoken(
        &token_uuid,
        uid,
        &issued_at,
        &expires_at,
        &display_code,
        signing_key,
    );

    // 7. Create soultoken record with full verification chain.
    let token = repository::create_soultoken(
        pool,
        token_uuid,
        &display_code,
        1,
        uid,
        &req.token_type,
        None,
        identity_credential_id,
        Some(attest_presence_threshold_id),
        Some(req.attestation_id),
        Some(&signature),
        expires_at,
    ).await?;

    // 8. Update users.soultoken_id.
    repository::update_user_soultoken_id(pool, uid, Some(token.id)).await?;

    // 9. Write verification_event and audit_event.
    if let Err(e) = sqlx::query(
        "INSERT INTO verification_events \
         (user_id, event_type, reference_type, reference_id, actor_id, metadata) \
         VALUES ($1, 'soultoken_issued', 'soultoken', $2, $1, $3)"
    )
    .bind(uid)
    .bind(token.id)
    .bind(serde_json::json!({ "token_type": &req.token_type }))
    .execute(pool)
    .await
    {
        tracing::error!(error = %e, "verification_events (soultoken_issued) insert failed");
    }

    audit::write(
        pool,
        Some(uid),
        None,
        "soultoken.issued",
        serde_json::json!({
            "soultoken_id": token.id,
            "token_type":   &req.token_type,
        }),
    ).await;

    // 10. Publish domain event.
    event_bus.publish(DomainEvent::SoultokenIssued {
        soultoken_id: token.id,
        user_id:      uid,
        token_type:   req.token_type,
    });

    Ok(to_response(token))
}

/// Return the active soultoken for the authenticated user (BFIP Section 7.2).
///
/// Never includes uuid in the response.
pub async fn get_my_soultoken(
    pool:    &PgPool,
    user_id: UserId,
) -> AppResult<SoultokenResponse> {
    let uid = i32::from(user_id);
    let row = repository::get_active_soultoken_by_user(pool, uid)
        .await?
        .ok_or(DomainError::NotFound)?;
    Ok(to_response(row))
}

/// Revoke a soultoken (platform_admin or attestation_reviewer action) (BFIP Section 7.4).
///
/// Revocation resets the user to `registered` — they must restart the protocol.
pub async fn revoke_soultoken(
    pool:               &PgPool,
    soultoken_id:       i32,
    requesting_user_id: UserId,
    req:                RevokeSoultokenRequest,
) -> AppResult<SoultokenResponse> {
    let rid = i32::from(requesting_user_id);

    // 1. Requesting user must be platform_admin or attestation_reviewer.
    let requester = user_repo::find_by_id(pool, requesting_user_id)
        .await?
        .ok_or(DomainError::Unauthorized)?;

    let is_reviewer: bool = if requester.is_platform_admin {
        true
    } else {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM staff_roles \
             WHERE user_id = $1 \
               AND role = 'attestation_reviewer' \
               AND revoked_at IS NULL \
               AND (expires_at IS NULL OR expires_at > now())"
        )
        .bind(rid)
        .fetch_one(pool)
        .await
        .map_err(DomainError::Db)?;
        count > 0
    };

    if !is_reviewer {
        return Err(DomainError::Forbidden);
    }

    // 2. Load soultoken — must exist, not already revoked.
    let token = repository::get_soultoken_by_id(pool, soultoken_id)
        .await?
        .ok_or(DomainError::NotFound)?;

    if token.revoked_at.is_some() {
        return Err(DomainError::Conflict("soultoken is already revoked".to_string()));
    }

    // 3. Validate revocation_reason.
    let valid_reasons = ["stripe_flag", "staff_rescission", "platform_ban"];
    if !valid_reasons.contains(&req.revocation_reason.as_str()) {
        return Err(DomainError::invalid_input(
            "revocation_reason must be one of: stripe_flag, staff_rescission, platform_ban",
        ));
    }

    // 4. Revoke soultoken.
    let revoked = repository::revoke_soultoken(
        pool,
        soultoken_id,
        &req.revocation_reason,
        Some(rid),
        req.revocation_visit_id,
        None,
    ).await?;

    // 5. Reset user to registered.
    sqlx::query(
        "UPDATE users SET \
         verification_status = 'registered', \
         soultoken_id        = NULL, \
         updated_at          = now() \
         WHERE id = $1"
    )
    .bind(token.holder_user_id)
    .execute(pool)
    .await
    .map_err(DomainError::Db)?;

    // 6. Write verification_event and audit_event.
    if let Err(e) = sqlx::query(
        "INSERT INTO verification_events \
         (user_id, event_type, reference_type, reference_id, actor_id, metadata) \
         VALUES ($1, 'soultoken_revoked', 'soultoken', $2, $3, $4)"
    )
    .bind(token.holder_user_id)
    .bind(soultoken_id)
    .bind(rid)
    .bind(serde_json::json!({ "reason": &req.revocation_reason }))
    .execute(pool)
    .await
    {
        tracing::error!(error = %e, "verification_events (soultoken_revoked) insert failed");
    }

    audit::write(
        pool,
        Some(rid),
        None,
        "soultoken.revoked",
        serde_json::json!({
            "soultoken_id": soultoken_id,
            "user_id":      token.holder_user_id,
            "reason":       &req.revocation_reason,
        }),
    ).await;

    Ok(to_response(revoked))
}

/// Voluntary surrender by the soultoken holder (BFIP Section 7.5).
///
/// Requires in-person visit and witnessed by delivery staff.
pub async fn surrender_soultoken(
    pool:               &PgPool,
    soultoken_id:       i32,
    requesting_user_id: UserId,
    req:                SurrenderSoultokenRequest,
    event_bus:          &EventBus,
) -> AppResult<SoultokenResponse> {
    let uid = i32::from(requesting_user_id);

    // 1. Load soultoken — must not be already revoked.
    let token = repository::get_soultoken_by_id(pool, soultoken_id)
        .await?
        .ok_or(DomainError::NotFound)?;

    // 2. Requesting user must be the holder.
    if token.holder_user_id != uid {
        return Err(DomainError::Forbidden);
    }

    if token.revoked_at.is_some() {
        return Err(DomainError::Conflict("soultoken is already revoked".to_string()));
    }

    // 3. revocation_visit_id must reference a real staff_visit.
    let visit_exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM staff_visits WHERE id = $1"
    )
    .bind(req.revocation_visit_id)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)?;

    if visit_exists == 0 {
        return Err(DomainError::InvalidInput(
            "revocation_visit_id does not reference a valid staff visit".to_string(),
        ));
    }

    // 4. surrender_witnessed_by must be an active delivery_staff member.
    let witness_is_staff: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM staff_roles \
         WHERE user_id = $1 \
           AND role = 'delivery_staff' \
           AND revoked_at IS NULL \
           AND (expires_at IS NULL OR expires_at > now())"
    )
    .bind(req.surrender_witnessed_by)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)?;

    if witness_is_staff == 0 {
        return Err(DomainError::InvalidInput(
            "surrender_witnessed_by must be an active delivery_staff member".to_string(),
        ));
    }

    // 5. Revoke with reason = 'voluntary_surrender'.
    let revoked = repository::revoke_soultoken(
        pool,
        soultoken_id,
        "voluntary_surrender",
        None,
        Some(req.revocation_visit_id),
        Some(req.surrender_witnessed_by),
    ).await?;

    // 6. Reset user to registered.
    sqlx::query(
        "UPDATE users SET \
         verification_status = 'registered', \
         soultoken_id        = NULL, \
         updated_at          = now() \
         WHERE id = $1"
    )
    .bind(uid)
    .execute(pool)
    .await
    .map_err(DomainError::Db)?;

    // 7. Write verification_events (surrender_requested + surrender_completed).
    for event_type in &["soultoken_surrender_requested", "soultoken_surrender_completed"] {
        if let Err(e) = sqlx::query(
            "INSERT INTO verification_events \
             (user_id, event_type, reference_type, reference_id, actor_id, metadata) \
             VALUES ($1, $2, 'soultoken', $3, $1, $4)"
        )
        .bind(uid)
        .bind(*event_type)
        .bind(soultoken_id)
        .bind(serde_json::json!({
            "revocation_visit_id":    req.revocation_visit_id,
            "surrender_witnessed_by": req.surrender_witnessed_by,
        }))
        .execute(pool)
        .await
        {
            tracing::error!(error = %e, "verification_events ({event_type}) insert failed");
        }
    }

    audit::write(
        pool,
        Some(uid),
        None,
        "soultoken.surrendered",
        serde_json::json!({
            "soultoken_id":           soultoken_id,
            "revocation_visit_id":    req.revocation_visit_id,
            "surrender_witnessed_by": req.surrender_witnessed_by,
        }),
    ).await;

    event_bus.publish(DomainEvent::SoultokenRevoked {
        soultoken_id,
        user_id: uid,
        reason:  "voluntary_surrender".to_string(),
    });

    Ok(to_response(revoked))
}

/// Renew an active soultoken for 12 more months (BFIP Section 7.6).
///
/// A qualifying presence event extends the validity window.
pub async fn renew_soultoken(
    pool:              &PgPool,
    user_id:           UserId,
    req:               RenewSoultokenRequest,
    event_bus:         &EventBus,
) -> AppResult<SoultokenRenewalResponse> {
    let uid = i32::from(user_id);

    // 1. Get active soultoken — must exist and not be revoked or expired.
    let token = repository::get_active_soultoken_by_user(pool, uid)
        .await?
        .ok_or(DomainError::NotFound)?;

    // 2. Cannot renew if revoked (belt-and-suspenders — DB trigger also enforces this).
    if token.revoked_at.is_some() {
        return Err(DomainError::Conflict(
            "cannot renew a revoked soultoken".to_string(),
        ));
    }

    // 3. Compute new expiry.
    let previous_expires_at = token.expires_at;
    let new_expires_at      = Utc::now() + chrono::Duration::days(365);

    // 4. Update soultoken expires_at.
    repository::renew_soultoken(pool, token.id, new_expires_at).await?;

    // 5. Create renewal record.
    let renewal = repository::create_renewal(
        pool,
        token.id,
        uid,
        req.presence_event_id,
        &req.renewal_type,
        previous_expires_at,
        new_expires_at,
    ).await?;

    // 6. Write verification_event and audit_event.
    if let Err(e) = sqlx::query(
        "INSERT INTO verification_events \
         (user_id, event_type, reference_type, reference_id, actor_id, metadata) \
         VALUES ($1, 'soultoken_renewed', 'soultoken', $2, $1, $3)"
    )
    .bind(uid)
    .bind(token.id)
    .bind(serde_json::json!({
        "renewal_type":       &renewal.renewal_type,
        "new_expires_at":     new_expires_at.to_rfc3339(),
        "presence_event_id":  req.presence_event_id.unwrap_or(0),
    }))
    .execute(pool)
    .await
    {
        tracing::error!(error = %e, "verification_events (soultoken_renewed) insert failed");
    }

    audit::write(
        pool,
        Some(uid),
        None,
        "soultoken.renewed",
        serde_json::json!({
            "soultoken_id":   token.id,
            "new_expires_at": new_expires_at.to_rfc3339(),
        }),
    ).await;

    event_bus.publish(DomainEvent::SoultokenRenewed {
        soultoken_id: token.id,
        user_id:      uid,
    });

    Ok(to_renewal_response(renewal))
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

    const TEST_HMAC_KEY:    &[u8] = b"test-soultoken-hmac-key-32bytes!!";
    const TEST_SIGNING_KEY: &[u8] = b"test-soultoken-sign-key-32bytes!!";

    // ── Test fixture ──────────────────────────────────────────────────────────

    /// Seed: attested user with approved attestation + identity_credential.
    /// Returns (user_id, attestation_id).
    async fn setup_attested_user(pool: &PgPool) -> (UserId, i32) {
        use fake::{Fake, faker::internet::en::SafeEmail};

        // Users
        let (uid,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified, verification_status) \
             VALUES ($1, true, 'presence_confirmed') RETURNING id",
        )
        .bind(&SafeEmail().fake::<String>())
        .fetch_one(pool).await.unwrap();

        let (staff_id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id",
        )
        .bind(&SafeEmail().fake::<String>())
        .fetch_one(pool).await.unwrap();

        let (r1_id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id",
        )
        .bind(&SafeEmail().fake::<String>())
        .fetch_one(pool).await.unwrap();

        let (r2_id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id",
        )
        .bind(&SafeEmail().fake::<String>())
        .fetch_one(pool).await.unwrap();

        // Identity credential (required for user soultoken schema constraint)
        sqlx::query(
            "INSERT INTO identity_credentials \
             (user_id, credential_type, verified_at, cooling_ends_at, cooling_completed_at) \
             VALUES ($1, 'stripe_identity', now(), now() + interval '7 days', now())",
        )
        .bind(uid).execute(pool).await.unwrap();

        // Location + business
        let (loc_id,): (i32,) = sqlx::query_as(
            "INSERT INTO locations (name, location_type, address, timezone) \
             VALUES ('ST Store', 'box_fraise_store', '1 ST St', 'America/Edmonton') \
             RETURNING id",
        )
        .fetch_one(pool).await.unwrap();

        let (biz_id,): (i32,) = sqlx::query_as(
            "INSERT INTO businesses \
             (location_id, primary_holder_id, name, verification_status) \
             VALUES ($1, $2, 'ST Biz', 'active') RETURNING id",
        )
        .bind(loc_id).bind(uid).fetch_one(pool).await.unwrap();

        // Presence threshold
        let (thresh_id,): (i32,) = sqlx::query_as(
            "INSERT INTO presence_thresholds \
             (user_id, business_id, event_count, days_count, threshold_met_at) \
             VALUES ($1, $2, 3, 3, now()) RETURNING id",
        )
        .bind(uid).bind(biz_id).fetch_one(pool).await.unwrap();

        // Staff visit (completed)
        let (visit_id,): (i32,) = sqlx::query_as(
            "INSERT INTO staff_visits \
             (location_id, staff_id, visit_type, status, scheduled_at) \
             VALUES ($1, $2, 'combined', 'completed', now()) RETURNING id",
        )
        .bind(loc_id).bind(staff_id).fetch_one(pool).await.unwrap();

        // Attestation (user still presence_confirmed — trigger allows insert)
        let (attest_id,): (i32,) = sqlx::query_as(
            "INSERT INTO visit_attestations \
             (visit_id, user_id, staff_id, presence_threshold_id, \
              assigned_reviewer_1_id, assigned_reviewer_2_id, status) \
             VALUES ($1, $2, $3, $4, $5, $6, 'approved') RETURNING id",
        )
        .bind(visit_id).bind(uid).bind(staff_id)
        .bind(thresh_id).bind(r1_id).bind(r2_id)
        .fetch_one(pool).await.unwrap();

        // Promote user to attested
        sqlx::query(
            "UPDATE users SET verification_status = 'attested', \
             attested_at = now() WHERE id = $1",
        )
        .bind(uid).execute(pool).await.unwrap();

        (UserId::from(uid), attest_id)
    }

    // ── Tests 1–3: issue_soultoken ────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn issue_soultoken_succeeds_for_attested_user(pool: PgPool) {
        let (user_id, attest_id) = setup_attested_user(&pool).await;
        let bus = EventBus::new();

        let resp = issue_soultoken(
            &pool, user_id,
            IssueSoultokenRequest { attestation_id: attest_id, token_type: "user".to_owned() },
            TEST_HMAC_KEY, TEST_SIGNING_KEY, &bus,
        ).await.expect("issue_soultoken must succeed");

        // Display code format: XXXX-XXXX-XXXX
        let re = regex::Regex::new(r"^[A-Z0-9]{4}-[A-Z0-9]{4}-[A-Z0-9]{4}$").unwrap();
        assert!(re.is_match(&resp.display_code),
            "display_code must match XXXX-XXXX-XXXX, got: {}", resp.display_code);

        // UUID must NOT appear in SoultokenResponse fields
        let json = serde_json::to_string(&resp).unwrap();
        let uuid_pattern = regex::Regex::new(
            r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}"
        ).unwrap();
        assert!(!uuid_pattern.is_match(&json),
            "uuid must NOT appear in SoultokenResponse JSON: {json}");

        // users.soultoken_id must be set
        let st_id: Option<i32> = sqlx::query_scalar(
            "SELECT soultoken_id FROM users WHERE id = $1"
        )
        .bind(i32::from(user_id)).fetch_one(&pool).await.unwrap();
        assert_eq!(st_id, Some(resp.id), "users.soultoken_id must be set");

        // verification_event written
        let ve_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM verification_events \
             WHERE event_type = 'soultoken_issued' AND user_id = $1"
        )
        .bind(i32::from(user_id)).fetch_one(&pool).await.unwrap();
        assert_eq!(ve_count, 1, "verification_event 'soultoken_issued' must be written");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn issue_soultoken_fails_if_not_attested(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let bus = EventBus::new();

        let (uid,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified, verification_status) \
             VALUES ($1, true, 'presence_confirmed') RETURNING id",
        )
        .bind(&SafeEmail().fake::<String>())
        .fetch_one(&pool).await.unwrap();

        let err = issue_soultoken(
            &pool, UserId::from(uid),
            IssueSoultokenRequest { attestation_id: 999, token_type: "user".to_owned() },
            TEST_HMAC_KEY, TEST_SIGNING_KEY, &bus,
        ).await.unwrap_err();

        assert!(matches!(err, DomainError::InvalidInput(_)),
            "non-attested user must get InvalidInput, got: {err:?}");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn issue_soultoken_fails_if_already_has_active_soultoken(pool: PgPool) {
        let (user_id, attest_id) = setup_attested_user(&pool).await;
        let bus = EventBus::new();

        issue_soultoken(
            &pool, user_id,
            IssueSoultokenRequest { attestation_id: attest_id, token_type: "user".to_owned() },
            TEST_HMAC_KEY, TEST_SIGNING_KEY, &bus,
        ).await.unwrap();

        let err = issue_soultoken(
            &pool, user_id,
            IssueSoultokenRequest { attestation_id: attest_id, token_type: "user".to_owned() },
            TEST_HMAC_KEY, TEST_SIGNING_KEY, &bus,
        ).await.unwrap_err();

        assert!(matches!(err, DomainError::Conflict(_)),
            "duplicate soultoken must be Conflict, got: {err:?}");
    }

    // ── Tests 4–6: derive_display_code ───────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn derive_display_code_produces_correct_format(_pool: PgPool) {
        let uuid = Uuid::new_v4();
        let code = derive_display_code(&uuid, TEST_HMAC_KEY, 1);
        let re = regex::Regex::new(r"^[A-Z0-9]{4}-[A-Z0-9]{4}-[A-Z0-9]{4}$").unwrap();
        assert!(re.is_match(&code), "display_code must match XXXX-XXXX-XXXX, got: {code}");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn derive_display_code_is_deterministic(_pool: PgPool) {
        let uuid = Uuid::new_v4();
        let c1 = derive_display_code(&uuid, TEST_HMAC_KEY, 1);
        let c2 = derive_display_code(&uuid, TEST_HMAC_KEY, 1);
        assert_eq!(c1, c2, "display_code must be deterministic for same UUID+key");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn derive_display_code_differs_for_different_uuids(_pool: PgPool) {
        let u1 = Uuid::new_v4();
        let u2 = Uuid::new_v4();
        let c1 = derive_display_code(&u1, TEST_HMAC_KEY, 1);
        let c2 = derive_display_code(&u2, TEST_HMAC_KEY, 1);
        assert_ne!(c1, c2, "different UUIDs must produce different display codes");
    }

    // ── Test 7: sign_soultoken ────────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn sign_soultoken_produces_non_empty_signature(_pool: PgPool) {
        let uuid   = Uuid::new_v4();
        let now    = Utc::now();
        let exp    = now + chrono::Duration::days(365);
        let code   = derive_display_code(&uuid, TEST_HMAC_KEY, 1);
        let sig    = sign_soultoken(&uuid, 42, &now, &exp, &code, TEST_SIGNING_KEY);
        assert!(!sig.is_empty(), "signature must be non-empty");
        assert!(sig.chars().all(|c| c.is_ascii_hexdigit()),
            "signature must be lowercase hex, got: {sig}");
    }

    // ── Test 8: get_my_soultoken — no uuid in response ────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn get_my_soultoken_returns_display_code_not_uuid(pool: PgPool) {
        let (user_id, attest_id) = setup_attested_user(&pool).await;
        let bus = EventBus::new();

        issue_soultoken(
            &pool, user_id,
            IssueSoultokenRequest { attestation_id: attest_id, token_type: "user".to_owned() },
            TEST_HMAC_KEY, TEST_SIGNING_KEY, &bus,
        ).await.unwrap();

        let resp = get_my_soultoken(&pool, user_id).await.unwrap();

        let json = serde_json::to_string(&resp).unwrap();
        let uuid_pattern = regex::Regex::new(
            r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}"
        ).unwrap();
        assert!(!uuid_pattern.is_match(&json),
            "uuid must NOT appear in JSON response: {json}");

        let re = regex::Regex::new(r"^[A-Z0-9]{4}-[A-Z0-9]{4}-[A-Z0-9]{4}$").unwrap();
        assert!(re.is_match(&resp.display_code),
            "display_code must be present, got: {}", resp.display_code);
    }

    // ── Tests 9–10: revoke_soultoken ─────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn revoke_soultoken_sets_user_back_to_registered(pool: PgPool) {
        let (user_id, attest_id) = setup_attested_user(&pool).await;
        let bus = EventBus::new();

        // Need a platform_admin to revoke
        let (admin_id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified, is_platform_admin) \
             VALUES ('admin@revoketest.test', true, true) RETURNING id",
        )
        .fetch_one(&pool).await.unwrap();
        let admin = UserId::from(admin_id);

        let token = issue_soultoken(
            &pool, user_id,
            IssueSoultokenRequest { attestation_id: attest_id, token_type: "user".to_owned() },
            TEST_HMAC_KEY, TEST_SIGNING_KEY, &bus,
        ).await.unwrap();

        revoke_soultoken(
            &pool, token.id, admin,
            RevokeSoultokenRequest {
                revocation_reason:   "stripe_flag".to_owned(),
                revocation_visit_id: None,
            },
        ).await.expect("revoke must succeed");

        let status: String = sqlx::query_scalar(
            "SELECT verification_status FROM users WHERE id = $1"
        )
        .bind(i32::from(user_id)).fetch_one(&pool).await.unwrap();
        assert_eq!(status, "registered");

        let st_id: Option<i32> = sqlx::query_scalar(
            "SELECT soultoken_id FROM users WHERE id = $1"
        )
        .bind(i32::from(user_id)).fetch_one(&pool).await.unwrap();
        assert!(st_id.is_none(), "users.soultoken_id must be NULL after revocation");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn surrender_soultoken_requires_visit_record(pool: PgPool) {
        let (user_id, attest_id) = setup_attested_user(&pool).await;
        let bus = EventBus::new();

        let token = issue_soultoken(
            &pool, user_id,
            IssueSoultokenRequest { attestation_id: attest_id, token_type: "user".to_owned() },
            TEST_HMAC_KEY, TEST_SIGNING_KEY, &bus,
        ).await.unwrap();

        // Attempt surrender with non-existent visit
        let err = surrender_soultoken(
            &pool, token.id, user_id,
            SurrenderSoultokenRequest {
                revocation_visit_id:    999_999,
                surrender_witnessed_by: i32::from(user_id),
            },
            &bus,
        ).await.unwrap_err();

        assert!(matches!(err, DomainError::InvalidInput(_)),
            "surrender with invalid visit must be InvalidInput, got: {err:?}");
    }

    // ── Tests 11–12: renew_soultoken ─────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn renew_soultoken_extends_expires_at_by_12_months(pool: PgPool) {
        let (user_id, attest_id) = setup_attested_user(&pool).await;
        let bus = EventBus::new();

        let token = issue_soultoken(
            &pool, user_id,
            IssueSoultokenRequest { attestation_id: attest_id, token_type: "user".to_owned() },
            TEST_HMAC_KEY, TEST_SIGNING_KEY, &bus,
        ).await.unwrap();

        let before_renewal = token.expires_at;

        let renewal = renew_soultoken(
            &pool, user_id,
            RenewSoultokenRequest { presence_event_id: None, renewal_type: "beacon_dwell".to_owned() },
            &bus,
        ).await.expect("renew must succeed");

        assert!(renewal.new_expires_at > before_renewal,
            "new_expires_at must be after previous expires_at");
        let diff_days = (renewal.new_expires_at - Utc::now()).num_days();
        assert!(diff_days >= 364, "new expiry must be ~12 months from now, got {diff_days} days");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn renew_soultoken_fails_for_revoked_soultoken(pool: PgPool) {
        let (user_id, attest_id) = setup_attested_user(&pool).await;
        let bus = EventBus::new();

        let (admin_id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified, is_platform_admin) \
             VALUES ('admin@renewtest.test', true, true) RETURNING id",
        )
        .fetch_one(&pool).await.unwrap();
        let admin = UserId::from(admin_id);

        let token = issue_soultoken(
            &pool, user_id,
            IssueSoultokenRequest { attestation_id: attest_id, token_type: "user".to_owned() },
            TEST_HMAC_KEY, TEST_SIGNING_KEY, &bus,
        ).await.unwrap();

        revoke_soultoken(
            &pool, token.id, admin,
            RevokeSoultokenRequest {
                revocation_reason:   "platform_ban".to_owned(),
                revocation_visit_id: None,
            },
        ).await.unwrap();

        let err = renew_soultoken(
            &pool, user_id,
            RenewSoultokenRequest { presence_event_id: None, renewal_type: "beacon_dwell".to_owned() },
            &bus,
        ).await.unwrap_err();

        assert!(matches!(err, DomainError::NotFound),
            "renewing a revoked soultoken must be NotFound, got: {err:?}");
    }

    // ── Adversarial tests ─────────────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_revoke_another_users_soultoken(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let (user_id, attest_id) = setup_attested_user(&pool).await;
        let bus = EventBus::new();

        let token = issue_soultoken(
            &pool, user_id,
            IssueSoultokenRequest { attestation_id: attest_id, token_type: "user".to_owned() },
            TEST_HMAC_KEY, TEST_SIGNING_KEY, &bus,
        ).await.unwrap();

        // Attacker is a regular user (no admin or reviewer role)
        let (attacker_id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id",
        )
        .bind(&SafeEmail().fake::<String>())
        .fetch_one(&pool).await.unwrap();

        let err = revoke_soultoken(
            &pool, token.id, UserId::from(attacker_id),
            RevokeSoultokenRequest {
                revocation_reason: "stripe_flag".to_owned(),
                revocation_visit_id: None,
            },
        ).await.unwrap_err();

        assert!(matches!(err, DomainError::Forbidden));
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_surrender_another_users_soultoken(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let (user_id, attest_id) = setup_attested_user(&pool).await;
        let bus = EventBus::new();

        let token = issue_soultoken(
            &pool, user_id,
            IssueSoultokenRequest { attestation_id: attest_id, token_type: "user".to_owned() },
            TEST_HMAC_KEY, TEST_SIGNING_KEY, &bus,
        ).await.unwrap();

        let (attacker_id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id",
        )
        .bind(&SafeEmail().fake::<String>())
        .fetch_one(&pool).await.unwrap();

        let err = surrender_soultoken(
            &pool, token.id, UserId::from(attacker_id),
            SurrenderSoultokenRequest {
                revocation_visit_id:    1,
                surrender_witnessed_by: attacker_id,
            },
            &bus,
        ).await.unwrap_err();

        assert!(matches!(err, DomainError::Forbidden));
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_renew_expired_soultoken(pool: PgPool) {
        let (user_id, attest_id) = setup_attested_user(&pool).await;
        let bus = EventBus::new();

        issue_soultoken(
            &pool, user_id,
            IssueSoultokenRequest { attestation_id: attest_id, token_type: "user".to_owned() },
            TEST_HMAC_KEY, TEST_SIGNING_KEY, &bus,
        ).await.unwrap();

        // Backdate expires_at to the past
        sqlx::query(
            "UPDATE soultokens SET expires_at = now() - interval '1 day' \
             WHERE holder_user_id = $1"
        )
        .bind(i32::from(user_id)).execute(&pool).await.unwrap();

        let err = renew_soultoken(
            &pool, user_id,
            RenewSoultokenRequest { presence_event_id: None, renewal_type: "beacon_dwell".to_owned() },
            &bus,
        ).await.unwrap_err();

        // get_active_soultoken_by_user filters WHERE expires_at > now(), so expired → NotFound
        assert!(matches!(err, DomainError::NotFound),
            "renewing an expired soultoken must be NotFound, got: {err:?}");
    }
}
