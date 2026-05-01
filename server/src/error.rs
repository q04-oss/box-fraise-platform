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

    #[error("payment required")]
    PaymentRequired,

    #[error("{0}")]
    BadGateway(String),

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
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            Self::Unauthorized      => (StatusCode::UNAUTHORIZED,          self.to_string()),
            Self::Forbidden         => (StatusCode::FORBIDDEN,             self.to_string()),
            Self::BadRequest(m)     => (StatusCode::BAD_REQUEST,           m.clone()),
            Self::NotFound          => (StatusCode::NOT_FOUND,             self.to_string()),
            Self::Conflict(m)       => (StatusCode::CONFLICT,              m.clone()),
            Self::Unprocessable(m)  => (StatusCode::UNPROCESSABLE_ENTITY,  m.clone()),
            Self::TooManyRequests   => (StatusCode::TOO_MANY_REQUESTS,     self.to_string()),
            Self::PaymentRequired   => (StatusCode::PAYMENT_REQUIRED,      self.to_string()),
            Self::BadGateway(m)     => (StatusCode::BAD_GATEWAY,           m.clone()),
            Self::Internal(e) => {
                tracing::error!(error = %e, "internal server error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error".to_owned())
            }
            Self::Db(e) => {
                tracing::error!(error = %e, "database error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error".to_owned())
            }
        };
        (status, Json(json!({ "error": message }))).into_response()
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
