use sqlx::PgPool;

use crate::{
    audit,
    error::{AppResult, DomainError},
    types::UserId,
};
use crate::domain::auth::repository as user_repo;
use super::{
    repository,
    types::{
        PlatformConfigurationHistoryResponse, PlatformConfigurationResponse,
        PlatformConfigurationRow, UpdateConfigurationRequest,
    },
};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn to_response(row: PlatformConfigurationRow) -> PlatformConfigurationResponse {
    PlatformConfigurationResponse {
        key:               row.key,
        value:             row.value,
        value_type:        row.value_type,
        description:       row.description,
        cache_ttl_seconds: row.cache_ttl_seconds,
        updated_at:        row.updated_at,
    }
}

fn validate_value(value: &str, value_type: &str) -> AppResult<()> {
    match value_type {
        "integer" => {
            if value.parse::<i64>().is_err() {
                return Err(DomainError::InvalidInput(
                    format!("value '{value}' is not a valid integer"),
                ));
            }
        }
        "boolean" => {
            if value != "true" && value != "false" {
                return Err(DomainError::InvalidInput(
                    "boolean value must be 'true' or 'false'".to_string(),
                ));
            }
        }
        "interval" => {
            match value.parse::<i64>() {
                Ok(n) if n > 0 => {}
                _ => return Err(DomainError::InvalidInput(
                    format!("value '{value}' is not a valid positive integer (interval)"),
                )),
            }
        }
        "text" => {} // any string is valid
        _ => return Err(DomainError::InvalidInput(
            format!("unknown value_type '{value_type}'"),
        )),
    }
    Ok(())
}

// ── Service functions ─────────────────────────────────────────────────────────

/// Ensure BFIP Section 15 defaults are present — called at server startup.
///
/// Uses ON CONFLICT DO NOTHING so custom values set by admins are never overwritten.
pub async fn initialize_defaults(pool: &PgPool) -> AppResult<()> {
    repository::seed_defaults(pool).await
}

/// Return all configuration values — platform_admin only (BFIP Section 15.1).
pub async fn get_all_configuration(
    pool:               &PgPool,
    requesting_user_id: UserId,
) -> AppResult<Vec<PlatformConfigurationResponse>> {
    let uid = i32::from(requesting_user_id);

    let user = user_repo::find_by_id(pool, requesting_user_id)
        .await?
        .ok_or(DomainError::Unauthorized)?;
    if !user.is_platform_admin {
        return Err(DomainError::Forbidden);
    }

    let rows = repository::get_all(pool).await?;

    audit::write(
        pool,
        Some(uid),
        None,
        "config.viewed",
        serde_json::json!({ "key_count": rows.len() }),
    ).await;

    Ok(rows.into_iter().map(to_response).collect())
}

/// Return a single configuration value by key — no auth required (BFIP Section 15.2).
///
/// Used internally by the application to read protocol parameters at runtime.
pub async fn get_configuration(
    pool: &PgPool,
    key:  &str,
) -> AppResult<PlatformConfigurationResponse> {
    let row = repository::get_by_key(pool, key)
        .await?
        .ok_or(DomainError::NotFound)?;
    Ok(to_response(row))
}

/// Update a configuration value — platform_admin only (BFIP Section 15.3).
///
/// Records previous value in `platform_configuration_history` before updating.
pub async fn update_configuration(
    pool:               &PgPool,
    key:                &str,
    requesting_user_id: UserId,
    req:                UpdateConfigurationRequest,
) -> AppResult<PlatformConfigurationResponse> {
    let uid = i32::from(requesting_user_id);

    // 1. Must be platform_admin.
    let user = user_repo::find_by_id(pool, requesting_user_id)
        .await?
        .ok_or(DomainError::Unauthorized)?;
    if !user.is_platform_admin {
        return Err(DomainError::Forbidden);
    }

    // 2. Key must exist.
    let current = repository::get_by_key(pool, key)
        .await?
        .ok_or(DomainError::NotFound)?;

    // 3. Validate new value against value_type.
    validate_value(&req.value, &current.value_type)?;

    let previous_value = current.value.clone();

    // 4–5. Update.
    let updated = repository::update_value(pool, key, &req.value, uid).await?;

    // 6. Record history.
    repository::record_history(
        pool,
        current.id,
        &previous_value,
        &req.value,
        uid,
    ).await?;

    // 7. Audit.
    audit::write(
        pool,
        Some(uid),
        None,
        "config.updated",
        serde_json::json!({
            "key":            key,
            "previous_value": &previous_value,
            "new_value":      &req.value,
        }),
    ).await;

    Ok(to_response(updated))
}

