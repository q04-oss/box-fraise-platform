use chrono::Utc;
use sqlx::{PgConnection, PgPool};

use crate::{
    audit,
    crypto::{verify_aggregated_ed25519, verify_ed25519},
    error::{AppResult, DomainError},
    event_bus::EventBus,
    events::DomainEvent,
    transaction::RlsTransaction,
    types::UserId,
};
use crate::domain::auth::repository as user_repo;
use super::{
    repository,
    types::{
        InitiateAttestationRequest, RejectAttestationRequest,
        ReviewerSignAttestationRequest, StaffSignAttestationRequest,
        VisitAttestationRow,
    },
};

// ── BFIP Hardening 1c — Ed25519 attestation signing ──────────────────────────

/// Canonical attestation evidence payload. Both reviewers and the delivery
/// staff sign these exact bytes, so anyone holding the row can rebuild it and
/// verify the stored Ed25519 signatures offline.
///
/// Format: `attestation_id|visit_id|user_id|photo_hash|BFIP_ATTESTATION_V1`
/// (`photo_hash` is the empty string when absent).
pub fn attestation_payload(attestation: &VisitAttestationRow) -> String {
    format!(
        "{}|{}|{}|{}|BFIP_ATTESTATION_V1",
        attestation.id,
        attestation.visit_id,
        attestation.user_id,
        attestation.photo_hash.as_deref().unwrap_or(""),
    )
}

/// Format used in `visit_signatures.signature` and `visit_attestations.staff_signature`.
/// The verifying key is stored alongside the signature so a third party can
/// re-run the aggregated Ed25519 verification offline without needing a
/// reviewer-key directory.
fn encode_signature_record(verifying_key_hex: &str, signature_hex: &str) -> String {
    format!("{verifying_key_hex}:{signature_hex}")
}

/// Inverse of [`encode_signature_record`]. Returns `(verifying_key_hex, signature_hex)`.
/// Errors with `InvalidInput` when the stored value does not match the
/// expected `<key>:<sig>` shape.
fn parse_signature_record(stored: &str) -> Result<(String, String), DomainError> {
    let (vk, sig) = stored.split_once(':').ok_or_else(|| {
        DomainError::InvalidInput(
            "stored signature record is malformed — expected verifying_key_hex:signature_hex".to_string(),
        )
    })?;
    if vk.is_empty() || sig.is_empty() {
        return Err(DomainError::InvalidInput(
            "stored signature record has empty verifying key or signature".to_string(),
        ));
    }
    Ok((vk.to_string(), sig.to_string()))
}

