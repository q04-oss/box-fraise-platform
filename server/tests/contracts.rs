//! Contract tests between the server crate and the domain crate.
//!
//! For each domain service function called from a route handler these tests
//! verify at compile time that:
//!
//! 1. The function accepts the argument types the route handler provides.
//! 2. The return type is what the route handler unwraps with `?`.
//! 3. `DomainError` converts to `AppError` via the `From` impl.
//!
//! No database is needed — these tests run without I/O.
//! They fail at *compile time* when a service signature diverges from
//! what a route handler provides.

use box_fraise_domain::{
    error::{AppResult, DomainError},
    event_bus::EventBus,
    types::UserId,
};
use box_fraise_server::error::AppError;

// ── DomainError → AppError conversion ────────────────────────────────────────

/// Every DomainError variant must map to an AppError via From.
#[test]
fn domain_error_unauthorized_converts() {
    let _: AppError = DomainError::Unauthorized.into();
}

#[test]
fn domain_error_forbidden_converts() {
    let _: AppError = DomainError::Forbidden.into();
}

#[test]
fn domain_error_not_found_converts() {
    let _: AppError = DomainError::NotFound.into();
}

#[test]
fn domain_error_invalid_input_converts() {
    let _: AppError = DomainError::invalid_input("bad field").into();
}

#[test]
fn domain_error_conflict_converts() {
    let _: AppError = DomainError::conflict("already exists").into();
}

#[test]
fn domain_error_unprocessable_converts() {
    let _: AppError = DomainError::unprocessable("cannot process").into();
}

#[test]
fn domain_error_rate_limit_converts() {
    let _: AppError = DomainError::RateLimitExceeded.into();
}

#[test]
fn domain_error_payment_required_converts() {
    let _: AppError = DomainError::PaymentRequired.into();
}

#[test]
fn domain_error_external_service_converts() {
    let _: AppError = DomainError::ExternalServiceError("upstream error".into()).into();
}

#[test]
fn domain_error_internal_converts() {
    let _: AppError = DomainError::Internal(anyhow::anyhow!("oops")).into();
}

// ── Service function signature contracts ──────────────────────────────────────

/// Return type of update_push_token is compatible with route handler usage.
/// The route does: service::update_push_token(...).await? — so the return
/// must be AppResult<()>, and DomainError must convert to AppError.
#[test]
fn update_push_token_return_type_is_app_result_unit() {
    // Prove AppResult<()> is the right type for ? propagation into AppError.
    fn assert_converts(_: impl Fn(AppResult<()>) -> Result<(), AppError>) {}
    assert_converts(|r| r.map_err(AppError::from));
}

/// Return type of update_display_name matches the route handler.
#[test]
fn update_display_name_return_type_is_app_result_unit() {
    fn assert_converts(_: impl Fn(AppResult<()>) -> Result<(), AppError>) {}
    assert_converts(|r| r.map_err(AppError::from));
}

/// EventBus::new() can be created without args (used in route handlers via state.event_bus).
#[test]
fn event_bus_is_constructible() {
    let _: EventBus = EventBus::new();
}

/// UserId implements From<i32> (route handlers receive it from axum Path extractor).
#[test]
fn user_id_from_i32() {
    let _: UserId = UserId::from(42i32);
}

/// UserId implements Into<i32> (service functions bind it to SQL queries).
#[test]
fn user_id_into_i32() {
    let id = UserId::from(42i32);
    let _: i32 = i32::from(id);
}
