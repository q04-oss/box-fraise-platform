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

        // Audit write already done inside service::ask_dorotka.
        DomainEvent::DorotkaQueried { .. } => {}
    }
}
