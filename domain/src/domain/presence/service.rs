use chrono::{NaiveDate, Utc};
use sqlx::PgPool;

use crate::{
    audit,
    error::{AppResult, DomainError},
    event_bus::EventBus,
    events::DomainEvent,
    types::UserId,
};
use crate::domain::auth::repository as user_repo;
use crate::domain::beacons::{
    repository as beacon_repo,
    service::derive_witness_hmac,
};
use super::{
    repository,
    types::{
        PresenceEventSummary, PresenceStatusResponse,
        PresenceThresholdRow, RecordBeaconDwellRequest, RecordNfcTapRequest,
    },
};

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Constant-time comparison of two byte slices — prevents timing oracle on HMAC tags.
fn hmac_eq(a: &str, b: &str) -> bool {
    let ab = a.as_bytes();
    let bb = b.as_bytes();
    if ab.len() != bb.len() {
        return false;
    }
    ab.iter().zip(bb.iter()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Build a PresenceStatusResponse from a threshold row + its qualifying events.
async fn build_status_response(
    pool:        &PgPool,
    user_id:     i32,
    business_id: i32,
) -> AppResult<PresenceStatusResponse> {
    let threshold = repository::get_threshold_by_user(pool, user_id).await?;
    status_from_threshold(pool, user_id, business_id, threshold.as_ref()).await
}

async fn status_from_threshold(
    pool:        &PgPool,
    user_id:     i32,
    business_id: i32,
    threshold:   Option<&PresenceThresholdRow>,
) -> AppResult<PresenceStatusResponse> {
    let (event_count, days_count, threshold_met, threshold_met_at, biz_id, qe) =
        if let Some(t) = threshold {
            let raw = repository::get_qualifying_events(pool, t.id).await?;
            let summaries = raw.into_iter().map(|e| PresenceEventSummary {
                id:               e.id,
                event_type:       e.event_type,
                calendar_date:    e.calendar_date.format("%Y-%m-%d").to_string(),
                is_qualifying:    e.is_qualifying,
                rejection_reason: e.rejection_reason,
                occurred_at:      e.occurred_at,
            }).collect();
            (t.event_count, t.days_count, t.threshold_met_at.is_some(), t.threshold_met_at, t.business_id, summaries)
        } else {
            (0, 0, false, None, business_id, vec![])
        };

    Ok(PresenceStatusResponse {
        user_id,
        business_id: biz_id,
        event_count,
        days_count,
        threshold_met,
        threshold_met_at,
        qualifying_events: qe,
    })
}

/// Count qualifying events for a user at a business on a specific calendar date.
/// Used to enforce the separate-day rule (days_count only increments for new dates).
async fn qualifying_events_on_date(
    pool:        &PgPool,
    user_id:     i32,
    business_id: i32,
    date:        NaiveDate,
) -> AppResult<i64> {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM presence_events \
         WHERE user_id = $1 AND business_id = $2 \
           AND calendar_date = $3 AND is_qualifying = true"
    )
    .bind(user_id)
    .bind(business_id)
    .bind(date)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

/// Core threshold advancement — shared by both beacon dwell and NFC tap.
/// Creates the threshold if it doesn't exist, increments event_count,
/// increments days_count only for new calendar dates, and triggers the
/// presence_confirmed status change when threshold is first met.
async fn advance_threshold(
    pool:          &PgPool,
    user_id:       i32,
    business_id:   i32,
    event_id:      i32,
    calendar_date: NaiveDate,
    bus:           &EventBus,
) -> AppResult<PresenceThresholdRow> {
    // Check BEFORE creating threshold so we don't count the event we haven't written yet.
    let already_qualifying_today =
        qualifying_events_on_date(pool, user_id, business_id, calendar_date).await?;

    let threshold = repository::get_or_create_threshold(pool, user_id, business_id).await?;
    let was_already_met = threshold.threshold_met_at.is_some();

    let new_event_count = threshold.event_count + 1;
    // Only advance days_count when this is the first qualifying event today.
    // The event we just created is already in the DB when advance_threshold is called.
    // already_qualifying_today counts it — if == 1, this is the first today.
    let is_first_event_today = already_qualifying_today <= 1;
    let new_days_count = if is_first_event_today {
        threshold.days_count + 1
    } else {
        threshold.days_count
    };

    // Detect first-time threshold completion.
    let threshold_met_at = if !was_already_met && new_event_count >= 3 && new_days_count >= 3 {
        Some(Utc::now())
    } else {
        threshold.threshold_met_at
    };

    let updated = repository::update_threshold(
        pool, threshold.id, new_event_count, new_days_count,
        Some(Utc::now()), threshold_met_at,
    ).await?;

    repository::record_qualifying_event(pool, updated.id, event_id).await?;

    // Newly met — upgrade user status and write verification events.
    if threshold_met_at.is_some() && !was_already_met {
        let _ = sqlx::query(
            "UPDATE users SET verification_status = 'presence_confirmed' WHERE id = $1"
        )
        .bind(user_id)
        .execute(pool)
        .await;

        // verification_event: presence threshold met
        if let Err(e) = sqlx::query(
            "INSERT INTO verification_events \
             (user_id, event_type, reference_type, reference_id, actor_id, metadata) \
             VALUES ($1, 'presence_threshold_met', 'presence_threshold', $2, $1, $3)"
        )
        .bind(user_id)
        .bind(updated.id)
        .bind(serde_json::json!({
            "event_count": new_event_count,
            "days_count":  new_days_count,
            "business_id": business_id,
        }))
        .execute(pool)
        .await
        {
            tracing::error!(error = %e, "verification_events (presence_threshold_met) insert failed");
        }

        // verification_event: status_changed (no reference_type — user-level status transition)
        if let Err(e) = sqlx::query(
            "INSERT INTO verification_events \
             (user_id, event_type, from_status, to_status, actor_id, metadata) \
             VALUES ($1, 'status_changed', 'identity_confirmed', 'presence_confirmed', $1, $2)"
        )
        .bind(user_id)
        .bind(serde_json::json!({ "business_id": business_id }))
        .execute(pool)
        .await
        {
            tracing::error!(error = %e, "verification_events (status_changed) insert failed");
        }

        bus.publish(DomainEvent::PresenceThresholdMet {
            user_id,
            business_id,
        });
    }

    // verification_event: presence_event_recorded (every qualifying event)
    // reference_type = 'presence_event' is in the allowed CHECK list.
    if let Err(e) = sqlx::query(
        "INSERT INTO verification_events \
         (user_id, event_type, reference_type, reference_id, actor_id, metadata) \
         VALUES ($1, 'presence_event_recorded', 'presence_event', $2, $1, $3)"
    )
    .bind(user_id)
    .bind(event_id)
    .bind(serde_json::json!({
        "calendar_date": calendar_date.format("%Y-%m-%d").to_string(),
        "business_id":   business_id,
    }))
    .execute(pool)
    .await
    {
        tracing::error!(error = %e, "verification_events (presence_event_recorded) insert failed");
    }

    Ok(updated)
}

// ── Commands ──────────────────────────────────────────────────────────────────

/// Record a BLE beacon dwell event.
///
/// Validates HMAC, RSSI, and minimum dwell time per BFIP Section 5 before
/// advancing the user's presence threshold. Returns the current threshold state.
pub async fn record_beacon_dwell(
    pool:    &PgPool,
    user_id: UserId,
    req:     RecordBeaconDwellRequest,
    bus:     &EventBus,
) -> AppResult<PresenceStatusResponse> {
    let uid = i32::from(user_id);

    // 1. Validate user.
    let user = user_repo::find_by_id(pool, user_id).await?.ok_or(DomainError::Unauthorized)?;
    if user.is_banned { return Err(DomainError::Forbidden); }
    if matches!(user.verification_status.as_str(), "attested" | "cleared") {
        return Err(DomainError::Conflict("presence verification already complete".into()));
    }

    // 2. Load beacon — must be active and belong to the stated business.
    let beacon = beacon_repo::get_beacon_by_id(pool, req.beacon_id).await?
        .ok_or(DomainError::NotFound)?;
    if beacon.business_id != Some(req.business_id) {
        return Err(DomainError::invalid_input("beacon does not belong to this business"));
    }

    let calendar_date = req.started_at.date_naive();

    // Shared non-qualifying event helper.
    let record_rejected = |reason: &'static str| {
        repository::create_presence_event(
            pool, uid, req.business_id, Some(req.beacon_id), None, None,
            "beacon_dwell", Some(req.rssi), Some(beacon.minimum_rssi_threshold),
            Some(req.started_at), Some(req.ended_at), Some(req.dwell_minutes),
            false, Some(reason),
            req.app_attest_assertion.as_deref(),
            Some(req.beacon_witness_hmac.as_str()),
            req.device_identifier.as_deref(),
            calendar_date,
        )
    };

    // 3. Validate beacon_witness_hmac.
    let expected = derive_witness_hmac(&beacon.secret_key, req.business_id, calendar_date, uid);
    if !hmac_eq(&expected, &req.beacon_witness_hmac) {
        let _ = record_rejected("invalid_beacon_witness_hmac").await;
        bus.publish(DomainEvent::PresenceEventRecorded {
            user_id: uid, event_type: "beacon_dwell".into(), is_qualifying: false,
        });
        return build_status_response(pool, uid, req.business_id).await;
    }

    // 4. Check RSSI.
    if req.rssi < beacon.minimum_rssi_threshold {
        let _ = record_rejected("rssi_below_threshold").await;
        bus.publish(DomainEvent::PresenceEventRecorded {
            user_id: uid, event_type: "beacon_dwell".into(), is_qualifying: false,
        });
        return build_status_response(pool, uid, req.business_id).await;
    }

    // 5. Check minimum dwell time (BFIP Section 5 requires >= 15 minutes).
    if req.dwell_minutes < 15 {
        let _ = record_rejected("insufficient_dwell_time").await;
        bus.publish(DomainEvent::PresenceEventRecorded {
            user_id: uid, event_type: "beacon_dwell".into(), is_qualifying: false,
        });
        return build_status_response(pool, uid, req.business_id).await;
    }

    // 6. Create presence session.
    let session = repository::create_presence_session(
        pool, uid, req.business_id, Some(req.beacon_id),
        req.device_identifier.as_deref(),
        req.started_at, req.ended_at, Some(req.dwell_minutes),
    ).await?;

    // 7. Create qualifying presence event.
    let event = repository::create_presence_event(
        pool, uid, req.business_id, Some(req.beacon_id), Some(session.id), None,
        "beacon_dwell", Some(req.rssi), Some(beacon.minimum_rssi_threshold),
        Some(req.started_at), Some(req.ended_at), Some(req.dwell_minutes),
        true, None,
        req.app_attest_assertion.as_deref(),
        Some(req.beacon_witness_hmac.as_str()),
        req.device_identifier.as_deref(),
        calendar_date,
    ).await?;

    // 8–13. Advance threshold, check completion, write verification events.
    advance_threshold(pool, uid, req.business_id, event.id, calendar_date, bus).await?;

    // 14. Audit event.
    audit::write(
        pool, Some(uid), None, "presence.beacon_dwell",
        serde_json::json!({
            "beacon_id":    req.beacon_id,
            "business_id":  req.business_id,
            "rssi":         req.rssi,
            "dwell_minutes": req.dwell_minutes,
        }),
    ).await;

    bus.publish(DomainEvent::PresenceEventRecorded {
        user_id: uid, event_type: "beacon_dwell".into(), is_qualifying: true,
    });

    // 15. Return current status.
    build_status_response(pool, uid, req.business_id).await
}

/// Record an NFC tap of a visit box.
///
/// Validates HMAC and single-use box constraints per BFIP Section 5.
pub async fn record_nfc_tap(
    pool:    &PgPool,
    user_id: UserId,
    req:     RecordNfcTapRequest,
    bus:     &EventBus,
) -> AppResult<PresenceStatusResponse> {
    let uid = i32::from(user_id);

    // 1. Validate user.
    let user = user_repo::find_by_id(pool, user_id).await?.ok_or(DomainError::Unauthorized)?;
    if user.is_banned { return Err(DomainError::Forbidden); }
    if matches!(user.verification_status.as_str(), "attested" | "cleared") {
        return Err(DomainError::Conflict("presence verification already complete".into()));
    }

    // 2. Load visit box — must be activated, not expired, not already tapped.
    let box_row: Option<(i32, Option<chrono::DateTime<Utc>>, Option<chrono::DateTime<Utc>>, Option<i32>)> =
        sqlx::query_as(
            "SELECT id, activated_at, expires_at, tapped_by_user_id \
             FROM visit_boxes WHERE id = $1"
        )
        .bind(req.box_id)
        .fetch_optional(pool)
        .await
        .map_err(DomainError::Db)?;

    let (_, activated_at, expires_at, tapped_by) = box_row.ok_or(DomainError::NotFound)?;

    if activated_at.is_none() {
        return Err(DomainError::invalid_input("visit box has not been activated"));
    }
    if expires_at.map(|e| e < Utc::now()).unwrap_or(false) {
        return Err(DomainError::invalid_input("visit box delivery window has expired"));
    }
    if tapped_by.is_some() {
        return Err(DomainError::Conflict("visit box already tapped".into()));
    }

    let calendar_date = Utc::now().date_naive();

    // 3. Validate beacon_witness_hmac using the first active beacon for this business.
    let beacon_opt = beacon_repo::get_beacons_by_business(pool, req.business_id).await?
        .into_iter().next();

    if let Some(beacon_summary) = beacon_opt {
        // Fetch full row with secret_key for HMAC validation.
        if let Some(beacon) = beacon_repo::get_beacon_by_id(pool, beacon_summary.id).await? {
            let expected = derive_witness_hmac(&beacon.secret_key, req.business_id, calendar_date, uid);
            if !hmac_eq(&expected, &req.beacon_witness_hmac) {
                let _ = repository::create_presence_event(
                    pool, uid, req.business_id, Some(beacon_summary.id), None, Some(req.box_id),
                    "nfc_tap", None, None, None, None, None,
                    false, Some("invalid_beacon_witness_hmac"),
                    req.app_attest_assertion.as_deref(),
                    Some(req.beacon_witness_hmac.as_str()),
                    req.device_identifier.as_deref(),
                    calendar_date,
                ).await;
                bus.publish(DomainEvent::PresenceEventRecorded {
                    user_id: uid, event_type: "nfc_tap".into(), is_qualifying: false,
                });
                return build_status_response(pool, uid, req.business_id).await;
            }
        }
    }

    // 4. Mark box as tapped (single-use).
    sqlx::query(
        "UPDATE visit_boxes SET tapped_by_user_id = $1, tapped_at = now() WHERE id = $2"
    )
    .bind(uid)
    .bind(req.box_id)
    .execute(pool)
    .await
    .map_err(DomainError::Db)?;

    // 5. Create qualifying presence event.
    let event = repository::create_presence_event(
        pool, uid, req.business_id, None, None, Some(req.box_id),
        "nfc_tap", None, None, None, None, None,
        true, None,
        req.app_attest_assertion.as_deref(),
        Some(req.beacon_witness_hmac.as_str()),
        req.device_identifier.as_deref(),
        calendar_date,
    ).await?;

    // 6. Advance threshold.
    advance_threshold(pool, uid, req.business_id, event.id, calendar_date, bus).await?;

    // 7. Audit event.
    audit::write(
        pool, Some(uid), None, "presence.nfc_tap",
        serde_json::json!({ "box_id": req.box_id, "business_id": req.business_id }),
    ).await;

    bus.publish(DomainEvent::PresenceEventRecorded {
        user_id: uid, event_type: "nfc_tap".into(), is_qualifying: true,
    });

    build_status_response(pool, uid, req.business_id).await
}

// ── Queries ───────────────────────────────────────────────────────────────────

/// Return the current presence threshold state for a user.
pub async fn get_presence_status(
    pool:    &PgPool,
    user_id: UserId,
) -> AppResult<PresenceStatusResponse> {
    let uid       = i32::from(user_id);
    let threshold = repository::get_threshold_by_user(pool, uid).await?;

    let business_id = threshold.as_ref().map(|t| t.business_id).unwrap_or(0);
    status_from_threshold(pool, uid, business_id, threshold.as_ref()).await
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::beacons::{repository as beacon_repo, service::derive_witness_hmac},
        event_bus::EventBus,
        types::UserId,
    };
    use chrono::{DateTime, Duration, Utc};
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

    async fn create_attested_user(pool: &PgPool, email: &str) -> UserId {
        let (id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified, verification_status) \
             VALUES ($1, true, 'attested') RETURNING id"
        )
        .bind(email).fetch_one(pool).await.unwrap();
        UserId::from(id)
    }

    /// Returns (business_id, location_id, beacon_id, secret_key).
    async fn create_business_and_beacon(pool: &PgPool, owner_uid: i32) -> (i32, i32, i32, String) {
        use fake::{Fake, faker::company::en::CompanyName, faker::address::en::StreetName};

        let (loc_id,): (i32,) = sqlx::query_as(
            "INSERT INTO locations (name, location_type, address, timezone) \
             VALUES ($1, 'partner_business', $2, 'America/Edmonton') RETURNING id"
        )
        .bind(CompanyName().fake::<String>())
        .bind(format!("{} {}", (100..=999u32).fake::<u32>(), StreetName().fake::<String>()))
        .fetch_one(pool).await.unwrap();

        let (biz_id,): (i32,) = sqlx::query_as(
            "INSERT INTO businesses (location_id, primary_holder_id, name, verification_status) \
             VALUES ($1, $2, $3, 'active') RETURNING id"
        )
        .bind(loc_id)
        .bind(owner_uid)
        .bind(CompanyName().fake::<String>())
        .fetch_one(pool).await.unwrap();

        let secret_key = "test-beacon-secret-key-for-presence";
        let (beacon_id,): (i32,) = sqlx::query_as(
            "INSERT INTO beacons (location_id, business_id, secret_key, minimum_rssi_threshold) \
             VALUES ($1, $2, $3, -70) RETURNING id"
        )
        .bind(loc_id).bind(biz_id).bind(secret_key)
        .fetch_one(pool).await.unwrap();

        (biz_id, loc_id, beacon_id, secret_key.to_owned())
    }

    fn dwell_req(
        beacon_id:    i32,
        business_id:  i32,
        rssi:         i32,
        dwell_mins:   i32,
        witness_hmac: String,
        started_at:   DateTime<Utc>,
    ) -> RecordBeaconDwellRequest {
        RecordBeaconDwellRequest {
            beacon_id,
            business_id,
            rssi,
            dwell_minutes: dwell_mins,
            beacon_witness_hmac: witness_hmac,
            app_attest_assertion: None,
            device_identifier: None,
            started_at,
            ended_at: started_at + Duration::minutes(dwell_mins as i64),
        }
    }

    // ── Tests 1–6: record_beacon_dwell ────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn record_beacon_dwell_succeeds_and_advances_threshold(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let uid = create_identity_confirmed_user(&pool, &SafeEmail().fake::<String>()).await;
        let owner = create_attested_user(&pool, &SafeEmail().fake::<String>()).await;
        let (biz_id, _, beacon_id, secret) =
            create_business_and_beacon(&pool, i32::from(owner)).await;

        let now = Utc::now();
        let hmac = derive_witness_hmac(&secret, biz_id, now.date_naive(), i32::from(uid));
        let bus  = EventBus::new();

        let resp = record_beacon_dwell(
            &pool, uid,
            dwell_req(beacon_id, biz_id, -65, 20, hmac, now),
            &bus,
        ).await.expect("qualifying dwell must succeed");

        assert_eq!(resp.event_count, 1);
        assert_eq!(resp.days_count,  1);
        assert!(!resp.threshold_met);
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn record_beacon_dwell_rejected_for_low_rssi(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let uid   = create_identity_confirmed_user(&pool, &SafeEmail().fake::<String>()).await;
        let owner = create_attested_user(&pool, &SafeEmail().fake::<String>()).await;
        let (biz_id, _, beacon_id, secret) =
            create_business_and_beacon(&pool, i32::from(owner)).await;

        let now  = Utc::now();
        let hmac = derive_witness_hmac(&secret, biz_id, now.date_naive(), i32::from(uid));
        let bus  = EventBus::new();

        // minimum_rssi_threshold is -70 — send -80 (below threshold)
        let resp = record_beacon_dwell(
            &pool, uid,
            dwell_req(beacon_id, biz_id, -80, 20, hmac, now),
            &bus,
        ).await.expect("call must succeed");

        assert_eq!(resp.event_count, 0, "threshold must not advance for rejected event");

        let event: Option<(bool, Option<String>)> = sqlx::query_as(
            "SELECT is_qualifying, rejection_reason FROM presence_events \
             WHERE user_id = $1 ORDER BY occurred_at DESC LIMIT 1"
        )
        .bind(i32::from(uid))
        .fetch_optional(&pool).await.unwrap();
        let (is_qual, reason) = event.unwrap();
        assert!(!is_qual);
        assert_eq!(reason.as_deref(), Some("rssi_below_threshold"));
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn record_beacon_dwell_rejected_for_insufficient_dwell(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let uid   = create_identity_confirmed_user(&pool, &SafeEmail().fake::<String>()).await;
        let owner = create_attested_user(&pool, &SafeEmail().fake::<String>()).await;
        let (biz_id, _, beacon_id, secret) =
            create_business_and_beacon(&pool, i32::from(owner)).await;

        let now  = Utc::now();
        let hmac = derive_witness_hmac(&secret, biz_id, now.date_naive(), i32::from(uid));
        let bus  = EventBus::new();

        let resp = record_beacon_dwell(
            &pool, uid,
            dwell_req(beacon_id, biz_id, -65, 10, hmac, now),  // 10 mins < 15 min minimum
            &bus,
        ).await.unwrap();

        assert_eq!(resp.event_count, 0);

        let reason: Option<String> = sqlx::query_scalar(
            "SELECT rejection_reason FROM presence_events \
             WHERE user_id = $1 ORDER BY occurred_at DESC LIMIT 1"
        )
        .bind(i32::from(uid))
        .fetch_optional(&pool).await.unwrap().flatten();
        assert_eq!(reason.as_deref(), Some("insufficient_dwell_time"));
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn record_beacon_dwell_rejected_for_invalid_witness_hmac(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let uid   = create_identity_confirmed_user(&pool, &SafeEmail().fake::<String>()).await;
        let owner = create_attested_user(&pool, &SafeEmail().fake::<String>()).await;
        let (biz_id, _, beacon_id, _secret) =
            create_business_and_beacon(&pool, i32::from(owner)).await;

        let now  = Utc::now();
        let bad_hmac = "deadbeef".repeat(8); // wrong HMAC — 64 chars, all wrong
        let bus  = EventBus::new();

        let resp = record_beacon_dwell(
            &pool, uid,
            dwell_req(beacon_id, biz_id, -65, 20, bad_hmac, now),
            &bus,
        ).await.unwrap();

        assert_eq!(resp.event_count, 0);

        let reason: Option<String> = sqlx::query_scalar(
            "SELECT rejection_reason FROM presence_events \
             WHERE user_id = $1 ORDER BY occurred_at DESC LIMIT 1"
        )
        .bind(i32::from(uid))
        .fetch_optional(&pool).await.unwrap().flatten();
        assert_eq!(reason.as_deref(), Some("invalid_beacon_witness_hmac"));
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn same_calendar_date_does_not_advance_days_count(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let uid   = create_identity_confirmed_user(&pool, &SafeEmail().fake::<String>()).await;
        let owner = create_attested_user(&pool, &SafeEmail().fake::<String>()).await;
        let (biz_id, _, beacon_id, secret) =
            create_business_and_beacon(&pool, i32::from(owner)).await;

        let day = Utc::now();
        let bus = EventBus::new();

        // First dwell on day.
        let hmac1 = derive_witness_hmac(&secret, biz_id, day.date_naive(), i32::from(uid));
        record_beacon_dwell(&pool, uid, dwell_req(beacon_id, biz_id, -65, 20, hmac1, day), &bus)
            .await.unwrap();

        // Second dwell — same calendar date.
        let hmac2 = derive_witness_hmac(&secret, biz_id, day.date_naive(), i32::from(uid));
        let resp = record_beacon_dwell(
            &pool, uid,
            dwell_req(beacon_id, biz_id, -65, 20, hmac2, day + Duration::hours(2)),
            &bus,
        ).await.unwrap();

        assert_eq!(resp.event_count, 2, "two events recorded");
        assert_eq!(resp.days_count,  1, "only one distinct day — days_count must not double-count");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn three_qualifying_events_on_three_days_sets_presence_confirmed(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let uid   = create_identity_confirmed_user(&pool, &SafeEmail().fake::<String>()).await;
        let owner = create_attested_user(&pool, &SafeEmail().fake::<String>()).await;
        let (biz_id, _, beacon_id, secret) =
            create_business_and_beacon(&pool, i32::from(owner)).await;
        let bus = EventBus::new();

        for days_offset in 0..3i64 {
            // Use distinct past dates to avoid collisions with wall-clock
            let started_at = Utc::now() - Duration::days(2 - days_offset);
            let hmac = derive_witness_hmac(
                &secret, biz_id, started_at.date_naive(), i32::from(uid),
            );
            record_beacon_dwell(
                &pool, uid,
                dwell_req(beacon_id, biz_id, -65, 20, hmac, started_at),
                &bus,
            ).await.unwrap();
        }

        let resp = get_presence_status(&pool, uid).await.unwrap();
        assert_eq!(resp.event_count, 3);
        assert_eq!(resp.days_count,  3);
        assert!(resp.threshold_met,     "threshold must be met after 3 events on 3 days");
        assert!(resp.threshold_met_at.is_some());

        let status: String = sqlx::query_scalar(
            "SELECT verification_status FROM users WHERE id = $1"
        )
        .bind(i32::from(uid))
        .fetch_one(&pool).await.unwrap();
        assert_eq!(status, "presence_confirmed");

        let ve_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM verification_events \
             WHERE user_id = $1 AND event_type = 'presence_threshold_met'"
        )
        .bind(i32::from(uid))
        .fetch_one(&pool).await.unwrap();
        assert_eq!(ve_count, 1, "presence_threshold_met verification_event must be written");

        let sc_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM verification_events \
             WHERE user_id = $1 AND event_type = 'status_changed'"
        )
        .bind(i32::from(uid))
        .fetch_one(&pool).await.unwrap();
        assert!(sc_count >= 1, "status_changed verification_event must be written");
    }

    // ── Tests 7–8: record_nfc_tap ─────────────────────────────────────────────

    async fn create_visit_box(pool: &PgPool, loc_id: i32, staff_uid: i32) -> i32 {
        let (visit_id,): (i32,) = sqlx::query_as(
            "INSERT INTO staff_visits (location_id, staff_id, visit_type, scheduled_at) \
             VALUES ($1, $2, 'delivery', now() + interval '1 hour') RETURNING id"
        )
        .bind(loc_id).bind(staff_uid)
        .fetch_one(pool).await.unwrap();

        let (box_id,): (i32,) = sqlx::query_as(
            "INSERT INTO visit_boxes (visit_id, nfc_chip_uid, activated_at, expires_at) \
             VALUES ($1, $2, now(), now() + interval '4 hours') RETURNING id"
        )
        .bind(visit_id)
        .bind(format!("CHIP-{}", uuid::Uuid::new_v4()))
        .fetch_one(pool).await.unwrap();

        box_id
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn record_nfc_tap_advances_threshold(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let uid   = create_identity_confirmed_user(&pool, &SafeEmail().fake::<String>()).await;
        let owner = create_attested_user(&pool, &SafeEmail().fake::<String>()).await;
        let (biz_id, loc_id, beacon_id, secret) =
            create_business_and_beacon(&pool, i32::from(owner)).await;

        let box_id = create_visit_box(&pool, loc_id, i32::from(owner)).await;
        let hmac   = derive_witness_hmac(&secret, biz_id, Utc::now().date_naive(), i32::from(uid));
        let bus    = EventBus::new();

        let resp = record_nfc_tap(&pool, uid, RecordNfcTapRequest {
            box_id,
            business_id:          biz_id,
            beacon_witness_hmac:  hmac,
            app_attest_assertion: None,
            device_identifier:    None,
        }, &bus).await.expect("NFC tap must succeed");

        assert_eq!(resp.event_count, 1, "threshold must advance after NFC tap");

        let tapped_at: Option<chrono::DateTime<Utc>> = sqlx::query_scalar(
            "SELECT tapped_at FROM visit_boxes WHERE id = $1"
        )
        .bind(box_id)
        .fetch_one(&pool).await.unwrap();
        assert!(tapped_at.is_some(), "box.tapped_at must be set after tap");
        let _ = (beacon_id,); // used indirectly via create_business_and_beacon
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn record_nfc_tap_rejected_for_already_tapped_box(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let uid   = create_identity_confirmed_user(&pool, &SafeEmail().fake::<String>()).await;
        let owner = create_attested_user(&pool, &SafeEmail().fake::<String>()).await;
        let (biz_id, loc_id, _beacon_id, secret) =
            create_business_and_beacon(&pool, i32::from(owner)).await;

        let box_id = create_visit_box(&pool, loc_id, i32::from(owner)).await;

        let make_req = || RecordNfcTapRequest {
            box_id,
            business_id:          biz_id,
            beacon_witness_hmac:  derive_witness_hmac(&secret, biz_id, Utc::now().date_naive(), i32::from(uid)),
            app_attest_assertion: None,
            device_identifier:    None,
        };

        // First tap succeeds.
        let bus1 = EventBus::new();
        record_nfc_tap(&pool, uid, make_req(), &bus1).await.expect("first tap must succeed");

        // Second tap on same box must fail.
        let bus2 = EventBus::new();
        let err = record_nfc_tap(&pool, uid, make_req(), &bus2)
            .await.expect_err("second tap must be rejected");
        assert!(matches!(err, DomainError::Conflict(_)),
            "expected Conflict for already-tapped box, got: {err:?}");
    }

    // ── Test 9: get_presence_status ───────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn get_presence_status_returns_current_threshold(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let uid   = create_identity_confirmed_user(&pool, &SafeEmail().fake::<String>()).await;
        let owner = create_attested_user(&pool, &SafeEmail().fake::<String>()).await;
        let (biz_id, _, beacon_id, secret) =
            create_business_and_beacon(&pool, i32::from(owner)).await;

        let now  = Utc::now();
        let hmac = derive_witness_hmac(&secret, biz_id, now.date_naive(), i32::from(uid));
        let bus  = EventBus::new();

        record_beacon_dwell(
            &pool, uid, dwell_req(beacon_id, biz_id, -65, 20, hmac, now), &bus,
        ).await.unwrap();

        let status = get_presence_status(&pool, uid).await.unwrap();
        assert_eq!(status.event_count, 1);
        assert_eq!(status.days_count,  1);
        assert!(!status.threshold_met);
        assert_eq!(status.qualifying_events.len(), 1);
    }

    // ── Adversarial tests 10–12 ───────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_record_dwell_with_fake_witness_hmac(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let uid   = create_identity_confirmed_user(&pool, &SafeEmail().fake::<String>()).await;
        let owner = create_attested_user(&pool, &SafeEmail().fake::<String>()).await;
        let (biz_id, _, beacon_id, _) =
            create_business_and_beacon(&pool, i32::from(owner)).await;

        let now      = Utc::now();
        let fake_key = "not-the-real-beacon-secret-key!";
        let fake_hmac = derive_witness_hmac(fake_key, biz_id, now.date_naive(), i32::from(uid));
        let bus = EventBus::new();

        let resp = record_beacon_dwell(
            &pool, uid,
            dwell_req(beacon_id, biz_id, -65, 20, fake_hmac, now),
            &bus,
        ).await.unwrap();

        assert_eq!(resp.event_count, 0, "fake HMAC must not advance threshold");

        let reason: Option<String> = sqlx::query_scalar(
            "SELECT rejection_reason FROM presence_events \
             WHERE user_id = $1 LIMIT 1"
        )
        .bind(i32::from(uid))
        .fetch_optional(&pool).await.unwrap().flatten();
        assert_eq!(reason.as_deref(), Some("invalid_beacon_witness_hmac"));
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_reuse_nfc_tap(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let uid   = create_identity_confirmed_user(&pool, &SafeEmail().fake::<String>()).await;
        let owner = create_attested_user(&pool, &SafeEmail().fake::<String>()).await;
        let (biz_id, loc_id, _beacon_id, secret) =
            create_business_and_beacon(&pool, i32::from(owner)).await;

        let box_id = create_visit_box(&pool, loc_id, i32::from(owner)).await;
        let bus    = EventBus::new();

        let make_req = || RecordNfcTapRequest {
            box_id,
            business_id:          biz_id,
            beacon_witness_hmac:  derive_witness_hmac(&secret, biz_id, Utc::now().date_naive(), i32::from(uid)),
            app_attest_assertion: None,
            device_identifier:    None,
        };

        record_nfc_tap(&pool, uid, make_req(), &bus).await.expect("first tap must succeed");

        let err = record_nfc_tap(&pool, uid, make_req(), &bus)
            .await.expect_err("replay tap must be rejected");
        assert!(matches!(err, DomainError::Conflict(_)));
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_record_presence_for_inactive_beacon(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let uid   = create_identity_confirmed_user(&pool, &SafeEmail().fake::<String>()).await;
        let owner = create_attested_user(&pool, &SafeEmail().fake::<String>()).await;
        let (biz_id, _, beacon_id, secret) =
            create_business_and_beacon(&pool, i32::from(owner)).await;

        // Deactivate the beacon.
        sqlx::query("UPDATE beacons SET is_active = false WHERE id = $1")
            .bind(beacon_id)
            .execute(&pool).await.unwrap();

        let now  = Utc::now();
        let hmac = derive_witness_hmac(&secret, biz_id, now.date_naive(), i32::from(uid));
        let bus  = EventBus::new();

        let err = record_beacon_dwell(
            &pool, uid,
            dwell_req(beacon_id, biz_id, -65, 20, hmac, now),
            &bus,
        ).await.expect_err("inactive beacon must be rejected");

        assert!(matches!(err, DomainError::NotFound),
            "inactive beacon must return NotFound, got: {err:?}");
    }
}
