use sqlx::PgPool;

use crate::{
    error::{AppError, AppResult},
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
        .ok_or(AppError::NotFound)
}

pub async fn get_social_access(pool: &PgPool, user_id: UserId) -> AppResult<SocialAccess> {
    repository::social_access(pool, user_id)
        .await?
        .ok_or(AppError::NotFound)
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
