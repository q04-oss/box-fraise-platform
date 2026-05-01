/// OpenAPI 3.0 specification for the box-fraise API.
///
/// Served at:
///   `GET /api/docs/openapi.json` — machine-readable OpenAPI 3.0 JSON spec
///   `GET /api/docs`              — Swagger UI (loads spec from the JSON endpoint)
use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::get,
    Json, Router,
};
use utoipa::openapi::{
    self,
    path::{OperationBuilder, PathItemBuilder},
    Components, Info, OpenApiBuilder, PathsBuilder,
};

use crate::app::AppState;

/// Build the OpenAPI 3.0 document for this API.
pub fn build_spec() -> openapi::OpenApi {
    let info = Info::new("Box Fraise API", "0.1.0");

    let paths = PathsBuilder::new()
        // ── Auth ──────────────────────────────────────────────────────────────
        .path("/api/auth/apple", PathItemBuilder::new()
            .operation(openapi::HttpMethod::Post, OperationBuilder::new()
                .summary(Some("Authenticate with Apple Sign In")).tag("auth").build())
            .build())
        .path("/api/auth/me", PathItemBuilder::new()
            .operation(openapi::HttpMethod::Get, OperationBuilder::new()
                .summary(Some("Get authenticated user profile")).tag("auth").build())
            .build())
        .path("/api/auth/push-token", PathItemBuilder::new()
            .operation(openapi::HttpMethod::Patch, OperationBuilder::new()
                .summary(Some("Register or update push notification token")).tag("auth").build())
            .build())
        .path("/api/auth/display-name", PathItemBuilder::new()
            .operation(openapi::HttpMethod::Patch, OperationBuilder::new()
                .summary(Some("Update display name (1–50 chars)")).tag("auth").build())
            .build())
        .path("/api/auth/logout", PathItemBuilder::new()
            .operation(openapi::HttpMethod::Post, OperationBuilder::new()
                .summary(Some("Revoke the current session token")).tag("auth").build())
            .build())
        .path("/api/auth/magic-link", PathItemBuilder::new()
            .operation(openapi::HttpMethod::Post, OperationBuilder::new()
                .summary(Some("Request a magic link email")).tag("auth").build())
            .build())
        .path("/api/auth/magic-link/verify", PathItemBuilder::new()
            .operation(openapi::HttpMethod::Post, OperationBuilder::new()
                .summary(Some("Verify a magic link token and receive a JWT")).tag("auth").build())
            .build())
        // ── Messages ──────────────────────────────────────────────────────────
        .path("/api/messages/conversations", PathItemBuilder::new()
            .operation(openapi::HttpMethod::Get, OperationBuilder::new()
                .summary(Some("List active conversations")).tag("messages").build())
            .build())
        .path("/api/messages/conversations/{userId}/archive", PathItemBuilder::new()
            .operation(openapi::HttpMethod::Post, OperationBuilder::new()
                .summary(Some("Archive a conversation")).tag("messages").build())
            .build())
        .path("/api/messages/{userId}", PathItemBuilder::new()
            .operation(openapi::HttpMethod::Get, OperationBuilder::new()
                .summary(Some("Get message thread with a user")).tag("messages").build())
            .build())
        .path("/api/messages", PathItemBuilder::new()
            .operation(openapi::HttpMethod::Post, OperationBuilder::new()
                .summary(Some("Send a message")).tag("messages").build())
            .build())
        // ── Keys ──────────────────────────────────────────────────────────────
        .path("/api/keys/challenge", PathItemBuilder::new()
            .operation(openapi::HttpMethod::Post, OperationBuilder::new()
                .summary(Some("Issue a key registration challenge")).tag("keys").build())
            .build())
        .path("/api/keys/register", PathItemBuilder::new()
            .operation(openapi::HttpMethod::Post, OperationBuilder::new()
                .summary(Some("Register X3DH public key bundle")).tag("keys").build())
            .build())
        .path("/api/keys/one-time", PathItemBuilder::new()
            .operation(openapi::HttpMethod::Post, OperationBuilder::new()
                .summary(Some("Upload additional one-time pre-keys")).tag("keys").build())
            .build())
        .path("/api/keys/one-time/count", PathItemBuilder::new()
            .operation(openapi::HttpMethod::Get, OperationBuilder::new()
                .summary(Some("Get remaining one-time pre-key count")).tag("keys").build())
            .build())
        .path("/api/keys/bundle/{user_id}", PathItemBuilder::new()
            .operation(openapi::HttpMethod::Get, OperationBuilder::new()
                .summary(Some("Claim key bundle for a user by ID")).tag("keys").build())
            .build())
        .path("/api/keys/bundle/by-code/{code}", PathItemBuilder::new()
            .operation(openapi::HttpMethod::Get, OperationBuilder::new()
                .summary(Some("Claim key bundle for a user by short code")).tag("keys").build())
            .build())
        // ── Users ─────────────────────────────────────────────────────────────
        .path("/api/users/search", PathItemBuilder::new()
            .operation(openapi::HttpMethod::Get, OperationBuilder::new()
                .summary(Some("Search users by display name or email")).tag("users").build())
            .build())
        .path("/api/users/{id}/public-profile", PathItemBuilder::new()
            .operation(openapi::HttpMethod::Get, OperationBuilder::new()
                .summary(Some("Get a user's public profile")).tag("users").build())
            .build())
        // ── Dorotka ───────────────────────────────────────────────────────────
        .path("/api/dorotka/ask", PathItemBuilder::new()
            .operation(openapi::HttpMethod::Post, OperationBuilder::new()
                .summary(Some("Ask the Dorotka AI assistant")).tag("dorotka").build())
            .build())
        // ── Meta ──────────────────────────────────────────────────────────────
        .path("/health", PathItemBuilder::new()
            .operation(openapi::HttpMethod::Get, OperationBuilder::new()
                .summary(Some("Platform health check")).tag("meta").build())
            .build())
        .build();

    OpenApiBuilder::new()
        .info(info)
        .paths(paths)
        .components(Some(Components::new()))
        .build()
}

/// Serve the OpenAPI 3.0 JSON spec.
pub async fn openapi_json(_state: State<AppState>) -> Json<openapi::OpenApi> {
    Json(build_spec())
}

/// Serve a Swagger UI HTML page that loads the spec from `/api/docs/openapi.json`.
pub async fn swagger_ui() -> Response {
    let html = r#"<!DOCTYPE html>
<html>
<head>
  <title>Box Fraise API</title>
  <meta charset="utf-8"/>
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <link rel="stylesheet" type="text/css" href="https://unpkg.com/swagger-ui-dist@5/swagger-ui.css" >
</head>
<body>
<div id="swagger-ui"></div>
<script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js"> </script>
<script>
window.onload = function() {
  SwaggerUIBundle({
    url: "/api/docs/openapi.json",
    dom_id: '#swagger-ui',
    presets: [SwaggerUIBundle.presets.apis, SwaggerUIBundle.SwaggerUIStandalonePreset],
    layout: "StandaloneLayout"
  })
}
</script>
</body>
</html>"#;

    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        Html(html),
    )
        .into_response()
}

/// Build the router serving both the OpenAPI JSON spec and Swagger UI.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/docs/openapi.json", get(openapi_json))
        .route("/api/docs",              get(swagger_ui))
}
