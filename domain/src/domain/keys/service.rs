use base64::{engine::general_purpose::STANDARD, Engine};
use ring::signature::{self, UnparsedPublicKey};
use sqlx::PgPool;

use crate::error::{DomainError, AppResult};
use crate::types::{KeyId, UserId};
use super::{
    repository,
    types::{KeyBundleResponse, OtpkResponse, RegisterKeysBody},
};

// ── Challenge ─────────────────────────────────────────────────────────────────

pub async fn issue_challenge(pool: &PgPool, user_id: UserId) -> AppResult<String> {
    repository::create_challenge(pool, user_id).await
}

// ── Key registration ──────────────────────────────────────────────────────────

pub async fn register_keys(
    pool:    &PgPool,
    user_id: UserId,
    body:    RegisterKeysBody,
) -> AppResult<()> {
    match (&body.identity_signing_key, &body.challenge_sig) {
        (Some(signing_key_b64), Some(sig_b64)) => {
            let challenge = repository::consume_challenge(pool, user_id)
                .await?
                .ok_or_else(|| DomainError::invalid_input("no valid challenge found — request one first"))?;

            verify_ed25519(challenge.as_bytes(), sig_b64, signing_key_b64)?;
        }
        (Some(_), None) => {
            return Err(DomainError::invalid_input(
                "challenge_sig is required when identity_signing_key is provided",
            ));
        }
        _ => {}
    }

    repository::upsert_user_keys(
        pool,
        user_id,
        &body.identity_key,
        body.identity_signing_key.as_deref(),
        &body.signed_pre_key,
        &body.signed_pre_key_sig,
    )
    .await?;

    if !body.one_time_pre_keys.is_empty() {
        let pairs: Vec<(KeyId, String)> = body
            .one_time_pre_keys
            .into_iter()
            .map(|k| (k.key_id, k.public_key))
            .collect();
        repository::insert_otpks(pool, user_id, &pairs).await?;
    }

    Ok(())
}

// ── OPK management ────────────────────────────────────────────────────────────

pub async fn upload_otpks(pool: &PgPool, user_id: UserId, keys: Vec<(KeyId, String)>) -> AppResult<()> {
    repository::insert_otpks(pool, user_id, &keys).await
}

pub async fn get_otpk_count(pool: &PgPool, user_id: UserId) -> AppResult<i64> {
    repository::count_otpks(pool, user_id).await
}

// ── Key bundle (X3DH) ─────────────────────────────────────────────────────────

const KEY_REFRESH_GRACE_DAYS: i64 = 30;

// COMMAND — reads key material and atomically claims one OTPK.
// Named claim_ because it consumes a one-time pre-key from the DB on every
// call. OTPK consumption is inseparable: X3DH requires the initiating party
// receive exactly one fresh pre-key per session establishment.
pub async fn claim_key_bundle(pool: &PgPool, target_id: UserId) -> AppResult<KeyBundleResponse> {
    let keys = repository::find_user_keys(pool, target_id)
        .await?
        .ok_or(DomainError::NotFound)?;

    if keys.identity_signing_key.is_none() {
        let age_days = (chrono::Utc::now().naive_utc() - keys.updated_at).num_days();
        if age_days >= KEY_REFRESH_GRACE_DAYS {
            return Err(DomainError::Unprocessable("keys_expired".to_string()));
        }
        return Err(DomainError::Conflict("keys_need_refresh".to_string()));
    }

    let otpk = repository::claim_otpk(pool, target_id).await?.map(|r| OtpkResponse {
        key_id:     r.key_id,
        public_key: r.public_key,
    });

    Ok(KeyBundleResponse {
        user_id:              target_id,
        identity_key:         keys.identity_key,
        identity_signing_key: keys.identity_signing_key,
        signed_pre_key:       keys.signed_pre_key,
        signed_pre_key_sig:   keys.signed_pre_key_sig,
        one_time_pre_key:     otpk,
    })
}

