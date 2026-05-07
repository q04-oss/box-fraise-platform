//! Real-time notification stream (Hardening §7).
//!
//! Server-Sent Events (SSE) was chosen over WebSockets because every event
//! flows server → client; there's no client → server message path on this
//! channel. SSE auto-reconnects on disconnect and works over plain HTTP/1.1.
//!
//! The wire format is `data: <NotificationEvent JSON>\n\n` with axum's
//! default keep-alive comment frames every 15 seconds.

pub mod routes;
