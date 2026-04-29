/// Typed JSON extractor that maps deserialization errors to `AppError::BadRequest`.
///
/// Drop-in replacement for `axum::Json` on request bodies.
/// Response bodies still use `axum::Json` directly.
use axum::{
    extract::{FromRequest, Request},
    Json,
};
use serde::de::DeserializeOwned;

use crate::error::AppError;

pub struct AppJson<T>(pub T);

impl<T, S> FromRequest<S> for AppJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, AppError> {
        Json::<T>::from_request(req, state)
            .await
            .map(|Json(v)| AppJson(v))
            .map_err(|e| AppError::bad_request(e.body_text()))
    }
}