pub async fn claim_key_bundle_by_code(pool: &PgPool, code: &str) -> AppResult<KeyBundleResponse> {
    let target_id = repository::user_id_by_code(pool, code)
        .await?
        .ok_or(DomainError::NotFound)?;

    claim_key_bundle(pool, target_id).await
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn verify_ed25519(message: &[u8], sig_b64: &str, pubkey_b64: &str) -> AppResult<()> {
    let sig = STANDARD
        .decode(sig_b64)
        .map_err(|_| DomainError::invalid_input("invalid signature encoding"))?;

    let key = STANDARD
        .decode(pubkey_b64)
        .map_err(|_| DomainError::invalid_input("invalid signing key encoding"))?;

    UnparsedPublicKey::new(&signature::ED25519, &key)
        .verify(message, &sig)
        .map_err(|_| DomainError::invalid_input("signature verification failed"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{KeyId, UserId};
    use sqlx::PgPool;

    async fn test_user(pool: &PgPool) -> UserId {
        let (id,): (i32,) =
            sqlx::query_as("INSERT INTO users (email, verified) VALUES ($1, true) RETURNING id")
                .bind("keyuser@test.com")
                .fetch_one(pool)
                .await
                .unwrap();
        UserId::from(id)
    }

    /// Seed user_keys WITH identity_signing_key so claim_key_bundle succeeds.
    async fn seed_keys(pool: &PgPool, user_id: UserId) {
        sqlx::query(
            "INSERT INTO user_keys
                 (user_id, identity_key, identity_signing_key, signed_pre_key, signed_pre_key_sig)
             VALUES ($1, 'ik', 'isk', 'spk', 'spk_sig')",
        )
        .bind(user_id)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_otpk(pool: &PgPool, user_id: UserId, key_id: i32) {
        sqlx::query(
            "INSERT INTO one_time_pre_keys (user_id, key_id, public_key) VALUES ($1, $2, 'opk')",
        )
        .bind(user_id)
        .bind(key_id)
        .execute(pool)
        .await
        .unwrap();
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn register_keys_inserts_user_keys_row(pool: PgPool) {
        let user_id = test_user(&pool).await;
        let body = RegisterKeysBody {
            identity_key:         "ik_pub".to_owned(),
            identity_signing_key: None,
            signed_pre_key:       "spk_pub".to_owned(),
            signed_pre_key_sig:   "spk_sig".to_owned(),
            one_time_pre_keys:    vec![],
            challenge_sig:        None,
        };
        register_keys(&pool, user_id, body).await.unwrap();

        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM user_keys WHERE user_id = $1")
                .bind(i32::from(user_id))
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 1);
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn register_keys_batch_inserts_otpks(pool: PgPool) {
        let user_id = test_user(&pool).await;
        let body = RegisterKeysBody {
            identity_key:         "ik".to_owned(),
            identity_signing_key: None,
            signed_pre_key:       "spk".to_owned(),
            signed_pre_key_sig:   "sig".to_owned(),
            one_time_pre_keys: vec![
                crate::domain::keys::types::OneTimePreKeyItem { key_id: KeyId::from(1), public_key: "a".to_owned() },
                crate::domain::keys::types::OneTimePreKeyItem { key_id: KeyId::from(2), public_key: "b".to_owned() },
            ],
            challenge_sig: None,
        };
        register_keys(&pool, user_id, body).await.unwrap();

        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM one_time_pre_keys WHERE user_id = $1")
                .bind(i32::from(user_id))
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 2);
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn claim_key_bundle_returns_bundle_with_otpk(pool: PgPool) {
        let user_id = test_user(&pool).await;
        seed_keys(&pool, user_id).await;
        seed_otpk(&pool, user_id, 42).await;

        let bundle = claim_key_bundle(&pool, user_id).await.unwrap();
        assert_eq!(bundle.user_id, user_id);
        assert!(bundle.one_time_pre_key.is_some(), "should have consumed one OTPK");
        assert_eq!(bundle.one_time_pre_key.unwrap().key_id, KeyId::from(42));
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn claim_key_bundle_returns_none_otpk_when_depleted(pool: PgPool) {
        let user_id = test_user(&pool).await;
        seed_keys(&pool, user_id).await;
        // No OTPKs seeded — bundle is depleted.

        let bundle = claim_key_bundle(&pool, user_id).await.unwrap();
        assert!(bundle.one_time_pre_key.is_none(), "depleted bundle must have no OTPK");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn claim_key_bundle_not_found_for_unknown_user(pool: PgPool) {
        let result = claim_key_bundle(&pool, UserId::from(99999)).await;
        assert!(matches!(result, Err(DomainError::NotFound)));
    }
}