/// Return full change history for a key — platform_admin only (BFIP Section 15.4).
pub async fn get_configuration_history(
    pool:               &PgPool,
    key:                &str,
    requesting_user_id: UserId,
) -> AppResult<Vec<PlatformConfigurationHistoryResponse>> {
    let user = user_repo::find_by_id(pool, requesting_user_id)
        .await?
        .ok_or(DomainError::Unauthorized)?;
    if !user.is_platform_admin {
        return Err(DomainError::Forbidden);
    }

    repository::get_history_by_key(pool, key).await
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::UserId;
    use sqlx::PgPool;

    async fn create_admin(pool: &PgPool, email: &str) -> UserId {
        let (id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified, is_platform_admin) \
             VALUES ($1, true, true) RETURNING id",
        )
        .bind(email).fetch_one(pool).await.unwrap();
        UserId::from(id)
    }

    async fn create_user(pool: &PgPool, email: &str) -> UserId {
        let (id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id",
        )
        .bind(email).fetch_one(pool).await.unwrap();
        UserId::from(id)
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn get_all_configuration_returns_seeded_defaults(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        initialize_defaults(&pool).await.expect("seed must succeed");
        let admin = create_admin(&pool, &SafeEmail().fake::<String>()).await;

        let configs = get_all_configuration(&pool, admin).await.unwrap();
        assert_eq!(configs.len(), 14, "must have all 14 default keys");

        // Spot-check values.
        let cooling = configs.iter().find(|c| c.key == "cooling_period_days").unwrap();
        assert_eq!(cooling.value, "7");
        assert_eq!(cooling.value_type, "integer");

        let boolean = configs.iter().find(|c| c.key == "cleared_requires_all_checks").unwrap();
        assert_eq!(boolean.value, "true");
        assert_eq!(boolean.value_type, "boolean");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn get_all_configuration_requires_platform_admin(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        initialize_defaults(&pool).await.unwrap();
        let user = create_user(&pool, &SafeEmail().fake::<String>()).await;

        let err = get_all_configuration(&pool, user).await.unwrap_err();
        assert!(matches!(err, DomainError::Forbidden));
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn update_configuration_succeeds_for_valid_value(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        initialize_defaults(&pool).await.unwrap();
        let admin = create_admin(&pool, &SafeEmail().fake::<String>()).await;
        let uid   = i32::from(admin);

        let resp = update_configuration(
            &pool, "cooling_period_days", admin,
            UpdateConfigurationRequest { value: "14".to_string() },
        ).await.expect("update must succeed");

        assert_eq!(resp.value, "14");

        // History record created.
        let hist = repository::get_history_by_key(&pool, "cooling_period_days").await.unwrap();
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].previous_value, "7");
        assert_eq!(hist[0].new_value, "14");

        // Audit event written.
        let ae_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_events WHERE event_kind = 'config.updated'"
        ).fetch_one(&pool).await.unwrap();
        assert!(ae_count >= 1);

        let _ = uid;
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn update_configuration_validates_integer_type(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        initialize_defaults(&pool).await.unwrap();
        let admin = create_admin(&pool, &SafeEmail().fake::<String>()).await;

        let err = update_configuration(
            &pool, "cooling_period_days", admin,
            UpdateConfigurationRequest { value: "not_a_number".to_string() },
        ).await.unwrap_err();

        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn update_configuration_validates_boolean_type(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        initialize_defaults(&pool).await.unwrap();
        let admin = create_admin(&pool, &SafeEmail().fake::<String>()).await;

        let err = update_configuration(
            &pool, "cleared_requires_all_checks", admin,
            UpdateConfigurationRequest { value: "yes".to_string() },
        ).await.unwrap_err();

        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn update_configuration_records_previous_value(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        initialize_defaults(&pool).await.unwrap();
        let admin = create_admin(&pool, &SafeEmail().fake::<String>()).await;

        update_configuration(&pool, "cooling_period_days", admin,
            UpdateConfigurationRequest { value: "14".to_string() }).await.unwrap();
        update_configuration(&pool, "cooling_period_days", admin,
            UpdateConfigurationRequest { value: "21".to_string() }).await.unwrap();

        let hist = repository::get_history_by_key(&pool, "cooling_period_days").await.unwrap();
        // Returned DESC: most recent first.
        assert_eq!(hist.len(), 2);
        assert_eq!(hist[0].previous_value, "14");
        assert_eq!(hist[0].new_value, "21");
        assert_eq!(hist[1].previous_value, "7");
        assert_eq!(hist[1].new_value, "14");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn initialize_defaults_is_idempotent(pool: PgPool) {
        initialize_defaults(&pool).await.expect("first seed must succeed");
        initialize_defaults(&pool).await.expect("second seed must succeed");

        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM platform_configuration"
        ).fetch_one(&pool).await.unwrap();
        assert_eq!(count, 14, "idempotent seed must not create duplicate rows");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn get_configuration_history_returns_chronological(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        initialize_defaults(&pool).await.unwrap();
        let admin = create_admin(&pool, &SafeEmail().fake::<String>()).await;

        for v in &["10", "20", "30"] {
            update_configuration(&pool, "cooling_period_days", admin,
                UpdateConfigurationRequest { value: v.to_string() }).await.unwrap();
        }

        let hist = get_configuration_history(&pool, "cooling_period_days", admin)
            .await.unwrap();

        assert_eq!(hist.len(), 3);
        // DESC order — most recent first.
        assert!(hist[0].changed_at >= hist[1].changed_at);
        assert!(hist[1].changed_at >= hist[2].changed_at);
        assert_eq!(hist[0].new_value, "30");
    }

    // ── Adversarial tests ─────────────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_update_configuration_without_admin(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        initialize_defaults(&pool).await.unwrap();
        let user = create_user(&pool, &SafeEmail().fake::<String>()).await;

        let err = update_configuration(
            &pool, "cooling_period_days", user,
            UpdateConfigurationRequest { value: "999".to_string() },
        ).await.unwrap_err();

        assert!(matches!(err, DomainError::Forbidden));
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_inject_invalid_type_values(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        initialize_defaults(&pool).await.unwrap();
        let admin = create_admin(&pool, &SafeEmail().fake::<String>()).await;

        // SQL injection attempt via integer field.
        let err = update_configuration(
            &pool, "cooling_period_days", admin,
            UpdateConfigurationRequest { value: "1; DROP TABLE platform_configuration;--".to_string() },
        ).await.unwrap_err();

        assert!(matches!(err, DomainError::InvalidInput(_)),
            "SQL injection attempt must be rejected by type validation");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_view_configuration_without_admin(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        initialize_defaults(&pool).await.unwrap();
        let user = create_user(&pool, &SafeEmail().fake::<String>()).await;

        let err = get_all_configuration(&pool, user).await.unwrap_err();
        assert!(matches!(err, DomainError::Forbidden));
    }
}