/// Verify an incoming Ed25519 signature against the canonical payload.
/// Returns `Ok(())` only on a cryptographically valid signature.
///
// TODO(BFAP): Replace with per-reviewer hardware-bound Ed25519 keys. Today we
// trust whatever verifying key the reviewer's client supplies; BFAP will pin
// each reviewer to a Secure Enclave key registered at staff onboarding.
fn verify_signature(
    verifying_key_hex: &str,
    payload:           &str,
    signature_hex:     &str,
    context:           &'static str,
) -> AppResult<()> {
    match verify_ed25519(verifying_key_hex, payload.as_bytes(), signature_hex) {
        Ok(true)  => Ok(()),
        Ok(false) => Err(DomainError::InvalidInput(format!(
            "invalid {context} signature — Ed25519 verification failed"
        ))),
        Err(e)    => Err(DomainError::InvalidInput(format!(
            "invalid {context} signature — Ed25519 verification failed ({e:?})"
        ))),
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// SHA-256 hex format check: 64 chars, all lowercase hex.
/// Pairs with the canonical server-side
/// `StorageClient::compute_evidence_hash` which produces strings of
/// exactly this shape.
fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// BFIP Section 6.5 — Reviewer assignment algorithm v1.
///
/// Selects two eligible reviewers, excluding those who worked at the delivery
/// staff's location in the last 30 days, or whose pair exceeds the 7-day
/// co-sign collusion limit (>3 times).
///
/// Returns `(reviewer_1_id, reviewer_2_id, cosign_count)`.
async fn assign_reviewers_for_visit(
    conn:        &mut PgConnection,
    staff_id:    i32,
    location_id: i32,
) -> AppResult<(i32, i32, i64)> {
    // 1. Active attestation_reviewers — not the delivery staff, not same-location in 30 days.
    let candidates: Vec<(i32,)> = sqlx::query_as(
        "SELECT DISTINCT sr.user_id
         FROM staff_roles sr
         WHERE sr.role = 'attestation_reviewer'
           AND sr.revoked_at IS NULL
           AND (sr.expires_at IS NULL OR sr.expires_at > now())
           AND sr.user_id != $1
           AND sr.user_id NOT IN (
               SELECT user_id FROM staff_roles
               WHERE location_id = $2
                 AND revoked_at IS NULL
                 AND granted_at > now() - interval '30 days'
           )
         ORDER BY sr.user_id"
    )
    .bind(staff_id)
    .bind(location_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(DomainError::Db)?;

    if candidates.len() < 2 {
        return Err(DomainError::InvalidInput(
            "not enough eligible reviewers (need 2; check reviewer staffing and location exclusions)"
                .to_string(),
        ));
    }

    let ids: Vec<i32> = candidates.into_iter().map(|(id,)| id).collect();

    // 2. Find a pair where cosign_count <= 3 in the last 7 days.
    for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            let r1 = ids[i];
            let r2 = ids[j];
            let cosign: i64 = sqlx::query_scalar(
                "SELECT COUNT(*)
                 FROM visit_signatures vs1
                 JOIN visit_signatures vs2 ON vs1.visit_id = vs2.visit_id
                 WHERE vs1.reviewer_id = $1
                   AND vs2.reviewer_id = $2
                   AND vs1.signed_at IS NOT NULL
                   AND vs2.signed_at IS NOT NULL
                   AND vs1.signed_at > now() - interval '7 days'"
            )
            .bind(r1)
            .bind(r2)
            .fetch_one(&mut *conn)
            .await
            .map_err(DomainError::Db)?;

            if cosign <= 3 {
                return Ok((r1, r2, cosign));
            }
        }
    }

    Err(DomainError::InvalidInput(
        "all eligible reviewer pairs exceed the co-sign collusion limit (>3 in 7 days)".to_string(),
    ))
}

// ── Service functions ─────────────────────────────────────────────────────────

/// Initiate a staff attestation (BFIP Section 6.3).
///
/// Requesting user must be the delivery staff for the visit.
/// Target user must have `verification_status = 'presence_confirmed'`.
pub async fn initiate_attestation(
    tx:                 &mut RlsTransaction,
    pool:               &PgPool, // pool: for audit writes only — audit is outside the transaction so it lands even on rollback
    requesting_user_id: UserId,
    req:                InitiateAttestationRequest,
    event_bus:          &EventBus,
) -> AppResult<VisitAttestationRow> {
    let uid = i32::from(requesting_user_id);

    // 1. Load visit — must be in_progress.
    let (staff_id, location_id, status): (i32, i32, String) = sqlx::query_as(
        "SELECT staff_id, location_id, status FROM staff_visits WHERE id = $1"
    )
    .bind(req.visit_id)
    .fetch_optional(tx.as_mut())
    .await
    .map_err(DomainError::Db)?
    .ok_or(DomainError::NotFound)?;

    if status != "in_progress" {
        return Err(DomainError::Conflict(
            "visit must be in_progress to initiate attestation".to_string(),
        ));
    }

    // 2. Requesting user must be the assigned delivery staff.
    if staff_id != uid {
        return Err(DomainError::Forbidden);
    }

    // 3. Target user must be presence_confirmed.
    //    Cross-domain repository (auth) still takes `&PgPool` — call with pool.
    let target = user_repo::find_by_id(pool, UserId::from(req.user_id))
        .await?
        .ok_or(DomainError::NotFound)?;

    if target.verification_status != "presence_confirmed" {
        return Err(DomainError::InvalidInput(
            "user must have verification_status 'presence_confirmed' for attestation".to_string(),
        ));
    }

    // 4. Validate presence_threshold belongs to this user and is met.
    let (pt_user_id, threshold_met_at): (i32, Option<chrono::DateTime<Utc>>) =
        sqlx::query_as(
            "SELECT user_id, threshold_met_at FROM presence_thresholds WHERE id = $1"
        )
        .bind(req.presence_threshold_id)
        .fetch_optional(tx.as_mut())
        .await
        .map_err(DomainError::Db)?
        .ok_or_else(|| DomainError::InvalidInput("presence_threshold not found".to_string()))?;

    if pt_user_id != req.user_id {
        return Err(DomainError::InvalidInput(
            "presence_threshold does not belong to this user".to_string(),
        ));
    }
    if threshold_met_at.is_none() {
        return Err(DomainError::InvalidInput(
            "presence threshold has not been met yet".to_string(),
        ));
    }

    // 5. No active (non-rejected) attestation for this visit + user.
    let active: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM visit_attestations \
         WHERE visit_id = $1 AND user_id = $2 AND status != 'rejected'"
    )
    .bind(req.visit_id)
    .bind(req.user_id)
    .fetch_one(tx.as_mut())
    .await
    .map_err(DomainError::Db)?;

    if active > 0 {
        return Err(DomainError::Conflict(
            "an active attestation already exists for this visit and user".to_string(),
        ));
    }

    // 6. Validate photo hash + URI invariants (Hardening cleanup #5).
    //
    // Full hash verification (download from S3 + recompute) is deferred —
    // the upload endpoint at `POST /api/staff/visits/:id/evidence` already
    // computes the hash server-side via
    // `StorageClient::compute_evidence_hash`, so we validate format and
    // presence only here.
    //
    // TODO(hardening): consider storing the server-computed hash at upload
    // time and looking it up by `photo_storage_uri` instead of trusting
    // the client-provided value at all.
    if req.photo_storage_uri.is_some() && req.photo_hash.is_none() {
        return Err(DomainError::InvalidInput(
            "photo_hash is required when photo_storage_uri is provided".to_string(),
        ));
    }
    if let Some(hash) = req.photo_hash.as_deref() {
        if !is_sha256_hex(hash) {
            return Err(DomainError::InvalidInput(
                "photo_hash must be a 64-character lowercase hex string (SHA-256)".to_string(),
            ));
        }
    }

    // 7. Assign two eligible reviewers (BFIP Section 6.5).
    let (r1_id, r2_id, cosign_count) =
        assign_reviewers_for_visit(tx.as_mut(), uid, location_id).await?;

    // 8. Create attestation record.
    let attestation = repository::create_attestation(
        tx.as_mut(),
        req.visit_id,
        req.user_id,
        uid,
        req.presence_threshold_id,
        r1_id,
        r2_id,
        req.photo_hash.as_deref(),
        req.photo_storage_uri.as_deref(),
    ).await?;

    // 9. Log reviewer assignments to reviewer_assignment_log.
    let details = serde_json::json!({
        "same_location_30d": false,
        "cosign_7d":          cosign_count,
    });
    let _ = repository::log_reviewer_assignment(
        tx.as_mut(), req.visit_id, r1_id, cosign_count as i32, true, details.clone(),
    ).await;
    let _ = repository::log_reviewer_assignment(
        tx.as_mut(), req.visit_id, r2_id, cosign_count as i32, true, details,
    ).await;

    // 10. Audit + event.
    // Audit writes use `pool` (separate connection) — they commit
    // independently so the audit row lands even if `tx` is rolled back.
    audit::write(
        pool,
        Some(uid),
        None,
        "attestation.initiated",
        serde_json::json!({
            "attestation_id": attestation.id,
            "user_id":        req.user_id,
            "visit_id":       req.visit_id,
        }),
    ).await;

    event_bus.publish(DomainEvent::AttestationInitiated {
        attestation_id: attestation.id,
        user_id:        req.user_id,
        visit_id:       req.visit_id,
    });

    Ok(attestation)
}

/// Record the delivery staff's signature on an attestation (BFIP Section 6.4).
///
/// Sets `status = 'co_sign_pending'` and opens the 48-hour co-sign window.
/// Inserts `visit_signatures` rows for both assigned reviewers.
pub async fn staff_sign(
    tx:                 &mut RlsTransaction,
    pool:               &PgPool, // pool: for audit writes only — audit is outside the transaction so it lands even on rollback
    attestation_id:     i32,
    requesting_user_id: UserId,
    req:                StaffSignAttestationRequest,
    _event_bus:         &EventBus,
) -> AppResult<VisitAttestationRow> {
    let uid = i32::from(requesting_user_id);

    // 1. Load attestation — must be 'pending'.
    let attest = repository::get_attestation_by_id(tx.as_mut(), attestation_id)
        .await?
        .ok_or(DomainError::NotFound)?;

    if attest.status != "pending" {
        return Err(DomainError::Conflict(
            "attestation must be in 'pending' status for staff sign".to_string(),
        ));
    }

    // 2. Requesting user must be the delivery staff.
    if attest.staff_id != uid {
        return Err(DomainError::Forbidden);
    }

    // 3. Verify the staff Ed25519 signature against the canonical payload.
    //    Use the row that *would* exist after staff_sign — photo_hash may be
    //    overridden by the request body, but id/visit_id/user_id are stable.
    let payload_attest = if req.photo_hash.is_some() {
        VisitAttestationRow {
            photo_hash: req.photo_hash.clone(),
            ..attest.clone()
        }
    } else {
        attest.clone()
    };
    let payload = attestation_payload(&payload_attest);
    verify_signature(
        &req.verifying_key_hex,
        &payload,
        &req.staff_signature,
        "staff",
    )?;
    let staff_sig_record = encode_signature_record(&req.verifying_key_hex, &req.staff_signature);

    // 4. Set co_sign_deadline to now() + 48 hours.
    let deadline = Utc::now() + chrono::Duration::hours(48);

    // 5. Update attestation with signature and set status to co_sign_pending.
    let updated = repository::update_attestation_staff_signed(
        tx.as_mut(),
        attestation_id,
        &staff_sig_record,
        req.photo_hash.as_deref(),
        req.location_confirmed,
        req.user_present_confirmed,
        deadline,
    ).await?;

    // 6. Audit event.
    audit::write(
        pool,
        Some(uid),
        None,
        "attestation.staff_signed",
        serde_json::json!({ "attestation_id": attestation_id }),
    ).await;

    Ok(updated)
}

