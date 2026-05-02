use sqlx::PgPool;

use crate::{
    error::{DomainError, AppResult},
    types::UserId,
};
use super::{
    repository,
    types::{PublicProfile, UserSearchResult},
};

/// Search for users matching `query` (matched against display name and email).
pub async fn search_users(pool: &PgPool, query: &str) -> AppResult<Vec<UserSearchResult>> {
    repository::search(pool, query).await
}

/// Return the public profile for `user_id`. Returns `NotFound` when the user
/// does not exist or has been banned.
pub async fn get_public_profile(pool: &PgPool, user_id: UserId) -> AppResult<PublicProfile> {
    repository::public_profile(pool, user_id)
        .await?
        .ok_or(DomainError::NotFound)
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
