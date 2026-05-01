use sqlx::PgPool;

use crate::{
    error::{DomainError, AppResult},
    event_bus::EventBus,
    events::DomainEvent,
    types::{MessageId, UserId},
};
use super::{
    repository,
    types::{ConversationSummary, MessageRow, SendMessageBody},
};

pub async fn list_conversations(
    pool:    &PgPool,
    user_id: UserId,
) -> AppResult<Vec<ConversationSummary>> {
    repository::list_conversations(pool, user_id).await
}

pub async fn archive_conversation(
    pool:    &PgPool,
    user_id: UserId,
    peer_id: UserId,
) -> AppResult<()> {
    repository::archive(pool, user_id, peer_id).await
}

pub async fn get_thread(
    pool:      &PgPool,
    user_id:   UserId,
    peer_id:   UserId,
    before_id: Option<MessageId>,
    limit:     i64,
) -> AppResult<Vec<MessageRow>> {
    repository::thread(pool, user_id, peer_id, before_id, limit).await
}

/// Validates send permission, persists the message, and fires a push
/// notification to the recipient. The push is best-effort — failure is
/// logged but never blocks the response.
pub async fn send_message(
    pool:      &PgPool,
    http:      &reqwest::Client,
    user_id:   UserId,
    body:      SendMessageBody,
    event_bus: &EventBus,
) -> AppResult<MessageRow> {
    if !repository::can_message(pool, user_id, body.recipient_id).await? {
        return Err(DomainError::Forbidden);
    }

    let message = repository::insert(
        pool,
        user_id,
        body.recipient_id,
        &body.body,
        body.encrypted.unwrap_or(false),
        body.ephemeral_key.as_deref(),
        body.sender_identity_key.as_deref(),
        body.one_time_pre_key_id,
    )
    .await?;

    event_bus.publish(DomainEvent::MessageSent {
        message_id:   message.id,
        sender_id:    user_id,
        recipient_id: body.recipient_id,
    });

    // Fire-and-forget — push failure must never fail the message send.
    let db   = pool.clone();
    let http = http.clone();
    let rid  = body.recipient_id;
    tokio::spawn(async move {
        match sqlx::query_as::<_, (Option<String>,)>(
            "SELECT push_token FROM users WHERE id = $1",
        )
        .bind(rid)
        .fetch_optional(&db)
        .await
        {
            Ok(Some((Some(token),))) => {
                use box_fraise_integrations::expo_push::{send, PushMessage};
                if let Err(e) = send(&http, PushMessage { to: &token, body: "New message", ..Default::default() }).await {
                    tracing::error!(recipient_id = i32::from(rid), error = %e, "push notification failed");
                }
            }
            Ok(_)  => {}
            Err(e) => tracing::error!(recipient_id = i32::from(rid), error = %e, "push token lookup failed"),
        }
    });

    Ok(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{event_bus::EventBus, types::UserId};
    use sqlx::PgPool;

    async fn verified_user(pool: &PgPool, email: &str) -> UserId {
        let (id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, verified) VALUES ($1, true) RETURNING id",
        )
        .bind(email)
        .fetch_one(pool)
        .await
        .unwrap();
        UserId::from(id)
    }

    fn test_body(recipient_id: UserId, text: &str) -> SendMessageBody {
        SendMessageBody {
            recipient_id,
            body: text.to_owned(),
            encrypted: None,
            ephemeral_key: None,
            sender_identity_key: None,
            one_time_pre_key_id: None,
        }
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn send_message_persists_and_returns_row(pool: PgPool) {
        let alice = verified_user(&pool, "alice@test.com").await;
        let bob   = verified_user(&pool, "bob@test.com").await;
        let http  = reqwest::Client::new();
        let bus   = EventBus::new();

        let msg = send_message(&pool, &http, alice, test_body(bob, "hello bob"), &bus).await.unwrap();
        assert_eq!(msg.sender_id, alice);
        assert_eq!(msg.recipient_id, bob);
        assert_eq!(msg.body, "hello bob");
        assert!(!msg.encrypted);
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn send_message_forbidden_when_sender_unverified(pool: PgPool) {
        let alice: UserId = {
            let (id,): (i32,) =
                sqlx::query_as("INSERT INTO users (email) VALUES ($1) RETURNING id")
                    .bind("unverified@test.com")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            UserId::from(id)
        };
        let bob  = verified_user(&pool, "bob@test.com").await;
        let http = reqwest::Client::new();
        let bus  = EventBus::new();

        let result = send_message(&pool, &http, alice, test_body(bob, "hi"), &bus).await;
        assert!(
            matches!(result, Err(DomainError::Forbidden)),
            "unverified sender must be Forbidden, got: {result:?}"
        );
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn get_thread_returns_messages_newest_first(pool: PgPool) {
        let alice = verified_user(&pool, "alice@test.com").await;
        let bob   = verified_user(&pool, "bob@test.com").await;
        let http  = reqwest::Client::new();
        let bus   = EventBus::new();

        send_message(&pool, &http, alice, test_body(bob, "first"),  &bus).await.unwrap();
        send_message(&pool, &http, alice, test_body(bob, "second"), &bus).await.unwrap();
        send_message(&pool, &http, alice, test_body(bob, "third"),  &bus).await.unwrap();

        let thread = get_thread(&pool, alice, bob, None, 50).await.unwrap();
        assert_eq!(thread.len(), 3);
        assert_eq!(thread[0].body, "third", "newest message must be first");
        assert_eq!(thread[2].body, "first");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn get_thread_returns_empty_for_no_messages(pool: PgPool) {
        let alice = verified_user(&pool, "alice@test.com").await;
        let bob   = verified_user(&pool, "bob@test.com").await;

        let thread = get_thread(&pool, alice, bob, None, 50).await.unwrap();
        assert!(thread.is_empty());
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn get_thread_pagination_with_before_id(pool: PgPool) {
        let alice = verified_user(&pool, "alice@test.com").await;
        let bob   = verified_user(&pool, "bob@test.com").await;
        let http  = reqwest::Client::new();
        let bus   = EventBus::new();

        let m1  = send_message(&pool, &http, alice, test_body(bob, "one"),   &bus).await.unwrap();
        let m2  = send_message(&pool, &http, alice, test_body(bob, "two"),   &bus).await.unwrap();
        let _m3 = send_message(&pool, &http, alice, test_body(bob, "three"), &bus).await.unwrap();

        // before_id = m2 should only return m1
        let page = get_thread(&pool, alice, bob, Some(m2.id), 50).await.unwrap();
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].id, m1.id);
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn archive_conversation_hides_from_list(pool: PgPool) {
        let alice = verified_user(&pool, "alice@test.com").await;
        let bob   = verified_user(&pool, "bob@test.com").await;
        let http  = reqwest::Client::new();
        let bus   = EventBus::new();

        send_message(&pool, &http, alice, test_body(bob, "hi"), &bus).await.unwrap();

        // Before archive: alice sees the conversation.
        let before = list_conversations(&pool, alice).await.unwrap();
        assert_eq!(before.len(), 1);

        archive_conversation(&pool, alice, bob).await.unwrap();

        // After archive: conversation no longer appears for alice.
        let after = list_conversations(&pool, alice).await.unwrap();
        assert!(after.is_empty(), "archived conversation must not appear in list");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn list_conversations_returns_empty_for_new_user(pool: PgPool) {
        let alice = verified_user(&pool, "alice@test.com").await;
        let convs = list_conversations(&pool, alice).await.unwrap();
        assert!(convs.is_empty());
    }
}