/// Record a reviewer's co-signature on an attestation (BFIP Section 6.6).
///
/// When both assigned reviewers have signed, the attestation is approved
/// and the user is promoted to `verification_status = 'attested'`.
pub async fn reviewer_sign(
    tx:                 &mut RlsTransaction,
    pool:               &PgPool, // pool: for audit writes only — audit is outside the transaction so it lands even on rollback
    attestation_id:     i32,
    requesting_user_id: UserId,
    req:                ReviewerSignAttestationRequest,
    event_bus:          &EventBus,
) -> AppResult<VisitAttestationRow> {
    let uid = i32::from(requesting_user_id);

    // 1. Load attestation — must be 'co_sign_pending'.
    let attest = repository::get_attestation_by_id(tx.as_mut(), attestation_id)
        .await?
        .ok_or(DomainError::NotFound)?;

    if attest.status != "co_sign_pending" {
        return Err(DomainError::Conflict(
            "attestation must be in 'co_sign_pending' status for reviewer sign".to_string(),
        ));
    }

    // 2. Requesting user must be an assigned reviewer.
    if attest.assigned_reviewer_1_id != uid && attest.assigned_reviewer_2_id != uid {
        return Err(DomainError::Forbidden);
    }

    // 3. Deadline must not have passed.
    if let Some(deadline) = attest.co_sign_deadline {
        if Utc::now() > deadline {
            return Err(DomainError::Conflict("co-sign deadline has passed".to_string()));
        }
    }

    // 4. Verify the reviewer's Ed25519 signature over the canonical payload
    //    BEFORE persisting. Anything that fails verification never reaches the
    //    DB, so a stored signature is always cryptographically valid.
    let payload = attestation_payload(&attest);
    verify_signature(
        &req.verifying_key_hex,
        &payload,
        &req.signature,
        "reviewer",
    )?;
    let sig_record = encode_signature_record(&req.verifying_key_hex, &req.signature);

    // 5. Record signature — INSERT with all fields (signature col is NOT NULL).
    //    ON CONFLICT DO NOTHING guards against double-signing.
    let sign_deadline = attest
        .co_sign_deadline
        .unwrap_or_else(|| Utc::now() + chrono::Duration::hours(48));
    repository::record_reviewer_signature(
        tx.as_mut(),
        attest.visit_id,
        uid,
        sign_deadline,
        &sig_record,
        &req.evidence_hash_reviewed,
    ).await?;

    // 6. Check if both reviewers have now signed.
    let both_signed = repository::check_both_reviewers_signed(
        tx.as_mut(),
        attest.visit_id,
        attest.assigned_reviewer_1_id,
        attest.assigned_reviewer_2_id,
    ).await?;

    if let Some((sig_record_1, sig_record_2)) = both_signed {
        // 6a. Re-verify both reviewer signatures aggregated against the
        //     payload. This is belt-and-suspenders: each was already verified
        //     individually before storage, but aggregated verify proves the
        //     stored records can be replayed end-to-end by an auditor.
        let (vk1, sig1) = parse_signature_record(&sig_record_1)?;
        let (vk2, sig2) = parse_signature_record(&sig_record_2)?;
        let aggregated_ok = verify_aggregated_ed25519(
            &[vk1.as_str(), vk2.as_str()],
            payload.as_bytes(),
            &[sig1.as_str(), sig2.as_str()],
        )
        .map_err(|e| DomainError::InvalidInput(format!(
            "aggregated Ed25519 verification failed ({e:?})"
        )))?;
        if !aggregated_ok {
            return Err(DomainError::InvalidInput(
                "aggregated Ed25519 verification failed".to_string(),
            ));
        }

        // 6b. Approve attestation.
        let approved = repository::approve_attestation(tx.as_mut(), attestation_id).await?;

        // 6b. Promote user to 'attested'.
        sqlx::query(
            "UPDATE users SET verification_status = 'attested', \
             attested_at = now(), updated_at = now() WHERE id = $1"
        )
        .bind(attest.user_id)
        .execute(tx.as_mut())
        .await
        .map_err(DomainError::Db)?;

        // 6c. Record attempt.
        let _ = repository::record_attempt(
            tx.as_mut(),
            attest.user_id,
            attestation_id,
            attest.visit_id,
            attest.assigned_reviewer_1_id,
            attest.assigned_reviewer_2_id,
            attest.attempt_number,
            "approved",
            None,
            None,
        ).await;

        // 6d. Audit + event.
        audit::write(
            pool,
            Some(uid),
            None,
            "attestation.approved",
            serde_json::json!({
                "attestation_id": attestation_id,
                "user_id":        attest.user_id,
            }),
        ).await;

        event_bus.publish(DomainEvent::AttestationApproved {
            attestation_id,
            user_id: attest.user_id,
        });

        Ok(approved)
    } else {
        audit::write(
            pool,
            Some(uid),
            None,
            "attestation.reviewer_signed",
            serde_json::json!({ "attestation_id": attestation_id, "reviewer_id": uid }),
        ).await;

        repository::get_attestation_by_id(tx.as_mut(), attestation_id)
            .await?
            .ok_or(DomainError::NotFound)
    }
}

/// Reject an attestation (BFIP Section 6.7).
///
/// Only an assigned reviewer may reject. The user's status is reset to
/// `'presence_confirmed'` and the attempt is recorded in `attestation_attempts`.
pub async fn reject_attestation(
    tx:                 &mut RlsTransaction,
    pool:               &PgPool, // pool: for audit writes only — audit is outside the transaction so it lands even on rollback
    attestation_id:     i32,
    requesting_user_id: UserId,
    req:                RejectAttestationRequest,
    event_bus:          &EventBus,
) -> AppResult<VisitAttestationRow> {
    let uid = i32::from(requesting_user_id);

    // 1. Load attestation.
    let attest = repository::get_attestation_by_id(tx.as_mut(), attestation_id)
        .await?
        .ok_or(DomainError::NotFound)?;

    // 2. Status must be 'pending' or 'co_sign_pending'.
    if !["pending", "co_sign_pending"].contains(&attest.status.as_str()) {
        return Err(DomainError::Conflict(
            "attestation cannot be rejected in its current status".to_string(),
        ));
    }

    // 3. Only assigned reviewers may reject.
    if attest.assigned_reviewer_1_id != uid && attest.assigned_reviewer_2_id != uid {
        return Err(DomainError::Forbidden);
    }

    // 4. Set status = 'rejected'.
    let rejected = repository::set_rejected(tx.as_mut(), attestation_id).await?;

    // 5. Reset user to 'presence_confirmed'.
    sqlx::query(
        "UPDATE users SET verification_status = 'presence_confirmed', updated_at = now() \
         WHERE id = $1"
    )
    .bind(attest.user_id)
    .execute(tx.as_mut())
    .await
    .map_err(DomainError::Db)?;

    // 6. Record attempt.
    let _ = repository::record_attempt(
        tx.as_mut(),
        attest.user_id,
        attestation_id,
        attest.visit_id,
        attest.assigned_reviewer_1_id,
        attest.assigned_reviewer_2_id,
        attest.attempt_number,
        "rejected",
        Some(&req.rejection_reason),
        Some(uid),
    ).await;

    // 7. Audit + event.
    audit::write(
        pool,
        Some(uid),
        None,
        "attestation.rejected",
        serde_json::json!({
            "attestation_id":        attestation_id,
            "user_id":               attest.user_id,
            "rejection_reason":      &req.rejection_reason,
            "rejection_reviewer_id": uid,
        }),
    ).await;

    event_bus.publish(DomainEvent::AttestationRejected {
        attestation_id,
        user_id:               attest.user_id,
        rejection_reviewer_id: uid,
    });

    Ok(rejected)
}

