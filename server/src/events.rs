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

        DomainEvent::OrderCreated { order_id, user_id, business_id } => {
            tracing::info!(order_id, user_id, business_id, "order.created");
            audit::write(
                pool,
                Some(user_id),
                None,
                "order.created",
                serde_json::json!({
                    "order_id":    order_id,
                    "business_id": business_id,
                }),
            )
            .await;
        }

        DomainEvent::OrderCollected { order_id, user_id, box_id } => {
            tracing::info!(order_id, user_id, box_id, "order.collected");
            audit::write(
                pool,
                Some(user_id),
                None,
                "order.collected",
                serde_json::json!({
                    "order_id": order_id,
                    "box_id":   box_id,
                }),
            )
            .await;
        }

        DomainEvent::SoultokenIssued { soultoken_id, user_id, ref token_type } => {
            tracing::info!(soultoken_id, user_id, token_type, "soultoken.issued");
            audit::write(
                pool,
                Some(user_id),
                None,
                "soultoken.issued",
                serde_json::json!({
                    "soultoken_id": soultoken_id,
                    "token_type":   token_type,
                }),
            )
            .await;
        }

        DomainEvent::SoultokenRevoked { soultoken_id, user_id, ref reason } => {
            tracing::info!(soultoken_id, user_id, reason, "soultoken.revoked");
            audit::write(
                pool,
                Some(user_id),
                None,
                "soultoken.revoked",
                serde_json::json!({
                    "soultoken_id": soultoken_id,
                    "reason":       reason,
                }),
            )
            .await;
        }

        DomainEvent::SoultokenRenewed { soultoken_id, user_id } => {
            tracing::info!(soultoken_id, user_id, "soultoken.renewed");
            audit::write(
                pool,
                Some(user_id),
                None,
                "soultoken.renewed",
                serde_json::json!({ "soultoken_id": soultoken_id }),
            )
            .await;
        }

        DomainEvent::AttestationInitiated { attestation_id, user_id, visit_id } => {
            tracing::info!(attestation_id, user_id, visit_id, "attestation.initiated");
            audit::write(
                pool,
                Some(user_id),
                None,
                "attestation.initiated",
                serde_json::json!({
                    "attestation_id": attestation_id,
                    "visit_id":       visit_id,
                }),
            )
            .await;
        }

        DomainEvent::AttestationApproved { attestation_id, user_id } => {
            tracing::info!(attestation_id, user_id, "attestation.approved");
            audit::write(
                pool,
                Some(user_id),
                None,
                "attestation.approved",
                serde_json::json!({ "attestation_id": attestation_id }),
            )
            .await;
        }

        DomainEvent::AttestationRejected { attestation_id, user_id, rejection_reviewer_id } => {
            tracing::info!(attestation_id, user_id, rejection_reviewer_id, "attestation.rejected");
            audit::write(
                pool,
                Some(user_id),
                None,
                "attestation.rejected",
                serde_json::json!({
                    "attestation_id":        attestation_id,
                    "rejection_reviewer_id": rejection_reviewer_id,
                }),
            )
            .await;
        }

        DomainEvent::StaffRoleGranted { user_id, ref role } => {
            tracing::info!(user_id, role, "staff.role_granted");
            audit::write(
                pool,
                Some(user_id),
                None,
                "staff.role_granted",
                serde_json::json!({ "role": role }),
            )
            .await;
        }

        DomainEvent::VisitScheduled { visit_id, location_id } => {
            tracing::info!(visit_id, location_id, "staff.visit_scheduled");
            audit::write(
                pool,
                None,
                None,
                "staff.visit_scheduled",
                serde_json::json!({ "visit_id": visit_id, "location_id": location_id }),
            )
            .await;
        }

        DomainEvent::VisitCompleted { visit_id } => {
            tracing::info!(visit_id, "staff.visit_completed");
            audit::write(
                pool,
                None,
                None,
                "staff.visit_completed",
                serde_json::json!({ "visit_id": visit_id }),
            )
            .await;
        }

        DomainEvent::QualityAssessmentSubmitted { visit_id, business_id, overall_pass } => {
            tracing::info!(visit_id, business_id, overall_pass, "staff.quality_assessment_submitted");
            audit::write(
                pool,
                None,
                None,
                "staff.quality_assessment_submitted",
                serde_json::json!({
                    "visit_id": visit_id,
                    "business_id": business_id,
                    "overall_pass": overall_pass,
                }),
            )
            .await;
        }

        DomainEvent::BackgroundCheckInitiated { user_id, check_id, ref check_type } => {
            tracing::info!(user_id, check_id, check_type, "background_check.initiated");
            audit::write(
                pool,
                Some(user_id),
                None,
                "background_check.initiated",
                serde_json::json!({ "check_id": check_id, "check_type": check_type }),
            )
            .await;
        }

        DomainEvent::BackgroundCheckPassed { user_id, check_id, ref check_type } => {
            tracing::info!(user_id, check_id, check_type, "background_check.passed");
            audit::write(
                pool,
                Some(user_id),
                None,
                "background_check.passed",
                serde_json::json!({ "check_id": check_id, "check_type": check_type }),
            )
            .await;
        }

        DomainEvent::BackgroundCheckFailed { user_id, check_id, ref check_type } => {
            tracing::info!(user_id, check_id, check_type, "background_check.failed");
            audit::write(
                pool,
                Some(user_id),
                None,
                "background_check.failed",
                serde_json::json!({ "check_id": check_id, "check_type": check_type }),
            )
            .await;
        }

        // Audit write already done inside service::ask_dorotka.
        DomainEvent::DorotkaQueried { .. } => {}
    }
}
