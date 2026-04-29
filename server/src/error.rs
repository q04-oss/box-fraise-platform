use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use serde_json::json;
use thiserror::Error;

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
    #[error("internal error")]
    Internal(#[from] anyhow::Error),
    #[error("database error")]
    Db(#[from] sqlx::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::Unauthorized     => (StatusCode::UNAUTHORIZED,           self.to_string()),
            AppError::Forbidden        => (StatusCode::FORBIDDEN,              self.to_string()),
            AppError::BadRequest(m)    => (StatusCode::BAD_REQUEST,            m.clone()),
            AppError::NotFound         => (StatusCode::NOT_FOUND,              self.to_string()),
            AppError::Internal(_)      => (StatusCode::INTERNAL_SERVER_ERROR,  "internal error".into()),
            AppError::Db(_)            => (StatusCode::INTERNAL_SERVER_ERROR,  "internal error".into()),
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