/// List attestations in `'co_sign_pending'` status assigned to this reviewer.
///
/// Read-only, no audit — kept on `&PgPool` per cleanup #3 rule. Acquires a
/// pool connection internally to satisfy the repository's `&mut PgConnection`
/// signature.
pub async fn list_pending_for_reviewer(
    pool:    &PgPool,
    user_id: UserId,
) -> AppResult<Vec<VisitAttestationRow>> {
    let mut conn = pool.acquire().await.map_err(DomainError::Db)?;
    repository::get_pending_attestations_for_reviewer(&mut conn, i32::from(user_id)).await
}

/// List all attestations for the authenticated user (as the attested person).
///
/// Read-only, no audit — kept on `&PgPool` per cleanup #3 rule. Acquires a
/// pool connection internally to satisfy the repository's `&mut PgConnection`
/// signature.
pub async fn list_my_attestations(
    pool:    &PgPool,
    user_id: UserId,
) -> AppResult<Vec<VisitAttestationRow>> {
    let mut conn = pool.acquire().await.map_err(DomainError::Db)?;
    repository::get_attestations_by_user(&mut conn, i32::from(user_id)).await
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        crypto::Ed25519KeyPair,
        domain::staff::{
            service as staff_svc,
            types::{ArriveAtVisitRequest, GrantRoleRequest, ScheduleVisitRequest},
        },
        event_bus::EventBus,
        types::UserId,
        with_admin_tx, with_rls_tx,
    };
    use sqlx::PgPool;

    /// Sign `payload` with `kp` and return `(verifying_key_hex, signature_hex)`
    /// — the two values that a real client would send in a sign request.
    fn signed_pair(payload: &str, kp: &Ed25519KeyPair) -> (String, String) {
        (kp.verifying_key_hex(), kp.sign(payload.as_bytes()))
    }

    // ── Test context ──────────────────────────────────────────────────────────

    struct Ctx {
        admin:        UserId,
        staff:        UserId,
        reviewer_1:   UserId,
        reviewer_2:   UserId,
        visit_id:     i32,
        target:       UserId,
        threshold_id: i32,
    }

    /// Run `initiate_attestation` with a fresh `RlsTransaction` scoped to the
    /// requesting staff user — every test that previously passed `&pool`
    /// directly to the service now goes through this helper.
    async fn run_initiate(
        pool: &PgPool,
        staff: UserId,
        req: InitiateAttestationRequest,
        bus: &EventBus,
    ) -> AppResult<VisitAttestationRow> {
        Ok(with_rls_tx!(pool, staff, |tx| {
            initiate_attestation(&mut tx, pool, staff, req, bus).await?
        }))
    }

    /// Full attestation context: admin, delivery staff, 2 reviewers, in-progress
    /// visit, and a presence-confirmed target user with a met threshold.
    async fn setup(pool: &PgPool) -> Ctx {
        use fake::{Fake, faker::internet::en::SafeEmail};

        let bus = EventBus::new();

        let mk_admin = |email: String| async move {
            let (id,): (i32,) = sqlx::query_as(
                "INSERT INTO users (email, email_verified, is_platform_admin) \
                 VALUES ($1, true, true) RETURNING id",
            )
            .bind(&email)
            .fetch_one(pool)
            .await
            .unwrap();
            UserId::from(id)
        };

        let mk_user = |email: String| async move {
            let (id,): (i32,) = sqlx::query_as(
                "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id",
            )
            .bind(&email)
            .fetch_one(pool)
            .await
            .unwrap();
            UserId::from(id)
        };

        let admin = mk_admin(SafeEmail().fake::<String>()).await;
        let staff = mk_user(SafeEmail().fake::<String>()).await;

        let (loc_id,): (i32,) = sqlx::query_as(
            "INSERT INTO locations (name, location_type, address, timezone) \
             VALUES ('Attest Store', 'box_fraise_store', '1 Attest St', 'America/Edmonton') \
             RETURNING id",
        )
        .fetch_one(pool)
        .await
        .unwrap();

        let (biz_id,): (i32,) = sqlx::query_as(
            "INSERT INTO businesses (location_id, primary_holder_id, name, verification_status) \
             VALUES ($1, $2, 'Attest Biz', 'active') RETURNING id",
        )
        .bind(loc_id)
        .bind(i32::from(admin))
        .fetch_one(pool)
        .await
        .unwrap();

        // Grant delivery_staff role.
        with_admin_tx!(pool, |tx| {
            staff_svc::grant_staff_role(
                &mut tx,
                pool,
                admin,
                GrantRoleRequest {
                    user_id:      i32::from(staff),
                    role:         "delivery_staff".to_owned(),
                    location_id:  Some(loc_id),
                    expires_at:   None,
                    confirmed_by: None,
                },
                &bus,
            )
            .await
            .unwrap();
        });

        // Schedule + arrive at visit.
        let visit = with_rls_tx!(pool, staff, |tx| {
            staff_svc::schedule_visit(
                &mut tx,
                pool,
                staff,
                ScheduleVisitRequest {
                    location_id:              loc_id,
                    visit_type:               "combined".to_owned(),
                    scheduled_at:             chrono::Utc::now() + chrono::Duration::hours(1),
                    window_hours:             Some(4),
                    support_booking_capacity: Some(0),
                    expected_box_count:       Some(0),
                },
                &bus,
            )
            .await
            .unwrap()
        });

        with_rls_tx!(pool, staff, |tx| {
            staff_svc::arrive_at_visit(
                &mut tx,
                pool,
                visit.id,
                staff,
                ArriveAtVisitRequest { arrived_latitude: None, arrived_longitude: None },
            )
            .await
            .unwrap();
        });

        // Create 2 attestation reviewers (no location — eligible everywhere).
        let r1 = mk_user(SafeEmail().fake::<String>()).await;
        let r2 = mk_user(SafeEmail().fake::<String>()).await;

        for rid in [r1, r2] {
            with_admin_tx!(pool, |tx| {
                staff_svc::grant_staff_role(
                    &mut tx,
                    pool,
                    admin,
                    GrantRoleRequest {
                        user_id:      i32::from(rid),
                        role:         "attestation_reviewer".to_owned(),
                        location_id:  None,
                        expires_at:   None,
                        confirmed_by: None,
                    },
                    &bus,
                )
                .await
                .unwrap();
            });
        }

        // Target user: presence_confirmed status.
        let (target_id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified, verification_status) \
             VALUES ($1, true, 'presence_confirmed') RETURNING id",
        )
        .bind(&SafeEmail().fake::<String>())
        .fetch_one(pool)
        .await
        .unwrap();

        // Met presence threshold for the target user.
        let (threshold_id,): (i32,) = sqlx::query_as(
            "INSERT INTO presence_thresholds \
             (user_id, business_id, event_count, days_count, threshold_met_at) \
             VALUES ($1, $2, 3, 3, now()) RETURNING id",
        )
        .bind(target_id)
        .bind(biz_id)
        .fetch_one(pool)
        .await
        .unwrap();

        Ctx {
            admin,
            staff,
            reviewer_1: r1,
            reviewer_2: r2,
            visit_id: visit.id,
            target: UserId::from(target_id),
            threshold_id,
        }
    }

    fn initiate_req(ctx: &Ctx) -> InitiateAttestationRequest {
        InitiateAttestationRequest {
            visit_id:              ctx.visit_id,
            user_id:               i32::from(ctx.target),
            presence_threshold_id: ctx.threshold_id,
            // Valid SHA-256 hex (64 lowercase hex chars) — required by the
            // validator added in cleanup #5. Same fixture for every test
            // — the value isn't checked against the actual photo bytes
            // (server-side recompute is deferred — see initiate_attestation).
            photo_hash:            Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_owned()),
            photo_storage_uri:     Some("evidence/visits/test/photo".to_owned()),
        }
    }

    async fn run_staff_sign(pool: &PgPool, ctx: &Ctx, attestation_id: i32) -> VisitAttestationRow {
        let bus = EventBus::new();
        let mut conn = pool.acquire().await.unwrap();
        let attest = repository::get_attestation_by_id(&mut conn, attestation_id)
            .await.unwrap().unwrap();
        drop(conn);
        let payload = attestation_payload(&attest);
        let kp = Ed25519KeyPair::generate();
        let (vk, sig) = signed_pair(&payload, &kp);
        with_rls_tx!(pool, ctx.staff, |tx| {
            staff_sign(
                &mut tx,
                pool,
                attestation_id,
                ctx.staff,
                StaffSignAttestationRequest {
                    staff_signature:        sig,
                    verifying_key_hex:      vk,
                    photo_hash:             None,
                    location_confirmed:     true,
                    user_present_confirmed: true,
                },
                &bus,
            )
            .await
            .expect("staff_sign must succeed")
        })
    }

    async fn run_reviewer_sign(
        pool:          &PgPool,
        attestation_id: i32,
        reviewer:      UserId,
    ) -> VisitAttestationRow {
        let bus = EventBus::new();
        let mut conn = pool.acquire().await.unwrap();
        let attest = repository::get_attestation_by_id(&mut conn, attestation_id)
            .await.unwrap().unwrap();
        drop(conn);
        let payload = attestation_payload(&attest);
        let kp = Ed25519KeyPair::generate();
        let (vk, sig) = signed_pair(&payload, &kp);
        with_rls_tx!(pool, reviewer, |tx| {
            reviewer_sign(
                &mut tx,
                pool,
                attestation_id,
                reviewer,
                ReviewerSignAttestationRequest {
                    signature:              sig,
                    verifying_key_hex:      vk,
                    evidence_hash_reviewed: "evidence-hash".to_owned(),
                },
                &bus,
            )
            .await
            .expect("reviewer_sign must succeed")
        })
    }

    // ── Tests 1–3: initiate_attestation ──────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn initiate_attestation_assigns_two_reviewers_and_creates_record(pool: PgPool) {
        let ctx = setup(&pool).await;
        let bus = EventBus::new();

        let attest = run_initiate(&pool, ctx.staff, initiate_req(&ctx), &bus)
            .await
            .expect("initiate_attestation must succeed");

        assert_eq!(attest.status, "pending");
        assert_eq!(attest.visit_id, ctx.visit_id);
        assert_eq!(attest.user_id, i32::from(ctx.target));
        assert_ne!(attest.assigned_reviewer_1_id, attest.assigned_reviewer_2_id);
    }

    /// Hardening cleanup #5: a client supplying `photo_storage_uri`
    /// without an accompanying `photo_hash` must be rejected — without
    /// the hash there's no integrity anchor for the uploaded photo.
    #[sqlx::test(migrations = "../server/migrations")]
    async fn initiate_attestation_rejects_photo_uri_without_hash(pool: PgPool) {
        let ctx = setup(&pool).await;
        let bus = EventBus::new();

        let err = run_initiate(
            &pool,
            ctx.staff,
            InitiateAttestationRequest {
                visit_id:              ctx.visit_id,
                user_id:               i32::from(ctx.target),
                presence_threshold_id: ctx.threshold_id,
                photo_hash:            None,
                photo_storage_uri:     Some("evidence/visits/1/photo".to_owned()),
            },
            &bus,
        ).await.unwrap_err();
        match err {
            DomainError::InvalidInput(msg) =>
                assert!(msg.contains("photo_hash"), "msg should mention photo_hash, got: {msg}"),
            other => panic!("expected InvalidInput, got {other:?}"),
        }
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn initiate_attestation_fails_if_user_not_presence_confirmed(pool: PgPool) {
        let ctx = setup(&pool).await;
        let bus = EventBus::new();

        // Insert a non-presence_confirmed user.
        let (other_id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified, verification_status) \
             VALUES ('not-confirmed@test.test', true, 'identity_confirmed') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        let err = run_initiate(
            &pool,
            ctx.staff,
            InitiateAttestationRequest {
                visit_id:              ctx.visit_id,
                user_id:               other_id,
                presence_threshold_id: ctx.threshold_id,
                photo_hash:            None,
                photo_storage_uri:     None,
            },
            &bus,
        )
        .await
        .unwrap_err();

        assert!(
            matches!(err, DomainError::InvalidInput(_)),
            "expected InvalidInput, got: {err:?}"
        );
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn initiate_attestation_fails_with_no_eligible_reviewers(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let bus = EventBus::new();

        // Minimal setup with no attestation_reviewer roles in the DB.
        let (admin_id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified, is_platform_admin) \
             VALUES ('admin@nr.test', true, true) RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let admin = UserId::from(admin_id);

        let (staff_id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified) VALUES ('staff@nr.test', true) RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let staff = UserId::from(staff_id);

        let (loc_id,): (i32,) = sqlx::query_as(
            "INSERT INTO locations (name, location_type, address, timezone) \
             VALUES ('NR Store', 'box_fraise_store', '1 NR St', 'America/Edmonton') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        let (biz_id,): (i32,) = sqlx::query_as(
            "INSERT INTO businesses (location_id, primary_holder_id, name, verification_status) \
             VALUES ($1, $2, 'NR Biz', 'active') RETURNING id",
        )
        .bind(loc_id)
        .bind(admin_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        with_admin_tx!(&pool, |tx| {
            staff_svc::grant_staff_role(
                &mut tx,
                &pool,
                admin,
                GrantRoleRequest {
                    user_id: staff_id, role: "delivery_staff".to_owned(),
                    location_id: Some(loc_id), expires_at: None, confirmed_by: None,
                },
                &bus,
            )
            .await
            .unwrap();
        });

        let visit = with_rls_tx!(&pool, staff, |tx| {
            staff_svc::schedule_visit(
                &mut tx,
                &pool,
                staff,
                ScheduleVisitRequest {
                    location_id: loc_id, visit_type: "delivery".to_owned(),
                    scheduled_at: chrono::Utc::now() + chrono::Duration::hours(1),
                    window_hours: Some(4), support_booking_capacity: Some(0), expected_box_count: Some(0),
                },
                &bus,
            )
            .await
            .unwrap()
        });

        with_rls_tx!(&pool, staff, |tx| {
            staff_svc::arrive_at_visit(
                &mut tx,
                &pool, visit.id, staff,
                ArriveAtVisitRequest { arrived_latitude: None, arrived_longitude: None },
            )
            .await
            .unwrap();
        });

        let (target_id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified, verification_status) \
             VALUES ($1, true, 'presence_confirmed') RETURNING id",
        )
        .bind(&SafeEmail().fake::<String>())
        .fetch_one(&pool)
        .await
        .unwrap();

        let (threshold_id,): (i32,) = sqlx::query_as(
            "INSERT INTO presence_thresholds \
             (user_id, business_id, event_count, days_count, threshold_met_at) \
             VALUES ($1, $2, 3, 3, now()) RETURNING id",
        )
        .bind(target_id)
        .bind(biz_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        let err = run_initiate(
            &pool,
            staff,
            InitiateAttestationRequest {
                visit_id: visit.id, user_id: target_id, presence_threshold_id: threshold_id,
                photo_hash: None, photo_storage_uri: None,
            },
            &bus,
        )
        .await
        .unwrap_err();

        assert!(
            matches!(err, DomainError::InvalidInput(_)),
            "expected InvalidInput (no reviewers), got: {err:?}"
        );
    }

    // ── Tests 4–5: staff_sign ─────────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn staff_sign_sets_status_to_co_sign_pending(pool: PgPool) {
        let ctx = setup(&pool).await;
        let bus = EventBus::new();

        let attest = run_initiate(&pool, ctx.staff, initiate_req(&ctx), &bus)
            .await
            .unwrap();

        let signed = run_staff_sign(&pool, &ctx, attest.id).await;

        assert_eq!(signed.status, "co_sign_pending");
        assert!(signed.staff_signature.is_some());
        assert!(signed.co_sign_deadline.is_some());
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn staff_sign_fails_if_not_assigned_staff(pool: PgPool) {
        let ctx = setup(&pool).await;
        let bus = EventBus::new();

        let attest = run_initiate(&pool, ctx.staff, initiate_req(&ctx), &bus)
            .await
            .unwrap();

        // Forbidden short-circuits before signature verification.
        let mut tx = RlsTransaction::begin(&pool, i32::from(ctx.reviewer_1)).await.unwrap();
        let err = staff_sign(
            &mut tx,
            &pool,
            attest.id,
            ctx.reviewer_1,
            StaffSignAttestationRequest {
                staff_signature:        "impostor-sig".to_owned(),
                verifying_key_hex:      "00".repeat(32),
                photo_hash:             None,
                location_confirmed:     true,
                user_present_confirmed: true,
            },
            &bus,
        )
        .await
        .unwrap_err();

        assert!(matches!(err, DomainError::Forbidden));
    }

    // ── Tests 6–9: reviewer_sign / approve ────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn first_reviewer_sign_keeps_co_sign_pending(pool: PgPool) {
        let ctx = setup(&pool).await;
        let bus = EventBus::new();

        let attest = run_initiate(&pool, ctx.staff, initiate_req(&ctx), &bus).await.unwrap();
        run_staff_sign(&pool, &ctx, attest.id).await;

        let after = run_reviewer_sign(&pool, attest.id, ctx.reviewer_1).await;
        assert_eq!(after.status, "co_sign_pending", "one reviewer signed, still pending");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn second_reviewer_sign_approves_attestation(pool: PgPool) {
        let ctx = setup(&pool).await;
        let bus = EventBus::new();

        let attest = run_initiate(&pool, ctx.staff, initiate_req(&ctx), &bus).await.unwrap();
        run_staff_sign(&pool, &ctx, attest.id).await;
        run_reviewer_sign(&pool, attest.id, ctx.reviewer_1).await;

        let approved = run_reviewer_sign(&pool, attest.id, ctx.reviewer_2).await;
        assert_eq!(approved.status, "approved");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn approved_attestation_promotes_user_to_attested(pool: PgPool) {
        let ctx = setup(&pool).await;
        let bus = EventBus::new();

        let attest = run_initiate(&pool, ctx.staff, initiate_req(&ctx), &bus).await.unwrap();
        run_staff_sign(&pool, &ctx, attest.id).await;
        run_reviewer_sign(&pool, attest.id, ctx.reviewer_1).await;
        run_reviewer_sign(&pool, attest.id, ctx.reviewer_2).await;

        let status: String = sqlx::query_scalar(
            "SELECT verification_status FROM users WHERE id = $1",
        )
        .bind(i32::from(ctx.target))
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(status, "attested");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn approved_attestation_records_attempt(pool: PgPool) {
        let ctx = setup(&pool).await;
        let bus = EventBus::new();

        let attest = run_initiate(&pool, ctx.staff, initiate_req(&ctx), &bus).await.unwrap();
        run_staff_sign(&pool, &ctx, attest.id).await;
        run_reviewer_sign(&pool, attest.id, ctx.reviewer_1).await;
        run_reviewer_sign(&pool, attest.id, ctx.reviewer_2).await;

        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attestation_attempts \
             WHERE attestation_id = $1 AND outcome = 'approved'",
        )
        .bind(attest.id)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(count, 1, "approved attempt must be recorded");
    }

    // ── Tests 10–11: reject_attestation ──────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn rejected_attestation_records_attempt_with_rejected_outcome(pool: PgPool) {
        let ctx = setup(&pool).await;
        let bus = EventBus::new();

        let attest = run_initiate(&pool, ctx.staff, initiate_req(&ctx), &bus).await.unwrap();
        run_staff_sign(&pool, &ctx, attest.id).await;

        with_rls_tx!(&pool, ctx.reviewer_1, |tx| {
            reject_attestation(
                &mut tx,
                &pool,
                attest.id,
                ctx.reviewer_1,
                RejectAttestationRequest { rejection_reason: "identity mismatch".to_owned() },
                &bus,
            )
            .await
            .expect("reject must succeed");
        });

        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attestation_attempts \
             WHERE attestation_id = $1 AND outcome = 'rejected'",
        )
        .bind(attest.id)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(count, 1);
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn rejected_attestation_resets_user_to_presence_confirmed(pool: PgPool) {
        let ctx = setup(&pool).await;
        let bus = EventBus::new();

        let attest = run_initiate(&pool, ctx.staff, initiate_req(&ctx), &bus).await.unwrap();
        run_staff_sign(&pool, &ctx, attest.id).await;

        with_rls_tx!(&pool, ctx.reviewer_1, |tx| {
            reject_attestation(
                &mut tx,
                &pool,
                attest.id,
                ctx.reviewer_1,
                RejectAttestationRequest { rejection_reason: "photo mismatch".to_owned() },
                &bus,
            )
            .await
            .unwrap();
        });

        let status: String = sqlx::query_scalar(
            "SELECT verification_status FROM users WHERE id = $1",
        )
        .bind(i32::from(ctx.target))
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(status, "presence_confirmed");
    }

    // ── Tests 12–13: list queries ─────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn list_pending_for_reviewer_returns_co_sign_pending(pool: PgPool) {
        let ctx = setup(&pool).await;
        let bus = EventBus::new();

        let attest = run_initiate(&pool, ctx.staff, initiate_req(&ctx), &bus).await.unwrap();
        run_staff_sign(&pool, &ctx, attest.id).await;

        let pending = list_pending_for_reviewer(&pool, ctx.reviewer_1).await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].status, "co_sign_pending");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn list_my_attestations_returns_own_rows(pool: PgPool) {
        let ctx = setup(&pool).await;
        let bus = EventBus::new();

        run_initiate(&pool, ctx.staff, initiate_req(&ctx), &bus).await.unwrap();

        let mine = list_my_attestations(&pool, ctx.target).await.unwrap();
        assert_eq!(mine.len(), 1);
        assert_eq!(mine[0].user_id, i32::from(ctx.target));
    }

    // ── Adversarial tests ─────────────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_staff_sign_others_attestation(pool: PgPool) {
        let ctx = setup(&pool).await;
        let bus = EventBus::new();

        let attest = run_initiate(&pool, ctx.staff, initiate_req(&ctx), &bus).await.unwrap();

        // Forbidden is checked before signature verification, so the bytes
        // here never need to verify — but the struct still needs the field.
        let mut tx = RlsTransaction::begin(&pool, i32::from(ctx.admin)).await.unwrap();
        let err = staff_sign(
            &mut tx,
            &pool,
            attest.id,
            ctx.admin,
            StaffSignAttestationRequest {
                staff_signature:   "forged".to_owned(),
                verifying_key_hex: "00".repeat(32),
                photo_hash:        None,
                location_confirmed:     true,
                user_present_confirmed: true,
            },
            &bus,
        )
        .await
        .unwrap_err();

        assert!(matches!(err, DomainError::Forbidden));
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_reviewer_cannot_sign_unassigned_attestation(pool: PgPool) {
        let ctx = setup(&pool).await;
        let bus = EventBus::new();

        let attest = run_initiate(&pool, ctx.staff, initiate_req(&ctx), &bus).await.unwrap();
        run_staff_sign(&pool, &ctx, attest.id).await;

        // staff user is not a reviewer — Forbidden short-circuits before sig verify.
        let mut tx = RlsTransaction::begin(&pool, i32::from(ctx.staff)).await.unwrap();
        let err = reviewer_sign(
            &mut tx,
            &pool,
            attest.id,
            ctx.staff,
            ReviewerSignAttestationRequest {
                signature:              "bad".to_owned(),
                verifying_key_hex:      "00".repeat(32),
                evidence_hash_reviewed: "bad".to_owned(),
            },
            &bus,
        )
        .await
        .unwrap_err();

        assert!(matches!(err, DomainError::Forbidden));
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_reject_without_reviewer_assignment(pool: PgPool) {
        let ctx = setup(&pool).await;
        let bus = EventBus::new();

        let attest = run_initiate(&pool, ctx.staff, initiate_req(&ctx), &bus).await.unwrap();
        run_staff_sign(&pool, &ctx, attest.id).await;

        // admin is not an assigned reviewer.
        let mut tx = RlsTransaction::begin(&pool, i32::from(ctx.admin)).await.unwrap();
        let err = reject_attestation(
            &mut tx,
            &pool,
            attest.id,
            ctx.admin,
            RejectAttestationRequest { rejection_reason: "unauthorised".to_owned() },
            &bus,
        )
        .await
        .unwrap_err();

        assert!(matches!(err, DomainError::Forbidden));
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_initiate_for_non_presence_confirmed_user(pool: PgPool) {
        let ctx = setup(&pool).await;
        let bus = EventBus::new();

        // Try to attest the admin (whose status is 'registered').
        let err = run_initiate(
            &pool,
            ctx.staff,
            InitiateAttestationRequest {
                visit_id:              ctx.visit_id,
                user_id:               i32::from(ctx.admin),
                presence_threshold_id: ctx.threshold_id,
                photo_hash:            None,
                photo_storage_uri:     None,
            },
            &bus,
        )
        .await
        .unwrap_err();

        assert!(
            matches!(err, DomainError::InvalidInput(_)),
            "expected InvalidInput, got: {err:?}"
        );
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_double_sign_reviewer(pool: PgPool) {
        let ctx = setup(&pool).await;
        let bus = EventBus::new();

        let attest = run_initiate(&pool, ctx.staff, initiate_req(&ctx), &bus).await.unwrap();
        run_staff_sign(&pool, &ctx, attest.id).await;

        // First sign succeeds.
        run_reviewer_sign(&pool, attest.id, ctx.reviewer_1).await;

        // Second sign by the same reviewer must hit the DB ON CONFLICT path.
        // Signature must verify so we get past Ed25519 check and reach the insert.
        let mut conn = pool.acquire().await.unwrap();
        let attest_now = repository::get_attestation_by_id(&mut conn, attest.id)
            .await.unwrap().unwrap();
        drop(conn);
        let payload = attestation_payload(&attest_now);
        let kp = Ed25519KeyPair::generate();
        let (vk, sig) = signed_pair(&payload, &kp);
        let mut tx = RlsTransaction::begin(&pool, i32::from(ctx.reviewer_1)).await.unwrap();
        let err = reviewer_sign(
            &mut tx,
            &pool,
            attest.id,
            ctx.reviewer_1,
            ReviewerSignAttestationRequest {
                signature:              sig,
                verifying_key_hex:      vk,
                evidence_hash_reviewed: "double-hash".to_owned(),
            },
            &bus,
        )
        .await
        .unwrap_err();

        assert!(
            matches!(err, DomainError::Conflict(_)),
            "double-sign must be Conflict, got: {err:?}"
        );
    }

    // ── Hardening Section 1c — Ed25519 attestation signing tests ─────────────

    /// Walk an attestation up to `co_sign_pending` and return the row + the
    /// canonical payload. Used by the Ed25519 reviewer-sign tests.
    async fn ready_for_reviewer_sign(pool: &PgPool, ctx: &Ctx)
        -> (VisitAttestationRow, String)
    {
        let bus = EventBus::new();
        let attest = run_initiate(pool, ctx.staff, initiate_req(ctx), &bus)
            .await.unwrap();
        run_staff_sign(pool, ctx, attest.id).await;
        let mut conn = pool.acquire().await.unwrap();
        let attest_now = repository::get_attestation_by_id(&mut conn, attest.id)
            .await.unwrap().unwrap();
        let payload = attestation_payload(&attest_now);
        (attest_now, payload)
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn reviewer_sign_verifies_ed25519_before_storing(pool: PgPool) {
        let ctx = setup(&pool).await;
        let (attest_now, payload) = ready_for_reviewer_sign(&pool, &ctx).await;
        let bus = EventBus::new();

        let kp = Ed25519KeyPair::generate();
        let (vk, sig) = signed_pair(&payload, &kp);

        with_rls_tx!(&pool, ctx.reviewer_1, |tx| {
            reviewer_sign(
                &mut tx, &pool, attest_now.id, ctx.reviewer_1,
                ReviewerSignAttestationRequest {
                    signature:              sig.clone(),
                    verifying_key_hex:      vk.clone(),
                    evidence_hash_reviewed: "evidence".to_owned(),
                },
                &bus,
            ).await.expect("valid Ed25519 sign must succeed");
        });

        // Stored as verifying_key_hex:signature_hex in visit_signatures.
        let stored: String = sqlx::query_scalar(
            "SELECT signature FROM visit_signatures \
             WHERE visit_id = $1 AND reviewer_id = $2"
        )
        .bind(attest_now.visit_id)
        .bind(i32::from(ctx.reviewer_1))
        .fetch_one(&pool).await.unwrap();
        assert_eq!(stored, format!("{vk}:{sig}"),
            "stored value must be verifying_key_hex:signature_hex");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn reviewer_sign_rejects_invalid_signature(pool: PgPool) {
        let ctx = setup(&pool).await;
        let (attest_now, payload) = ready_for_reviewer_sign(&pool, &ctx).await;
        let bus = EventBus::new();

        let kp = Ed25519KeyPair::generate();
        let (vk, mut sig) = signed_pair(&payload, &kp);
        // Flip one hex digit — signature now does not verify against payload.
        let last = sig.pop().unwrap();
        let flipped = if last == '0' { '1' } else { '0' };
        sig.push(flipped);

        let mut tx = RlsTransaction::begin(&pool, i32::from(ctx.reviewer_1)).await.unwrap();
        let err = reviewer_sign(
            &mut tx, &pool, attest_now.id, ctx.reviewer_1,
            ReviewerSignAttestationRequest {
                signature:              sig,
                verifying_key_hex:      vk,
                evidence_hash_reviewed: "evidence".to_owned(),
            },
            &bus,
        ).await.unwrap_err();

        assert!(matches!(err, DomainError::InvalidInput(_)),
            "tampered signature must be InvalidInput, got: {err:?}");

        // Crucially, no row was written — verification happens before insert.
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM visit_signatures \
             WHERE visit_id = $1 AND reviewer_id = $2"
        )
        .bind(attest_now.visit_id)
        .bind(i32::from(ctx.reviewer_1))
        .fetch_one(&pool).await.unwrap();
        assert_eq!(count, 0, "failed sig verify must not write to visit_signatures");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn reviewer_sign_rejects_wrong_key(pool: PgPool) {
        let ctx = setup(&pool).await;
        let (attest_now, payload) = ready_for_reviewer_sign(&pool, &ctx).await;
        let bus = EventBus::new();

        let kp_a = Ed25519KeyPair::generate();
        let kp_b = Ed25519KeyPair::generate();
        let sig_a = kp_a.sign(payload.as_bytes());

        let mut tx = RlsTransaction::begin(&pool, i32::from(ctx.reviewer_1)).await.unwrap();
        let err = reviewer_sign(
            &mut tx, &pool, attest_now.id, ctx.reviewer_1,
            ReviewerSignAttestationRequest {
                signature:              sig_a,
                verifying_key_hex:      kp_b.verifying_key_hex(),
                evidence_hash_reviewed: "evidence".to_owned(),
            },
            &bus,
        ).await.unwrap_err();

        assert!(matches!(err, DomainError::InvalidInput(_)),
            "signature/key mismatch must be InvalidInput, got: {err:?}");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn both_reviewers_sign_passes_aggregated_verification(pool: PgPool) {
        let ctx = setup(&pool).await;
        let (attest_now, payload) = ready_for_reviewer_sign(&pool, &ctx).await;
        let bus = EventBus::new();

        // Distinct keys per reviewer — covers the aggregated path.
        let kp1 = Ed25519KeyPair::generate();
        let kp2 = Ed25519KeyPair::generate();
        let (vk1, sig1) = signed_pair(&payload, &kp1);
        let (vk2, sig2) = signed_pair(&payload, &kp2);

        with_rls_tx!(&pool, ctx.reviewer_1, |tx| {
            reviewer_sign(
                &mut tx, &pool, attest_now.id, ctx.reviewer_1,
                ReviewerSignAttestationRequest {
                    signature: sig1, verifying_key_hex: vk1,
                    evidence_hash_reviewed: "evidence-1".to_owned(),
                },
                &bus,
            ).await.unwrap();
        });

        let approved = with_rls_tx!(&pool, ctx.reviewer_2, |tx| {
            reviewer_sign(
                &mut tx, &pool, attest_now.id, ctx.reviewer_2,
                ReviewerSignAttestationRequest {
                    signature: sig2, verifying_key_hex: vk2,
                    evidence_hash_reviewed: "evidence-2".to_owned(),
                },
                &bus,
            ).await.expect("aggregated verify must pass with two valid sigs")
        });

        assert_eq!(approved.status, "approved");

        let user_status: String = sqlx::query_scalar(
            "SELECT verification_status FROM users WHERE id = $1"
        )
        .bind(i32::from(ctx.target))
        .fetch_one(&pool).await.unwrap();
        assert_eq!(user_status, "attested");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn payload_is_deterministic(pool: PgPool) {
        let ctx = setup(&pool).await;
        let bus = EventBus::new();
        let attest = run_initiate(&pool, ctx.staff, initiate_req(&ctx), &bus)
            .await.unwrap();

        let p1 = attestation_payload(&attest);
        let p2 = attestation_payload(&attest);
        assert_eq!(p1, p2, "payload must be byte-for-byte identical for the same row");
        assert!(p1.ends_with("|BFIP_ATTESTATION_V1"),
            "payload must end with the BFIP version tag");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn parse_signature_record_round_trips(_pool: PgPool) {
        let kp = Ed25519KeyPair::generate();
        let vk = kp.verifying_key_hex();
        let sig = kp.sign(b"any payload");

        let stored = encode_signature_record(&vk, &sig);
        let (parsed_vk, parsed_sig) = parse_signature_record(&stored)
            .expect("well-formed record must parse");
        assert_eq!(parsed_vk,  vk);
        assert_eq!(parsed_sig, sig);

        // Malformed inputs error rather than silently accepting.
        assert!(parse_signature_record("no-colon-here").is_err());
        assert!(parse_signature_record(":only-sig").is_err());
        assert!(parse_signature_record("only-key:").is_err());
    }
}
