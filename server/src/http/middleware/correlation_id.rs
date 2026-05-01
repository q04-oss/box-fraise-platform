/// Correlation ID middleware.
///
/// Every request gets a server-generated UUID v4. The ID is:
///   - stored as the root tracing span field `request_id` so every log line
///     emitted anywhere inside that request automatically carries it;
///   - echoed in the `X-Request-Id` response header so clients and load
///     balancers can correlate their logs with ours.
///
/// Client-supplied `X-Request-Id` headers are stripped before the handler
/// runs and replaced with our own value — we never trust client-provided IDs.
///
/// # Span structure
///
///   request{request_id=… method=… path=… status=… latency_ms=…}
///     └─ (all handler + middleware events are children of this span)
///
/// # Example log output
///
///   INFO request{request_id=550e8400 method=POST path=/api/auth/login
///               status=200 latency_ms=38}: box_fraise_server: response
///   INFO request{request_id=550e8400 …}: box_fraise_domain::domain::auth::service:
///               verification email delivery failed error=…
use std::time::Instant;

use axum::{
    extract::Request,
    http::{HeaderName, HeaderValue},
    middleware::Next,
    response::Response,
};
use tracing::{info_span, Instrument};
use uuid::Uuid;

static X_REQUEST_ID: HeaderName = HeaderName::from_static("x-request-id");

pub async fn track(mut req: Request, next: Next) -> Response {
    // Generate server-side ID — strip any client-supplied value first.
    req.headers_mut().remove(&X_REQUEST_ID);
    let id = Uuid::new_v4();

    let method = req.method().as_str().to_owned();
    let path   = req.uri().path().to_owned();
    let start  = Instant::now();

    let span = info_span!(
        "request",
        request_id = %id,
        method     = %method,
        path       = %path,
        // Filled in after the response is ready.
        status     = tracing::field::Empty,
        latency_ms = tracing::field::Empty,
    );

    // Run every inner middleware and handler inside this span so their
    // tracing events are automatically tagged with request_id.
    let mut response = next.run(req).instrument(span.clone()).await;

    let status  = response.status().as_u16();
    let latency = start.elapsed().as_millis();

    span.record("status",     status);
    span.record("latency_ms", latency);

    // One summary line per request with the full context.
    tracing::info!(
        parent: &span,
        status,
        latency_ms = latency,
        "response",
    );

    // Write our ID into the response, overriding any value set by inner layers.
    response.headers_mut().insert(
        X_REQUEST_ID.clone(),
        HeaderValue::from_str(&id.to_string()).expect("UUID is always a valid header value"),
    );

    response
}
