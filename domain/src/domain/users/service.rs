use chrono::Utc;
use sqlx::PgPool;

use crate::{
    audit,
    error::{DomainError, AppResult},
    types::UserId,
};
use super::{
    repository,
    types::{
        ConsentExport, ErasureResponse, OrderExport, PresenceEventSummary,
        PublicProfile, SoultokenSummary, UserDataExport, UserProfileExport,
        UserSearchResult, VerificationEventExport,
    },
};

/// Search for users matching `query` (matched against display name and email).
pub async fn search_users(pool: &PgPool, query: &str) -> AppResult<Vec<UserSearchResult>> {
    repository::search(pool, query).await
}

/// Return the public profile for `user_id`. Returns `NotFound` when the user
/// does not exist or has been banned.
pub async fn get_public_profile(pool: &PgPool, user_id: UserId) -> AppResult<PublicProfile> {
    repository::public_profile(pool, user_id)
        .await?
        .ok_or(DomainError::NotFound)
}

// ── Compliance — Hardening §9 ────────────────────────────────────────────────

/// `DELETE /api/users/me` — GDPR Article 17 right to erasure.
///
/// Anonymises the user record in place. The row itself is never hard-deleted
/// because it anchors immutable audit / verification / soultoken records;
/// dropping it would orphan everything those tables FK to. Personal
/// identifiers are nulled or replaced with a placeholder; everything else
/// is retained per the policy returned in `retained_data`.
///
/// Returns `Conflict` when the user holds an active soultoken — soultoken
/// surrender requires an in-person visit (BFIP §7.5) and can't be done
/// through this endpoint.
pub async fn request_erasure(pool: &PgPool, user_id: i32) -> AppResult<ErasureResponse> {
    // 1. User must exist and not already be erased.
    let exists: Option<bool> = sqlx::query_scalar(
        "SELECT TRUE FROM users WHERE id = $1 AND deleted_at IS NULL"
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(DomainError::Db)?;
    if exists.is_none() {
        return Err(DomainError::NotFound);
    }

    // 2. Reject when an active soultoken is still attached. The user must
    //    surrender it in person first (BFIP §7.5).
    let active_soultoken: Option<i32> = sqlx::query_scalar(
        "SELECT s.id FROM soultokens s \
         JOIN users u ON u.soultoken_id = s.id \
         WHERE u.id = $1 \
           AND s.revoked_at IS NULL \
           AND s.expires_at > now()"
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(DomainError::Db)?;
    if active_soultoken.is_some() {
        return Err(DomainError::Conflict(
            "Soultoken must be surrendered before erasure. \
             Please attend a Box Fraise location to surrender your \
             soultoken in person.".to_string(),
        ));
    }

    // 3. Anonymise in place. The placeholder email satisfies the UNIQUE
    //    constraint while clearly marking the row as erased.
    sqlx::query(
        "UPDATE users SET \
            email         = 'erased-' || id || '@deleted.boxfraise.com', \
            display_name  = 'Deleted User', \
            apple_id      = NULL, \
            push_token    = NULL, \
            soultoken_id  = NULL, \
            deleted_at    = now(), \
            updated_at    = now() \
         WHERE id = $1"
    )
    .bind(user_id)
    .execute(pool)
    .await
    .map_err(DomainError::Db)?;

    let retained = vec![
        "audit_events: retained 7 years (legal requirement)".to_string(),
        "verification_events: retained per legal obligation".to_string(),
        "attestation records: retained 7 years".to_string(),
        "soultoken records: retained 7 years after revocation".to_string(),
        "background check results: retained 12 months".to_string(),
    ];

    // 4. Audit. Includes the retained-data list so the audit trail records
    //    exactly what was kept, not just that erasure happened.
    audit::write(
        pool,
        Some(user_id),
        None,
        "user.erasure_requested",
        serde_json::json!({ "user_id": user_id, "retained_data": retained }),
    ).await;

    Ok(ErasureResponse {
        user_id,
        erasure_scheduled_at: Utc::now(),
        retained_data:        retained,
        erasure_note:         "Personal identifiers have been removed. \
                               Cryptographic records are retained as required \
                               by law.".to_string(),
    })
}

/// `GET /api/users/me/export` — GDPR Article 20 data portability.
pub async fn export_my_data(pool: &PgPool, user_id: i32) -> AppResult<UserDataExport> {
    // 1. Profile — single-row query.
    let user_profile: UserProfileExport = sqlx::query_as(
        "SELECT id, email, display_name, verification_status, created_at \
         FROM users WHERE id = $1 AND deleted_at IS NULL AND NOT is_banned"
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(DomainError::Db)?
    .ok_or(DomainError::NotFound)?;

    // 2. Verification journey.
    let verification_journey: Vec<VerificationEventExport> = sqlx::query_as(
        "SELECT id, event_type, created_at \
         FROM verification_events WHERE user_id = $1 \
         ORDER BY created_at ASC"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(DomainError::Db)?;

    // 3. Presence summaries — strip GPS / RSSI; export only the audit-relevant fields.
    let presence_history: Vec<PresenceEventSummary> = sqlx::query_as(
        "SELECT id, business_id, is_qualifying, occurred_at \
         FROM presence_events WHERE user_id = $1 \
         ORDER BY occurred_at ASC"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(DomainError::Db)?;

    // 4. Orders.
    let order_history: Vec<OrderExport> = sqlx::query_as(
        "SELECT id, business_id, box_count, amount_cents, status, created_at \
         FROM orders WHERE user_id = $1 \
         ORDER BY created_at ASC"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(DomainError::Db)?;

    // 5. Consent log.
    let consent_history: Vec<ConsentExport> = sqlx::query_as(
        "SELECT consent_type, consent_version, granted, granted_at, revoked_at \
         FROM consent_records WHERE user_id = $1 \
         ORDER BY granted_at ASC"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(DomainError::Db)?;

    // 6. Soultoken history (no UUID — deliberately excluded by the
    //    soultokens types layer; see SOULTOKEN_COLS.)
    let soultoken_history: Vec<SoultokenSummary> = sqlx::query_as(
        "SELECT id, display_code, token_type, issued_at, expires_at, revoked_at \
         FROM soultokens WHERE holder_user_id = $1 \
         ORDER BY issued_at ASC"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(DomainError::Db)?;

    audit::write(
        pool,
        Some(user_id),
        None,
        "user.data_export_requested",
        serde_json::Value::Null,
    ).await;

    Ok(UserDataExport {
        exported_at: Utc::now(),
        user_profile,
        verification_journey,
        presence_history,
        order_history,
        consent_history,
        soultoken_history,
    })
}

/// Insert a consent record. Called from auth flows on user creation and
/// from any service that triggers a new processing activity (e.g.
/// background check initiation — TODO).
pub async fn record_consent(
    pool:         &PgPool,
    user_id:      i32,
    consent_type: &str,
    granted:      bool,
    ip_address:   Option<&str>,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO consent_records \
            (user_id, consent_type, granted, ip_address) \
         VALUES ($1, $2, $3, $4)"
    )
    .bind(user_id)
    .bind(consent_type)
    .bind(granted)
    .bind(ip_address)
    .execute(pool)
    .await
    .map_err(DomainError::Db)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    async fn insert_user(pool: &PgPool, email: &str) -> UserId {
        let (id,): (i32,) =
            sqlx::query_as("INSERT INTO users (email) VALUES ($1) RETURNING id")
                .bind(email)
                .fetch_one(pool)
                .await
                .unwrap();
        UserId::from(id)
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn get_public_profile_returns_profile_for_known_user(pool: PgPool) {
        let user_id = insert_user(&pool, "alice@test.com").await;
        sqlx::query("UPDATE users SET display_name = 'Alice' WHERE id = $1")
            .bind(i32::from(user_id))
            .execute(&pool)
            .await
            .unwrap();

        let profile = get_public_profile(&pool, user_id).await.unwrap();
        assert_eq!(profile.id, user_id);
        assert_eq!(profile.display_name.as_deref(), Some("Alice"));
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn get_public_profile_returns_not_found_for_unknown_user(pool: PgPool) {
        let result = get_public_profile(&pool, UserId::from(99999)).await;
        assert!(matches!(result, Err(DomainError::NotFound)));
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn get_public_profile_returns_not_found_for_banned_user(pool: PgPool) {
        let user_id = insert_user(&pool, "banned@test.com").await;
        sqlx::query("UPDATE users SET is_banned = true WHERE id = $1")
            .bind(i32::from(user_id))
            .execute(&pool)
            .await
            .unwrap();
        let result = get_public_profile(&pool, user_id).await;
        assert!(matches!(result, Err(DomainError::NotFound)));
    }
}
