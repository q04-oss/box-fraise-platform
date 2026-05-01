use box_fraise_domain::{audit, events::DomainEvent, types::UserId};
use sqlx::PgPool;

/// Consume one domain event and write durable side-effects:
/// - audit trail row for every event kind
/// - push notification to key owner when their OTPK supply is exhausted
pub async fn handle(pool: &PgPool, http: &reqwest::Client, event: DomainEvent) {
    match event {
        DomainEvent::UserRegistered { user_id, email: _ } => {
            audit::write(
                pool,
                Some(i32::from(user_id)),
                None,
                "user.registered",
                serde_json::Value::Null,
                None,
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
                None,
            )
            .await;
        }

        DomainEvent::MessageSent { message_id, sender_id, recipient_id } => {
            audit::write(
                pool,
                Some(i32::from(sender_id)),
                None,
                "message.sent",
                serde_json::json!({
                    "message_id":   i32::from(message_id),
                    "recipient_id": i32::from(recipient_id),
                }),
                None,
            )
            .await;
        }

        DomainEvent::KeyBundleRegistered { user_id } => {
            audit::write(
                pool,
                Some(i32::from(user_id)),
                None,
                "keys.bundle_registered",
                serde_json::Value::Null,
                None,
            )
            .await;
        }

        DomainEvent::KeyBundleDepleted { user_id } => {
            audit::write(
                pool,
                Some(i32::from(user_id)),
                None,
                "keys.bundle_depleted",
                serde_json::Value::Null,
                None,
            )
            .await;
            notify_prekey_upload(pool, http, user_id).await;
        }
    }
}

async fn notify_prekey_upload(pool: &PgPool, http: &reqwest::Client, user_id: UserId) {
    use box_fraise_integrations::expo_push::{send, PushMessage};

    let push_token: Option<String> = sqlx::query_scalar(
        "SELECT push_token FROM users WHERE id = $1",
    )
    .bind(i32::from(user_id))
    .fetch_optional(pool)
    .await
    .unwrap_or(None)
    .flatten();

    if let Some(token) = push_token {
        if let Err(e) = send(
            http,
            PushMessage {
                to:    &token,
                title: Some("Key refresh needed"),
                body:  "Upload new pre-keys to keep your messages secure",
                ..Default::default()
            },
        )
        .await
        {
            tracing::error!(
                user_id = i32::from(user_id),
                error   = %e,
                "pre-key upload push notification failed"
            );
        }
    }
}
