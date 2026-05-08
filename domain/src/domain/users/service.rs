use chrono::Utc;
use sqlx::PgPool;

use crate::{
    audit,
    error::{DomainError, AppResult},
    transaction::{AdminRlsTransaction, RlsTransaction},
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
///
/// Read-only, no audit — kept on `&PgPool` per cleanup #3 rule. Acquires a
/// pool connection internally to satisfy the repository's `&mut PgConnection`
/// signature.
pub async fn search_users(pool: &PgPool, query: &str) -> AppResult<Vec<UserSearchResult>> {
    let mut conn = pool.acquire().await.map_err(DomainError::Db)?;
    repository::search(&mut conn, query).await
}

/// Return the public profile for `user_id`. Returns `NotFound` when the user
/// does not exist or has been banned.
///
/// Read-only, no audit — kept on `&PgPool` per cleanup #3 rule.
pub async fn get_public_profile(pool: &PgPool, user_id: UserId) -> AppResult<PublicProfile> {
    let mut conn = pool.acquire().await.map_err(DomainError::Db)?;
    repository::public_profile(&mut conn, user_id)
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
pub async fn request_erasure(
    tx:      &mut RlsTransaction,
    pool:    &PgPool, // pool: for audit writes only — audit is outside the transaction so it lands even on rollback
    user_id: i32,
) -> AppResult<ErasureResponse> {
    // 1. User must exist and not already be erased.
    let exists: Option<bool> = sqlx::query_scalar(
        "SELECT TRUE FROM users WHERE id = $1 AND deleted_at IS NULL"
    )
    .bind(user_id)
    .fetch_optional(tx.as_mut())
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
    .fetch_optional(tx.as_mut())
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
    .execute(tx.as_mut())
    .await
    .map_err(DomainError::Db)?;

    let retained = vec![
        "audit_events: retained 7 years (legal requirement)".to_string(),
        "verification_events: retained per legal obligation".to_string(),
        "attestation records: retained 7 years".to_string(),
        "soultoken records: retained 7 years after revocation".to_string(),
        "background check results: retained 12 months".to_string(),
    ];

    // 4. Audit — INLINED inside the tx (exception to the "audit on pool"
    //    rule). The standard `audit::write(pool, ...)` would acquire a
    //    separate pool connection and INSERT into `audit_events`, whose
    //    FK on `user_id` requires a lock on the same `users` row that
    //    this transaction's UPDATE in step 3 still holds. The audit
    //    INSERT therefore blocks indefinitely waiting for the tx to
    //    commit, but the tx cannot commit until this audit await
    //    returns — a circular wait that hangs the request. Inlining the
    //    audit on `tx.as_mut()` eliminates the cross-connection
    //    contention; the trade-off is that the audit row rolls back if
    //    the surrounding tx fails. For erasure that's acceptable
    //    because the only failure path is "user vanishes" and an audit
    //    row about a non-erased user would be misleading.
    let _ = sqlx::query(
        "INSERT INTO audit_events (event_kind, user_id, actor_id, metadata) \
         VALUES ($1, $2, $3, $4)"
    )
    .bind("user.erasure_requested")
    .bind(Some(user_id))
    .bind(None::<i32>)
    .bind(serde_json::json!({ "user_id": user_id, "retained_data": retained }))
    .execute(tx.as_mut())
    .await;
    // `pool` reference retained for compile compatibility — audit::write
    // is no longer called here.
    let _ = pool;

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
///
/// Pilot for Hardening cleanup #3 — runs every read inside the
/// `RlsTransaction`'s connection so the per-tx `app.user_id` setting
/// scopes row visibility. The handler is responsible for `begin` and
/// `commit`; this function only reads.
///
/// `audit::write` is still called against the pool (commits independently)
/// — the rollout has to decide whether to move it inside the transaction
/// for atomicity. See the `tx.as_mut()` calls vs the `pool: &PgPool`
/// audit call below for the contrast.
pub async fn export_my_data(
    tx:      &mut RlsTransaction,
    pool:    &PgPool,
    user_id: i32,
) -> AppResult<UserDataExport> {
    // 1. Profile — single-row query.
    let user_profile: UserProfileExport = sqlx::query_as(
        "SELECT id, email, display_name, verification_status, created_at \
         FROM users WHERE id = $1 AND deleted_at IS NULL AND NOT is_banned"
    )
    .bind(user_id)
    .fetch_optional(tx.as_mut())
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
    .fetch_all(tx.as_mut())
    .await
    .map_err(DomainError::Db)?;

    // 3. Presence summaries — strip GPS / RSSI; export only the audit-relevant fields.
    let presence_history: Vec<PresenceEventSummary> = sqlx::query_as(
        "SELECT id, business_id, is_qualifying, occurred_at \
         FROM presence_events WHERE user_id = $1 \
         ORDER BY occurred_at ASC"
    )
    .bind(user_id)
    .fetch_all(tx.as_mut())
    .await
    .map_err(DomainError::Db)?;

    // 4. Orders.
    let order_history: Vec<OrderExport> = sqlx::query_as(
        "SELECT id, business_id, box_count, amount_cents, status, created_at \
         FROM orders WHERE user_id = $1 \
         ORDER BY created_at ASC"
    )
    .bind(user_id)
    .fetch_all(tx.as_mut())
    .await
    .map_err(DomainError::Db)?;

    // 5. Consent log.
    let consent_history: Vec<ConsentExport> = sqlx::query_as(
        "SELECT consent_type, consent_version, granted, granted_at, revoked_at \
         FROM consent_records WHERE user_id = $1 \
         ORDER BY granted_at ASC"
    )
    .bind(user_id)
    .fetch_all(tx.as_mut())
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
    .fetch_all(tx.as_mut())
    .await
    .map_err(DomainError::Db)?;

    // Audit write — independent of the read transaction. Pilot leaves this
    // as-is; rollout decision: move inside the tx for atomicity, or keep
    // out for "audit always lands even if main work rolls back" semantics.
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
///
/// Stays on `&PgPool` — auth callers are unauthenticated when this
/// runs (the user is being created or a magic link is being verified),
/// so there is no JWT to scope an `RlsTransaction` to. The per-call
/// `user_id` is set by the auth-flow code, which is itself the
/// authority for who is being authenticated.
///
/// TODO(hardening): if/when consent gains its own RLS policy that
/// requires `app.user_id`, refactor auth flows to begin an
/// `RlsTransaction` with the just-created user id immediately after
/// user creation, and switch this signature to take it.
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

// ── Admin dispute tooling — Hardening §10 ────────────────────────────────────

/// `POST /api/admin/users/:id/ban` — platform-admin-only ban.
///
/// Atomic shape: mark the row banned, revoke any active soultoken, and
/// audit. Cannot ban another platform_admin (defence against compromised
/// admin accounts ban-blasting the team).
pub async fn admin_ban_user(
    tx:                 &mut AdminRlsTransaction,
    pool:               &PgPool, // pool: for audit writes only — audit is outside the transaction so it lands even on rollback
    requesting_user_id: i32,
    target_user_id:     i32,
    reason:             String,
) -> AppResult<()> {
    // 1. Requester must be platform_admin.
    let requester_admin: Option<bool> = sqlx::query_scalar(
        "SELECT is_platform_admin FROM users WHERE id = $1 AND deleted_at IS NULL"
    )
    .bind(requesting_user_id)
    .fetch_optional(tx.as_mut())
    .await
    .map_err(DomainError::Db)?;
    if !matches!(requester_admin, Some(true)) {
        return Err(DomainError::Forbidden);
    }

    // 2. Target must exist and not be a platform_admin.
    let target_admin: Option<bool> = sqlx::query_scalar(
        "SELECT is_platform_admin FROM users WHERE id = $1"
    )
    .bind(target_user_id)
    .fetch_optional(tx.as_mut())
    .await
    .map_err(DomainError::Db)?;
    match target_admin {
        None              => return Err(DomainError::NotFound),
        Some(true)        => return Err(DomainError::Forbidden),
        Some(false)       => {}
    }

    // 3. Mark banned. is_banned remains the source of truth; banned_at +
    //    banned_reason are audit display fields only.
    sqlx::query(
        "UPDATE users SET \
            is_banned     = true, \
            banned_at     = now(), \
            banned_reason = $2, \
            updated_at    = now() \
         WHERE id = $1"
    )
    .bind(target_user_id)
    .bind(&reason)
    .execute(tx.as_mut())
    .await
    .map_err(DomainError::Db)?;

    // 4. Revoke active soultoken if present. We use the soultokens service so
    //    its full audit + verification_event chain runs.
    let active_st: Option<i32> = sqlx::query_scalar(
        "SELECT s.id FROM soultokens s \
         JOIN users u ON u.soultoken_id = s.id \
         WHERE u.id = $1 \
           AND s.revoked_at IS NULL \
           AND s.expires_at > now()"
    )
    .bind(target_user_id)
    .fetch_optional(tx.as_mut())
    .await
    .map_err(DomainError::Db)?;
    if let Some(st_id) = active_st {
        // Cross-domain call: open a fresh RlsTransaction scoped to the
        // requesting admin so revoke_soultoken's RLS context is set
        // correctly. The two operations (ban + revoke) run in separate
        // transactions — partial-failure semantics are unchanged from
        // pre-migration code (which had no tx at all).
        if let Ok(mut st_tx) = RlsTransaction::begin(pool, requesting_user_id).await {
            let _ = crate::domain::soultokens::service::revoke_soultoken(
                &mut st_tx,
                pool,
                st_id,
                UserId::from(requesting_user_id),
                crate::domain::soultokens::types::RevokeSoultokenRequest {
                    revocation_reason:   "platform_ban".to_string(),
                    revocation_visit_id: None,
                },
            ).await;
            let _ = st_tx.commit().await;
        }
    }

    audit::write(
        pool,
        Some(requesting_user_id),
        None,
        "admin.user_banned",
        serde_json::json!({ "target_user_id": target_user_id, "reason": reason }),
    ).await;
    Ok(())
}

/// `POST /api/admin/users/:id/unban` — clears the ban. Soultokens that were
/// revoked by `admin_ban_user` are NOT automatically reissued; the user
/// goes through the verification protocol again if they want one back.
pub async fn admin_unban_user(
    tx:                 &mut AdminRlsTransaction,
    pool:               &PgPool, // pool: for audit writes only — audit is outside the transaction so it lands even on rollback
    requesting_user_id: i32,
    target_user_id:     i32,
) -> AppResult<()> {
    let requester_admin: Option<bool> = sqlx::query_scalar(
        "SELECT is_platform_admin FROM users WHERE id = $1 AND deleted_at IS NULL"
    )
    .bind(requesting_user_id)
    .fetch_optional(tx.as_mut())
    .await
    .map_err(DomainError::Db)?;
    if !matches!(requester_admin, Some(true)) {
        return Err(DomainError::Forbidden);
    }

    let result = sqlx::query(
        "UPDATE users SET \
            is_banned     = false, \
            banned_at     = NULL, \
            banned_reason = NULL, \
            updated_at    = now() \
         WHERE id = $1"
    )
    .bind(target_user_id)
    .execute(tx.as_mut())
    .await
    .map_err(DomainError::Db)?;
    if result.rows_affected() == 0 {
        return Err(DomainError::NotFound);
    }

    audit::write(
        pool,
        Some(requesting_user_id),
        None,
        "admin.user_unbanned",
        serde_json::json!({ "target_user_id": target_user_id }),
    ).await;
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
