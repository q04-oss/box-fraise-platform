//! S3-compatible object storage (DigitalOcean Spaces).
//!
//! Hardening Section 3. Provides upload, presigned-URL issuance, and delete
//! operations for evidence packages (visit completion photos, attestation
//! photos). Hash computation lives here too — [`StorageClient::compute_evidence_hash`]
//! is the canonical server-side replacement for the client-trusted
//! `evidence_hash` and `photo_hash` fields the database has held to date.
//!
//! `From<StorageError> for DomainError` is implemented in the `domain` crate
//! (see `domain/src/error.rs`) — domain already depends on this crate, so
//! reversing the direction would cycle.

use std::time::Duration;

use aws_sdk_s3::{
    config::{BehaviorVersion, Credentials, Region},
    presigning::PresigningConfig,
    primitives::ByteStream,
    types::{ObjectCannedAcl, ServerSideEncryption},
    Client,
};
use sha2::{Digest, Sha256};

/// Configuration for the [`StorageClient`].
///
/// Production values come from the `SPACES_*` environment variables loaded by
/// [`box_fraise_domain::config::Config`]. All five strings are required —
/// callers should obtain a [`StorageConfig`] only via
/// [`box_fraise_domain::config::Config::storage_config`], which returns
/// `None` when any field is missing.
#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// Spaces access key (analogous to AWS access key ID).
    pub access_key:              String,
    /// Spaces secret key (analogous to AWS secret access key).
    pub secret_key:              String,
    /// Bucket (Spaces "Space") name — e.g. `box-fraise-evidence`.
    pub bucket:                  String,
    /// Endpoint URL — e.g. `https://nyc3.digitaloceanspaces.com`.
    pub endpoint:                String,
    /// Region code — e.g. `nyc3`.
    pub region:                  String,
    /// Default presigned-URL expiry. 900s (15 min) is the recommended value.
    pub presign_expiry_seconds:  u64,
}

/// All errors the [`StorageClient`] can raise. Mapped to
/// `DomainError::ExternalServiceError` (→ `502 Bad Gateway`) by the impl in
/// `domain/src/error.rs`.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// `PutObject` failed.
    #[error("storage upload failed: {0}")]
    Upload(String),
    /// `GetObject` failed (currently only used by future code paths).
    #[error("storage download failed: {0}")]
    Download(String),
    /// Presigning the GET URL failed.
    #[error("storage presign failed: {0}")]
    Presign(String),
    /// `DeleteObject` failed.
    #[error("storage delete failed: {0}")]
    Delete(String),
    /// Configuration is malformed or incomplete.
    #[error("storage misconfigured: {0}")]
    Config(String),
}

/// S3-compatible object storage client wired for DigitalOcean Spaces.
///
/// Cheap to clone via `Arc` at the [`AppState`] level. All operations are
/// async and require a Tokio runtime.
///
/// [`AppState`]: <see server crate>
pub struct StorageClient {
    client:                  Client,
    bucket:                  String,
    presign_expiry_seconds:  u64,
}

impl StorageClient {
    /// Build a client from a [`StorageConfig`].
    ///
    /// Spaces requires path-style URLs (`https://endpoint/bucket/key`)
    /// because `bucket.endpoint` virtual-host routing isn't honoured by
    /// DigitalOcean's edge.
    pub async fn new(config: StorageConfig) -> Result<Self, StorageError> {
        if config.bucket.is_empty() {
            return Err(StorageError::Config("bucket is empty".to_string()));
        }
        if config.endpoint.is_empty() {
            return Err(StorageError::Config("endpoint is empty".to_string()));
        }

        let credentials = Credentials::new(
            config.access_key.clone(),
            config.secret_key.clone(),
            None,                          // session_token
            None,                          // expires_after
            "box-fraise-storage",          // provider_name (telemetry only)
        );

        let s3_config = aws_sdk_s3::config::Builder::new()
            .behavior_version(BehaviorVersion::latest())
            .endpoint_url(&config.endpoint)
            .credentials_provider(credentials)
            .region(Region::new(config.region.clone()))
            .force_path_style(true)
            .build();

        Ok(Self {
            client:                 Client::from_conf(s3_config),
            bucket:                 config.bucket,
            presign_expiry_seconds: config.presign_expiry_seconds,
        })
    }

