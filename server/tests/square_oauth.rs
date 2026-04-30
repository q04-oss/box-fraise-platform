//! Integration and unit tests for Square OAuth — specifically the location ID
//! resolution that was previously a TODO (stored empty string).
//!
//! Run with:
//!   DATABASE_URL=postgres://localhost/test cargo test --test square_oauth

mod common;

use box_fraise_server::{domain::squareoauth::service as squareoauth, integrations::square};
use serde_json::json;
use sqlx::PgPool;
use wiremock::{matchers::{method, path}, Mock, MockServer, ResponseTemplate};

// ── Unit tests: parse_first_active_location ───────────────────────────────────

/// These tests exercise the pure parsing function without HTTP.
/// They document the contract: what Square's response looks like and what
/// we extract from it.

#[test]
fn parse_returns_first_active_location_id() {
    let body = json!({
        "locations": [
            { "id": "LOC_ACTIVE_1", "name": "Main St", "status": "ACTIVE" },
            { "id": "LOC_ACTIVE_2", "name": "Backup",  "status": "ACTIVE" }
        ]
    }).to_string();

    let result = square::parse_first_active_location(&body).unwrap();
    assert_eq!(result, "LOC_ACTIVE_1", "must return the first ACTIVE location");
}

#[test]
fn parse_skips_inactive_locations() {
    let body = json!({
        "locations": [
            { "id": "LOC_CLOSED", "name": "Old Place", "status": "INACTIVE" },
            { "id": "LOC_OPEN",   "name": "New Place", "status": "ACTIVE" }
        ]
    }).to_string();

    let result = square::parse_first_active_location(&body).unwrap();
    assert_eq!(result, "LOC_OPEN", "must skip INACTIVE and return the ACTIVE one");
}

#[test]
fn parse_returns_bad_gateway_when_no_active_locations() {
    let body = json!({
        "locations": [
            { "id": "LOC_CLOSED", "status": "INACTIVE" }
        ]
    }).to_string();

    let err = square::parse_first_active_location(&body).unwrap_err();
    assert!(
        matches!(err, box_fraise_server::error::AppError::BadGateway(_)),
        "no active locations must return BadGateway, got: {err:?}"
    );
}

#[test]
fn parse_returns_bad_gateway_on_empty_locations_array() {
    let body = json!({ "locations": [] }).to_string();
    let err = square::parse_first_active_location(&body).unwrap_err();
    assert!(matches!(err, box_fraise_server::error::AppError::BadGateway(_)));
}

#[test]
fn parse_returns_error_on_malformed_json() {
    let err = square::parse_first_active_location("not json at all").unwrap_err();
    // Malformed JSON is an Internal error (parse failure), not BadGateway
    assert!(
        matches!(err, box_fraise_server::error::AppError::Internal(_)),
        "malformed JSON must return Internal, got: {err:?}"
    );
}

// ── Integration test: callback populates location_id ─────────────────────────

/// The full callback flow with both Square endpoints mocked:
///   POST /oauth2/token  → returns fake tokens
///   GET  /v2/locations  → returns one ACTIVE location
///
/// After handle_callback completes, square_oauth_tokens.square_location_id
/// must contain the location ID from the mock response.
#[sqlx::test(migrations = "migrations")]
async fn callback_stores_resolved_location_id(pool: PgPool) {
    let (_redis, redis_pool) = common::start_redis().await;
    let state = common::build_state_with_square_oauth(pool.clone(), Some(redis_pool.clone()));

    // ── Mock Square API ───────────────────────────────────────────────────────
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token":  "sq_access_test",
            "token_type":    "bearer",
            "expires_at":    "2030-01-01T00:00:00Z",
            "merchant_id":   "MERCHANT_TEST_123",
            "refresh_token": "sq_refresh_test"
        })))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v2/locations"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "locations": [{
                "id":     "LOC_MONTREAL_MAIN",
                "name":   "Montréal Main",
                "status": "ACTIVE"
            }]
        })))
        .mount(&mock_server)
        .await;

    // ── Setup: business + CSRF state ──────────────────────────────────────────
    let biz         = common::create_business(&pool, "Test Café").await;
    let csrf_token  = "test-csrf-state-abc123";
    common::seed_oauth_csrf_state(&redis_pool, csrf_token, biz.id).await;

    // ── Call the callback handler via the service layer ───────────────────────
    let result = squareoauth::handle_callback(
        &state,
        "fake_auth_code",
        csrf_token,
        None,
        &mock_server.uri(), // points at the mock, not Square's real API
    ).await;

    assert!(result.is_ok(), "callback must succeed with mocked Square: {result:?}");

    // ── Assert location_id is stored ──────────────────────────────────────────
    let location_id: String = sqlx::query_scalar(
        "SELECT square_location_id FROM square_oauth_tokens WHERE business_id = $1"
    )
    .bind(biz.id)
    .fetch_one(&pool)
    .await
    .expect("square_oauth_tokens row must exist after callback");

    assert_eq!(
        location_id, "LOC_MONTREAL_MAIN",
        "location_id stored in DB must match what the mock returned"
    );
}

/// When Square's locations endpoint returns no ACTIVE locations, the callback
/// must fail with BadGateway — not silently store an empty string.
#[sqlx::test(migrations = "migrations")]
async fn callback_fails_when_square_has_no_active_locations(pool: PgPool) {
    let (_redis, redis_pool) = common::start_redis().await;
    let state = common::build_state_with_square_oauth(pool.clone(), Some(redis_pool.clone()));

    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token":  "sq_access_test",
            "token_type":    "bearer",
            "expires_at":    "2030-01-01T00:00:00Z",
            "merchant_id":   "MERCHANT_TEST_123",
            "refresh_token": "sq_refresh_test"
        })))
        .mount(&mock_server)
        .await;

    // All locations are INACTIVE
    Mock::given(method("GET"))
        .and(path("/v2/locations"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "locations": [{ "id": "LOC_CLOSED", "status": "INACTIVE" }]
        })))
        .mount(&mock_server)
        .await;

    let biz = common::create_business(&pool, "Test Café").await;
    common::seed_oauth_csrf_state(&redis_pool, "csrf-no-location", biz.id).await;

    let result = squareoauth::handle_callback(
        &state, "fake_code", "csrf-no-location", None, &mock_server.uri(),
    ).await;

    assert!(
        matches!(result, Err(box_fraise_server::error::AppError::BadGateway(_))),
        "no active locations must return BadGateway, got: {result:?}"
    );

    // No tokens should be stored when the callback fails
    let row_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM square_oauth_tokens WHERE business_id = $1"
    )
    .bind(biz.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row_count, 0, "no tokens must be stored on failed callback");
}
