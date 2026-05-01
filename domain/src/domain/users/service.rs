use sqlx::PgPool;

use crate::{
    error::{DomainError, AppResult},
    types::UserId,
};
use super::{
    repository,
    types::{NotificationRow, PublicProfile, SocialAccess, UserSearchResult},
};

pub async fn search_users(pool: &PgPool, query: &str) -> AppResult<Vec<UserSearchResult>> {
    repository::search(pool, query).await
}

pub async fn get_public_profile(pool: &PgPool, user_id: UserId) -> AppResult<PublicProfile> {
    repository::public_profile(pool, user_id)
        .await?
        .ok_or(DomainError::NotFound)
}

pub async fn get_social_access(pool: &PgPool, user_id: UserId) -> AppResult<SocialAccess> {
    repository::social_access(pool, user_id)
        .await?
        .ok_or(DomainError::NotFound)
}

pub async fn list_notifications(
    pool:    &PgPool,
    user_id: UserId,
) -> AppResult<Vec<NotificationRow>> {
    repository::list_notifications(pool, user_id).await
}

pub async fn mark_notification_read(
    pool:     &PgPool,
    user_id:  UserId,
    notif_id: i32,
) -> AppResult<()> {
    repository::mark_read(pool, user_id, notif_id).await
}

pub async fn mark_all_notifications_read(pool: &PgPool, user_id: UserId) -> AppResult<()> {
    repository::mark_all_read(pool, user_id).await
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
        assert!(
            matches!(result, Err(DomainError::NotFound)),
            "unknown user must return NotFound, got: {result:?}"
        );
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn get_public_profile_returns_not_found_for_banned_user(pool: PgPool) {
        let user_id = insert_user(&pool, "banned@test.com").await;
        sqlx::query("UPDATE users SET banned = true WHERE id = $1")
            .bind(i32::from(user_id))
            .execute(&pool)
            .await
            .unwrap();

        let result = get_public_profile(&pool, user_id).await;
        assert!(
            matches!(result, Err(DomainError::NotFound)),
            "banned user must not appear in public profile, got: {result:?}"
        );
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn list_notifications_returns_empty_for_new_user(pool: PgPool) {
        let user_id = insert_user(&pool, "fresh@test.com").await;
        let notes = list_notifications(&pool, user_id).await.unwrap();
        assert!(notes.is_empty());
    }
}
