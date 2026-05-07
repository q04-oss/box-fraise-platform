//! OpenAPI 3.1 specification for the box-fraise API (Hardening cleanup #7).
//!
//! Spec is **derived** from `#[utoipa::path(...)]` annotations on every route
//! handler plus `#[derive(utoipa::ToSchema)]` on every request/response
//! type. The `ApiDoc` struct below is the single registry — adding a new
//! route means (1) annotate the handler, (2) add the path here, and
//! (3) add any new schemas to `components(schemas(...))`.
//!
//! Served at:
//!   `GET /api/docs/openapi.json` — machine-readable OpenAPI 3.1 JSON spec
//!   `GET /api/docs`              — Swagger UI (loads spec from the JSON endpoint)

use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::get,
    Json, Router,
};
use utoipa::{
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
    Modify, OpenApi,
};

use crate::app::AppState;

/// Adds the `bearer_auth` security scheme to the spec. Routes that need
/// auth declare `security(("bearer_auth" = []))` on their `#[utoipa::path]`.
pub struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearer_auth",
                SecurityScheme::Http(
                    HttpBuilder::new()
                        .scheme(HttpAuthScheme::Bearer)
                        .bearer_format("JWT")
                        .build(),
                ),
            );
        }
    }
}

#[derive(OpenApi)]
#[openapi(
    info(
        title       = "Box Fraise Platform API",
        version     = "0.2.0",
        description = "BFIP v0.2.0 — Box Fraise Identity Protocol",
    ),
    paths(
        // ── Auth ──────────────────────────────────────────────────────────────
        crate::domain::auth::routes::apple,
        crate::domain::auth::routes::me,
        crate::domain::auth::routes::push_token,
        crate::domain::auth::routes::display_name,
        crate::domain::auth::routes::logout,
        crate::domain::auth::routes::magic_link_request,
        crate::domain::auth::routes::magic_link_open,
        crate::domain::auth::routes::magic_link_verify,
        // ── Users ─────────────────────────────────────────────────────────────
        crate::domain::users::routes::search,
        crate::domain::users::routes::public_profile,
        crate::domain::users::routes::erase_me,
        crate::domain::users::routes::export_me,
        crate::domain::users::routes::ban_user,
        crate::domain::users::routes::unban_user,
        // ── Businesses ────────────────────────────────────────────────────────
        crate::domain::businesses::routes::create,
        crate::domain::businesses::routes::list_mine,
        crate::domain::businesses::routes::get_one,
        // ── Beacons ───────────────────────────────────────────────────────────
        crate::domain::beacons::routes::create,
        crate::domain::beacons::routes::list,
        crate::domain::beacons::routes::daily_uuid,
        crate::domain::beacons::routes::rotate_key,
        // ── Presence ──────────────────────────────────────────────────────────
        crate::domain::presence::routes::beacon_dwell,
        crate::domain::presence::routes::nfc_tap,
        crate::domain::presence::routes::status,
        // ── Identity ──────────────────────────────────────────────────────────
        crate::domain::identity_credentials::routes::initiate_verification,
        crate::domain::identity_credentials::routes::stripe_webhook,
        crate::domain::identity_credentials::routes::app_open,
        crate::domain::identity_credentials::routes::cooling_status,
        // ── Background checks ─────────────────────────────────────────────────
        crate::domain::background_checks::routes::initiate,
        crate::domain::background_checks::routes::webhook,
        crate::domain::background_checks::routes::status,
        // ── Staff ─────────────────────────────────────────────────────────────
        crate::domain::staff::routes::grant_role,
        crate::domain::staff::routes::my_roles,
        crate::domain::staff::routes::schedule_visit,
        crate::domain::staff::routes::list_visits,
        crate::domain::staff::routes::arrive,
        crate::domain::staff::routes::complete,
        crate::domain::staff::routes::quality_assessment,
        crate::domain::staff::routes::upload_evidence,
        crate::domain::staff::routes::evidence_url,
        // ── Attestations ──────────────────────────────────────────────────────
        crate::domain::attestations::routes::initiate,
        crate::domain::attestations::routes::list_mine,
        crate::domain::attestations::routes::list_pending,
        crate::domain::attestations::routes::staff_sign,
        crate::domain::attestations::routes::reviewer_sign,
        crate::domain::attestations::routes::reject,
        // ── Soultokens ────────────────────────────────────────────────────────
        crate::domain::soultokens::routes::issue,
        crate::domain::soultokens::routes::get_mine,
        crate::domain::soultokens::routes::renew,
        crate::domain::soultokens::routes::revoke,
        crate::domain::soultokens::routes::surrender,
        crate::domain::soultokens::routes::trust_registry_public_key,
        // ── Orders ────────────────────────────────────────────────────────────
        crate::domain::orders::routes::create,
        crate::domain::orders::routes::list_mine,
        crate::domain::orders::routes::collect,
        crate::domain::orders::routes::cancel,
        crate::domain::orders::routes::activate_box,
        crate::domain::orders::routes::list_boxes,
        // ── Support ───────────────────────────────────────────────────────────
        crate::domain::support::routes::create_booking,
        crate::domain::support::routes::get_my_bookings,
        crate::domain::support::routes::cancel_booking,
        crate::domain::support::routes::attend_booking,
        crate::domain::support::routes::resolve_booking,
        crate::domain::support::routes::list_bookings_for_visit,
        // ── Attestation tokens ────────────────────────────────────────────────
        crate::domain::attestation_tokens::routes::issue,
        crate::domain::attestation_tokens::routes::verify,
        crate::domain::attestation_tokens::routes::get_my_tokens,
        crate::domain::attestation_tokens::routes::revoke,
        // ── Verification events ───────────────────────────────────────────────
        crate::domain::verification_events::routes::my_trail,
        crate::domain::verification_events::routes::my_journey,
        crate::domain::verification_events::routes::admin_trail,
        // ── Platform configuration ────────────────────────────────────────────
        crate::domain::platform_configuration::routes::list_all,
        crate::domain::platform_configuration::routes::get_one,
        crate::domain::platform_configuration::routes::update_one,
        crate::domain::platform_configuration::routes::get_history,
        crate::domain::platform_configuration::routes::list_flags,
        crate::domain::platform_configuration::routes::update_flag,
        crate::domain::platform_configuration::routes::list_subscriptions,
        // ── Analytics ─────────────────────────────────────────────────────────
        crate::domain::analytics::routes::funnel,
        crate::domain::analytics::routes::attestations_daily,
        crate::domain::analytics::routes::attestations_time_to_attest,
        crate::domain::analytics::routes::businesses,
        crate::domain::analytics::routes::presence_daily,
        crate::domain::analytics::routes::soultokens,
        crate::domain::analytics::routes::background_checks,
        crate::domain::analytics::routes::conversion,
        // ── Dorotka ───────────────────────────────────────────────────────────
        crate::domain::dorotka::routes::ask,
        // ── Notifications ─────────────────────────────────────────────────────
        crate::domain::notifications::routes::stream,
    ),
    components(schemas(
        // ── Auth ──────────────────────────────────────────────────────────────
        box_fraise_domain::domain::auth::types::AppleAuthBody,
        box_fraise_domain::domain::auth::types::PushTokenBody,
        box_fraise_domain::domain::auth::types::DisplayNameBody,
        box_fraise_domain::domain::auth::types::MagicLinkBody,
        box_fraise_domain::domain::auth::types::MagicLinkVerifyBody,
        // ── Users ─────────────────────────────────────────────────────────────
        box_fraise_domain::domain::users::types::PublicProfile,
        box_fraise_domain::domain::users::types::UserSearchResult,
        box_fraise_domain::domain::users::types::ErasureResponse,
        box_fraise_domain::domain::users::types::UserDataExport,
        box_fraise_domain::domain::users::types::UserProfileExport,
        box_fraise_domain::domain::users::types::VerificationEventExport,
        box_fraise_domain::domain::users::types::OrderExport,
        box_fraise_domain::domain::users::types::ConsentExport,
        crate::domain::users::routes::BanRequest,
        // ── Businesses ────────────────────────────────────────────────────────
        box_fraise_domain::domain::businesses::types::CreateBusinessRequest,
        box_fraise_domain::domain::businesses::types::BusinessResponse,
        box_fraise_domain::domain::businesses::types::LocationResponse,
        // ── Beacons ───────────────────────────────────────────────────────────
        box_fraise_domain::domain::beacons::types::CreateBeaconRequest,
        box_fraise_domain::domain::beacons::types::BeaconResponse,
        box_fraise_domain::domain::beacons::types::DailyUuidResponse,
        // ── Presence ──────────────────────────────────────────────────────────
        box_fraise_domain::domain::presence::types::RecordBeaconDwellRequest,
        box_fraise_domain::domain::presence::types::RecordNfcTapRequest,
        box_fraise_domain::domain::presence::types::PresenceStatusResponse,
        // NB: `PresenceEventSummary` exists in three domains — only the
        // verification_events variant is registered to avoid name collision;
        // the others are referenced via `inline` in their parent schemas.
        box_fraise_domain::domain::verification_events::types::PresenceEventSummary,
        // ── Identity ──────────────────────────────────────────────────────────
        box_fraise_domain::domain::identity_credentials::types::InitiateVerificationRequest,
        box_fraise_domain::domain::identity_credentials::types::RecordAppOpenRequest,
        box_fraise_domain::domain::identity_credentials::types::IdentityCredentialResponse,
        box_fraise_domain::domain::identity_credentials::types::CoolingStatusResponse,
        // ── Background checks ─────────────────────────────────────────────────
        box_fraise_domain::domain::background_checks::types::InitiateCheckRequest,
        box_fraise_domain::domain::background_checks::types::CheckWebhookPayload,
        box_fraise_domain::domain::background_checks::types::BackgroundCheckResponse,
        box_fraise_domain::domain::background_checks::types::BackgroundCheckStatusResponse,
        // ── Staff ─────────────────────────────────────────────────────────────
        box_fraise_domain::domain::staff::types::GrantRoleRequest,
        box_fraise_domain::domain::staff::types::ScheduleVisitRequest,
        box_fraise_domain::domain::staff::types::ArriveAtVisitRequest,
        box_fraise_domain::domain::staff::types::CompleteVisitRequest,
        box_fraise_domain::domain::staff::types::QualityAssessmentRequest,
        box_fraise_domain::domain::staff::types::StaffRoleResponse,
        box_fraise_domain::domain::staff::types::StaffVisitResponse,
        box_fraise_domain::domain::staff::types::QualityAssessmentRow,
        // ── Attestations ──────────────────────────────────────────────────────
        box_fraise_domain::domain::attestations::types::VisitAttestationRow,
        box_fraise_domain::domain::attestations::types::InitiateAttestationRequest,
        box_fraise_domain::domain::attestations::types::StaffSignAttestationRequest,
        box_fraise_domain::domain::attestations::types::ReviewerSignAttestationRequest,
        box_fraise_domain::domain::attestations::types::RejectAttestationRequest,
        // ── Soultokens ────────────────────────────────────────────────────────
        box_fraise_domain::domain::soultokens::types::IssueSoultokenRequest,
        box_fraise_domain::domain::soultokens::types::RenewSoultokenRequest,
        box_fraise_domain::domain::soultokens::types::RevokeSoultokenRequest,
        box_fraise_domain::domain::soultokens::types::SurrenderSoultokenRequest,
        box_fraise_domain::domain::soultokens::types::SoultokenResponse,
        box_fraise_domain::domain::soultokens::types::SoultokenRenewalResponse,
        // NB: `SoultokenSummary` exists in users + verification_events;
        // verification_events variant is canonical.
        box_fraise_domain::domain::verification_events::types::SoultokenSummary,
        // ── Orders ────────────────────────────────────────────────────────────
        box_fraise_domain::domain::orders::types::CreateOrderRequest,
        box_fraise_domain::domain::orders::types::ActivateBoxRequest,
        box_fraise_domain::domain::orders::types::CollectOrderRequest,
        box_fraise_domain::domain::orders::types::OrderResponse,
        box_fraise_domain::domain::orders::types::VisitBoxResponse,
        // ── Support ───────────────────────────────────────────────────────────
        box_fraise_domain::domain::support::types::CreateBookingRequest,
        box_fraise_domain::domain::support::types::ResolveBookingRequest,
        box_fraise_domain::domain::support::types::CancelBookingRequest,
        box_fraise_domain::domain::support::types::SupportBookingResponse,
        // ── Attestation tokens ────────────────────────────────────────────────
        box_fraise_domain::domain::attestation_tokens::types::IssueAttestationTokenRequest,
        box_fraise_domain::domain::attestation_tokens::types::VerifyAttestationTokenRequest,
        box_fraise_domain::domain::attestation_tokens::types::AttestationTokenResponse,
        box_fraise_domain::domain::attestation_tokens::types::VerificationResultResponse,
        box_fraise_domain::domain::attestation_tokens::types::AttestationTokenMeta,
        // ── Verification events ───────────────────────────────────────────────
        box_fraise_domain::domain::verification_events::types::VerificationEventResponse,
        box_fraise_domain::domain::verification_events::types::AttestationSummary,
        box_fraise_domain::domain::verification_events::types::AttestationTokenSummary,
        box_fraise_domain::domain::verification_events::types::UserAuditTrailResponse,
        // ── Platform configuration ────────────────────────────────────────────
        box_fraise_domain::domain::platform_configuration::types::UpdateConfigurationRequest,
        box_fraise_domain::domain::platform_configuration::types::PlatformConfigurationResponse,
        box_fraise_domain::domain::platform_configuration::types::PlatformConfigurationHistoryResponse,
        box_fraise_domain::domain::platform_configuration::types::FeatureFlagRow,
        box_fraise_domain::domain::platform_configuration::types::UpdateFeatureFlagRequest,
        // ── Analytics (in server crate) ───────────────────────────────────────
        crate::domain::analytics::queries::FunnelStage,
        crate::domain::analytics::queries::DailyCount,
        crate::domain::analytics::queries::TimeToAttest,
        crate::domain::analytics::queries::BusinessStats,
        crate::domain::analytics::queries::SoultokenStats,
        crate::domain::analytics::queries::BackgroundCheckStats,
        crate::domain::analytics::queries::DropOff,
        // ── Dorotka (in server crate) ─────────────────────────────────────────
        crate::domain::dorotka::routes::AskBody,
    )),
    modifiers(&SecurityAddon),
    tags(
        (name = "auth",                description = "Authentication and session management"),
        (name = "users",               description = "User profile and account management"),
        (name = "businesses",          description = "Partner-business registration and lookup"),
        (name = "beacons",             description = "BLE beacon configuration and key rotation"),
        (name = "presence",            description = "Presence proof events (BFIP §5)"),
        (name = "identity",            description = "Identity verification + cooling period"),
        (name = "background-checks",   description = "Background-check lifecycle"),
        (name = "staff",               description = "Operational staff roles, visits, evidence"),
        (name = "attestations",        description = "Staff attestations + reviewer co-signing (BFIP §6)"),
        (name = "soultokens",          description = "Soultoken issuance, renewal, revocation (BFIP §7)"),
        (name = "orders",              description = "Strawberry orders + NFC box collection (BFIP §9)"),
        (name = "support",             description = "Support bookings + platform-gift flow"),
        (name = "attestation-tokens",  description = "Short-lived third-party verification tokens (BFIP §11)"),
        (name = "verification",        description = "Verification-event audit trail (BFIP §17)"),
        (name = "platform-config",     description = "Platform configuration + feature flags (BFIP §15, admin)"),
        (name = "analytics",           description = "Internal product analytics (admin)"),
        (name = "notifications",       description = "Server-Sent Events real-time stream"),
        (name = "dorotka",             description = "Dorotka AI assistant"),
    ),
)]
pub struct ApiDoc;

/// Build the derived OpenAPI document.
pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}

/// Serve the OpenAPI 3.1 JSON spec.
pub async fn openapi_json(_state: State<AppState>) -> Json<utoipa::openapi::OpenApi> {
    Json(openapi())
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
