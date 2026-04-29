/// Request tracing constants and helpers.
///
/// The actual TraceLayer, SetRequestIdLayer, and PropagateRequestIdLayer
/// are constructed in app.rs so Rust can infer the concrete generic types
/// without requiring us to name them explicitly.

/// The canonical header name for request IDs echoed in responses.
pub const REQUEST_ID_HEADER: &str = "x-request-id";
