use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

use box_fraise_domain::error::DomainError;

/// HTTP-layer error type. This is the only type in the codebase that maps
/// errors to HTTP status codes. Domain code never sees this type.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("unauthorized")]
    Unauthorized,

    #[error("forbidden")]
    Forbidden,

    #[error("{0}")]
    BadRequest(String),

    #[error("not found")]
    NotFound,

    #[error("{0}")]
    Conflict(String),

    #[error("{0}")]
    Unprocessable(String),

    #[error("rate limit exceeded")]
    TooManyRequests,

    /// Per-user rate limit hit. Carries the seconds-until-window-reset
    /// so the response can return an accurate `Retry-After`.
    #[error("rate limit exceeded")]
    RateLimited(u64),

    #[error("payment required")]
    PaymentRequired,

    #[error("{0}")]
    BadGateway(String),

    #[error("{0}")]
    ServiceUnavailable(String),

    #[error("internal error")]
    Internal(#[from] anyhow::Error),

    #[error("database error")]
    Db(#[from] sqlx::Error),
}

impl AppError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::BadRequest(msg.into())
    }

    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::Conflict(msg.into())
    }

    pub fn unprocessable(msg: impl Into<String>) -> Self {
        Self::Unprocessable(msg.into())
    }

    /// Construct a per-user rate-limit response with a precise
    /// `Retry-After` value (seconds until the window resets).
    pub fn rate_limited(retry_after_secs: u64) -> Self {
        Self::RateLimited(retry_after_secs)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        // Hardening §4 — structured error response.
        // `error`   is a stable machine-readable slug.
        // `message` is the human-readable detail.
        // The X-Request-Id header (added by the correlation_id middleware
        // outside this layer) carries the correlation ID; it isn't echoed
        // in the body because IntoResponse has no request context.
        let (status, error_slug, message) = match &self {
            Self::Unauthorized          => (StatusCode::UNAUTHORIZED,          "unauthorized",        self.to_string()),
            Self::Forbidden             => (StatusCode::FORBIDDEN,             "forbidden",           self.to_string()),
            Self::BadRequest(m)         => (StatusCode::BAD_REQUEST,           "bad_request",         m.clone()),
            Self::NotFound              => (StatusCode::NOT_FOUND,             "not_found",           self.to_string()),
            Self::Conflict(m)           => (StatusCode::CONFLICT,              "conflict",            m.clone()),
            Self::Unprocessable(m)      => (StatusCode::UNPROCESSABLE_ENTITY,  "unprocessable",       m.clone()),
            Self::TooManyRequests       => (StatusCode::TOO_MANY_REQUESTS,     "rate_limited",        self.to_string()),
            Self::RateLimited(_)        => (StatusCode::TOO_MANY_REQUESTS,     "rate_limited",        "Too many requests".to_owned()),
            Self::PaymentRequired       => (StatusCode::PAYMENT_REQUIRED,      "payment_required",    self.to_string()),
            Self::BadGateway(m)         => (StatusCode::BAD_GATEWAY,           "bad_gateway",         m.clone()),
            Self::ServiceUnavailable(m) => (StatusCode::SERVICE_UNAVAILABLE,   "service_unavailable", m.clone()),
            Self::Internal(e) => {
                tracing::error!(error = %e, "internal server error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error", "internal error".to_owned())
            }
            Self::Db(e) => {
                tracing::error!(error = %e, "database error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error", "internal error".to_owned())
            }
        };
        // Hardening §6 — Retry-After on 429 lets clients schedule retries
        // instead of busy-looping. The global IP limiter uses a fixed 60s
        // window; the per-user limiter (Grade A item 3) computes the actual
        // remaining TTL and ships it in the header + body.
        match self {
            Self::TooManyRequests => {
                let body = Json(json!({ "error": error_slug, "message": message }));
                (status, [(axum::http::header::RETRY_AFTER, "60")], body).into_response()
            }
            Self::RateLimited(retry_after) => {
                let body = Json(json!({
                    "error":       error_slug,
                    "message":     message,
                    "retry_after": retry_after,
                }));
                (status,
                 [(axum::http::header::RETRY_AFTER, retry_after.to_string())],
                 body).into_response()
            }
            _ => {
                let body = Json(json!({ "error": error_slug, "message": message }));
                (status, body).into_response()
            }
        }
    }
}

impl From<DomainError> for AppError {
    fn from(e: DomainError) -> Self {
        match e {
            DomainError::Unauthorized            => Self::Unauthorized,
            DomainError::Forbidden               => Self::Forbidden,
            DomainError::InvalidInput(m)         => Self::BadRequest(m),
            DomainError::NotFound                => Self::NotFound,
            DomainError::Conflict(m)             => Self::Conflict(m),
            DomainError::Unprocessable(m)        => Self::Unprocessable(m),
            DomainError::RateLimitExceeded       => Self::TooManyRequests,
            DomainError::PaymentRequired         => Self::PaymentRequired,
            DomainError::ExternalServiceError(m) => Self::BadGateway(m),
            DomainError::Internal(e)             => Self::Internal(e),
            DomainError::Db(e)                   => Self::Db(e),
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;
