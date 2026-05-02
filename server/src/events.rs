use box_fraise_domain::{audit, events::DomainEvent};
use sqlx::PgPool;

/// Consume one domain event and write durable side-effects:
/// - audit trail row for every event kind
pub async fn handle(pool: &PgPool, _http: &reqwest::Client, event: DomainEvent) {
    match event {
        DomainEvent::UserRegistered { user_id, email: _ } => {
            audit::write(
                pool,
                Some(i32::from(user_id)),
                None,
                "user.registered",
                serde_json::Value::Null,
            )
            .await;
        }

        DomainEvent::UserLoggedIn { user_id } => {
            audit::write(
                pool,
                Some(i32::from(user_id)),
                None,
                "user.login",
                serde_json::Value::Null,
            )
            .await;
        }

        DomainEvent::BusinessCreated { business_id, user_id } => {
            tracing::info!(business_id, user_id, "business.created");
            // The service writes the audit row directly at creation time.
            // This event handler also writes one to maintain consistency
            // with the event bus pattern — every DomainEvent produces
            // an audit row in the handler regardless of service writes.
            audit::write(
                pool,
                Some(user_id),
                None,
                "business.created",
                serde_json::json!({ "business_id": business_id }),
            )
            .await;
        }

        DomainEvent::BeaconCreated { beacon_id, business_id, user_id } => {
            tracing::info!(beacon_id, business_id, user_id, "beacon.created");
            // The service writes the audit row directly at creation time.
            // This event handler also writes one to maintain consistency
            // with the event bus pattern — every DomainEvent produces
            // an audit row in the handler regardless of service writes.
            audit::write(
                pool,
                Some(user_id),
                None,
                "beacon.created",
                serde_json::json!({ "beacon_id": beacon_id, "business_id": business_id }),
            )
            .await;
        }

        DomainEvent::BeaconKeyRotated { beacon_id, user_id } => {
            tracing::info!(beacon_id, user_id, "beacon.key_rotated");
            audit::write(
                pool,
                Some(user_id),
                None,
                "beacon.key_rotated",
                serde_json::json!({ "beacon_id": beacon_id }),
            )
            .await;
        }

        DomainEvent::PresenceThresholdMet { user_id, business_id } => {
            tracing::info!(user_id, business_id, "presence.threshold_met");
            audit::write(
                pool,
                Some(user_id),
                None,
                "presence.threshold_met",
                serde_json::json!({ "business_id": business_id }),
            )
            .await;
        }

        DomainEvent::PresenceEventRecorded { user_id, ref event_type, is_qualifying } => {
            if is_qualifying {
                tracing::info!(user_id, event_type, "presence.event_recorded");
            }
            audit::write(
                pool,
                Some(user_id),
                None,
                "presence.event_recorded",
                serde_json::json!({ "event_type": event_type, "is_qualifying": is_qualifying }),
            )
            .await;
        }

        DomainEvent::IdentityVerificationInitiated { user_id, credential_id } => {
            tracing::info!(user_id, credential_id, "identity.verification_initiated");
            audit::write(
                pool,
                Some(user_id),
                None,
                "identity.verification_initiated",
                serde_json::json!({ "credential_id": credential_id }),
            )
            .await;
        }

        DomainEvent::CoolingAppOpenRecorded { user_id, credential_id, days_completed } => {
            tracing::info!(user_id, credential_id, days_completed, "identity.cooling_app_open");
            audit::write(
                pool,
                Some(user_id),
                None,
                "identity.cooling_app_open",
                serde_json::json!({ "credential_id": credential_id, "days_completed": days_completed }),
            )
            .await;
        }

        DomainEvent::CoolingPeriodCompleted { user_id, credential_id } => {
            tracing::info!(user_id, credential_id, "identity.cooling_completed");
            audit::write(
                pool,
                Some(user_id),
                None,
                "identity.cooling_completed",
                serde_json::json!({ "credential_id": credential_id }),
            )
            .await;
        }

        // Audit write already done inside service::ask_dorotka.
        DomainEvent::DorotkaQueried { .. } => {}
    }
}
