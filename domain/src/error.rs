use thiserror::Error;

/// All error conditions that can arise within the domain layer.
///
/// This type knows nothing about HTTP — status codes are assigned in
/// `server/src/error.rs` via `From<DomainError> for AppError`.
#[derive(Debug, Error)]
pub enum DomainError {
    #[error("unauthorized")]
    /// The request lacks valid authentication credentials.
    Unauthorized,

    #[error("forbidden")]
    /// The authenticated caller does not have permission for this action.
    Forbidden,

    #[error("{0}")]
    /// The caller supplied invalid or malformed input.
    InvalidInput(String),

    #[error("not found")]
    /// The requested resource does not exist.
    NotFound,

    #[error("{0}")]
    /// A state conflict prevents the operation from completing.
    Conflict(String),

    #[error("{0}")]
    /// The operation cannot be completed due to the current state of the resource.
    Unprocessable(String),

    #[error("rate limit exceeded")]
    /// The caller has exceeded the allowed request rate.
    RateLimitExceeded,

    #[error("payment required")]
    /// The operation requires a completed payment.
    PaymentRequired,

    #[error("{0}")]
    /// An upstream external service returned an error.
    ExternalServiceError(String),

    #[error("internal error")]
    /// An unexpected internal failure occurred.
    Internal(#[from] anyhow::Error),

    #[error("database error")]
    /// A database query failed.
    Db(#[from] sqlx::Error),
}

impl DomainError {
    /// Construct an [`InvalidInput`](DomainError::InvalidInput) error with the given message.
    pub fn invalid_input(msg: impl Into<String>) -> Self {
        Self::InvalidInput(msg.into())
    }

    /// Construct a [`Conflict`](DomainError::Conflict) error with the given message.
    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::Conflict(msg.into())
    }

    /// Construct an [`Unprocessable`](DomainError::Unprocessable) error with the given message.
    pub fn unprocessable(msg: impl Into<String>) -> Self {
        Self::Unprocessable(msg.into())
    }
}

/// Alias for `Result<T, DomainError>` used throughout the domain layer.
pub type AppResult<T> = Result<T, DomainError>;
