use thiserror::Error;

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("unauthorized")]
    Unauthorized,

    #[error("forbidden")]
    Forbidden,

    #[error("{0}")]
    InvalidInput(String),

    #[error("not found")]
    NotFound,

    #[error("{0}")]
    Conflict(String),

    #[error("{0}")]
    Unprocessable(String),

    #[error("rate limit exceeded")]
    RateLimitExceeded,

    #[error("payment required")]
    PaymentRequired,

    #[error("{0}")]
    ExternalServiceError(String),

    #[error("internal error")]
    Internal(#[from] anyhow::Error),

    #[error("database error")]
    Db(#[from] sqlx::Error),
}

impl DomainError {
    pub fn invalid_input(msg: impl Into<String>) -> Self {
        Self::InvalidInput(msg.into())
    }

    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::Conflict(msg.into())
    }

    pub fn unprocessable(msg: impl Into<String>) -> Self {
        Self::Unprocessable(msg.into())
    }
}

pub type AppResult<T> = Result<T, DomainError>;