    /// Upload `data` to `path` as a private, server-side-encrypted object.
    /// Returns `path` unchanged so callers can store it directly.
    pub async fn upload(
        &self,
        path:         &str,
        data:         Vec<u8>,
        content_type: &str,
    ) -> Result<String, StorageError> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(path)
            .body(ByteStream::from(data))
            .content_type(content_type)
            .server_side_encryption(ServerSideEncryption::Aes256)
            .acl(ObjectCannedAcl::Private)
            .send()
            .await
            .map_err(|e| StorageError::Upload(e.to_string()))?;
        Ok(path.to_string())
    }

    /// Compute SHA-256 over `data` and return it as 64 lowercase hex chars.
    ///
    /// This is the canonical server-side hash. It supersedes the
    /// client-supplied `evidence_hash` / `photo_hash` fields the database
    /// previously trusted (see TODOs in `staff::service::complete_visit`
    /// and `attestations::service::initiate_attestation`).
    pub fn compute_evidence_hash(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hex::encode(hasher.finalize())
    }

    /// Generate a presigned `GET` URL for an existing object.
    ///
    /// Default expiry is 900s (15 min); pass `0` to use
    /// [`StorageConfig::presign_expiry_seconds`].
    pub async fn presigned_url(
        &self,
        path:           &str,
        expiry_seconds: u64,
    ) -> Result<String, StorageError> {
        let secs = if expiry_seconds == 0 { self.presign_expiry_seconds } else { expiry_seconds };
        let cfg  = PresigningConfig::expires_in(Duration::from_secs(secs))
            .map_err(|e| StorageError::Presign(e.to_string()))?;
        let req  = self.client
            .get_object()
            .bucket(&self.bucket)
            .key(path)
            .presigned(cfg)
            .await
            .map_err(|e| StorageError::Presign(e.to_string()))?;
        Ok(req.uri().to_string())
    }

    /// Delete an object. Used when a visit is cancelled or an attestation
    /// is rejected and the captured evidence is no longer needed.
    pub async fn delete(&self, path: &str) -> Result<(), StorageError> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(path)
            .send()
            .await
            .map_err(|e| StorageError::Delete(e.to_string()))?;
        Ok(())
    }
}

/// Construct the canonical evidence object key for a given visit.
///
/// Layout: `evidence/visits/<visit_id>/<unix_seconds>_<evidence_type>`.
/// Exposed as a free function so callers (and tests) can rebuild the path
/// without holding a [`StorageClient`].
pub fn evidence_path(visit_id: i32, timestamp_secs: i64, evidence_type: &str) -> String {
    format!("evidence/visits/{visit_id}/{timestamp_secs}_{evidence_type}")
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_evidence_hash_is_deterministic() {
        let data = b"some evidence bytes";
        let a    = StorageClient::compute_evidence_hash(data);
        let b    = StorageClient::compute_evidence_hash(data);
        assert_eq!(a, b, "same bytes must hash to same hex");
    }

    #[test]
    fn compute_evidence_hash_differs_for_different_data() {
        let a = StorageClient::compute_evidence_hash(b"alpha");
        let b = StorageClient::compute_evidence_hash(b"beta");
        assert_ne!(a, b, "different bytes must hash to different hex");
    }

    #[test]
    fn compute_evidence_hash_produces_64_char_hex() {
        let h = StorageClient::compute_evidence_hash(b"");
        assert_eq!(h.len(), 64, "SHA-256 hex must be 64 chars");
        assert!(
            h.chars().all(|c| c.is_ascii_hexdigit()),
            "hash must be lowercase hex, got: {h}",
        );
    }

    #[test]
    fn evidence_path_format_is_correct() {
        let p = evidence_path(123, 1_700_000_000, "photo");
        assert_eq!(p, "evidence/visits/123/1700000000_photo");
    }

    /// End-to-end round trip — upload, presign, GET, delete. Requires real
    /// Spaces credentials. Skipped silently when env vars are absent so
    /// the suite stays green in CI without leaking secrets.
    #[tokio::test]
    async fn upload_and_presign_round_trip() {
        let Ok(access_key)  = std::env::var("SPACES_ACCESS_KEY")  else { return };
        let Ok(secret_key)  = std::env::var("SPACES_SECRET_KEY")  else { return };
        let Ok(bucket)      = std::env::var("SPACES_BUCKET")      else { return };
        let endpoint = std::env::var("SPACES_ENDPOINT")
            .unwrap_or_else(|_| "https://nyc3.digitaloceanspaces.com".to_owned());
        let region   = std::env::var("SPACES_REGION").unwrap_or_else(|_| "nyc3".to_owned());

        let cfg = StorageConfig {
            access_key, secret_key, bucket, endpoint, region,
            presign_expiry_seconds: 900,
        };
        let client = StorageClient::new(cfg).await.expect("client must build");

        let path = format!("test/round-trip-{}", chrono::Utc::now().timestamp());
        let body = b"box-fraise round-trip payload".to_vec();
        let expected_hash = StorageClient::compute_evidence_hash(&body);

        client.upload(&path, body.clone(), "application/octet-stream")
            .await.expect("upload must succeed");

        let url = client.presigned_url(&path, 60).await.expect("presign must succeed");
        let resp = reqwest::get(&url).await.expect("presigned GET must dispatch");
        assert!(resp.status().is_success(), "presigned URL must return 2xx, got {}", resp.status());
        let fetched = resp.bytes().await.expect("presigned body must read").to_vec();
        let fetched_hash = StorageClient::compute_evidence_hash(&fetched);
        assert_eq!(fetched_hash, expected_hash, "round-tripped bytes must match");

        client.delete(&path).await.expect("delete must succeed");
    }
}
