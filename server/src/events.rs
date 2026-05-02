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

        // Audit write already done inside service::ask_dorotka.
        DomainEvent::DorotkaQueried { .. } => {}
    }
}
