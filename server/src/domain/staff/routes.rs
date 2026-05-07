use std::sync::Arc;

use axum::{
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use box_fraise_integrations::storage::{evidence_path, StorageClient};
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;

use crate::{
    app::AppState,
    error::{AppError, AppResult},
    http::extractors::{auth::RequireUser, json::AppJson},
};
use box_fraise_domain::types::UserId;
use super::{service, types::*};

const MAX_EVIDENCE_BYTES: usize = 50 * 1024 * 1024; // 50 MB

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/staff/roles",                          post(grant_role))
        .route("/api/staff/roles/me",                       get(my_roles))
        .route("/api/staff/visits",                         post(schedule_visit).get(list_visits))
        .route("/api/staff/visits/{id}/arrive",              post(arrive))
        .route("/api/staff/visits/{id}/complete",            post(complete))
        .route("/api/staff/visits/{id}/quality-assessment",  post(quality_assessment))
        .route("/api/staff/visits/{id}/evidence",            post(upload_evidence))
        .route("/api/staff/visits/{id}/evidence/url",        get(evidence_url))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// POST /api/staff/roles
///
/// Grant a staff role to a user. Requires platform_admin.
async fn grant_role(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body):        AppJson<GrantRoleRequest>,
) -> AppResult<(StatusCode, Json<StaffRoleResponse>)> {
    let resp = service::grant_staff_role(&state.db, user_id, body, &state.event_bus).await?;
    Ok((StatusCode::CREATED, Json(resp)))
}

/// GET /api/staff/roles/me
///
/// List the authenticated user's own active staff roles.
async fn my_roles(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<Vec<StaffRoleResponse>>> {
    Ok(Json(service::get_my_roles(&state.db, user_id).await?))
}

/// POST /api/staff/visits
///
/// Schedule a new staff visit. Requires delivery_staff or platform_admin.
async fn schedule_visit(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body):        AppJson<ScheduleVisitRequest>,
) -> AppResult<(StatusCode, Json<StaffVisitResponse>)> {
    let resp = service::schedule_visit(&state.db, user_id, body, &state.event_bus).await?;
    Ok((StatusCode::CREATED, Json(resp)))
}

/// GET /api/staff/visits
///
/// List visits. Platform admins see all; delivery staff see only their own.
async fn list_visits(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<Vec<StaffVisitResponse>>> {
    Ok(Json(service::list_visits(&state.db, user_id).await?))
}

/// POST /api/staff/visits/:id/arrive
///
/// Record arrival at a scheduled visit — sets status to in_progress.
/// Only the assigned staff member may call this.
async fn arrive(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(visit_id):       Path<i32>,
    AppJson(body):        AppJson<ArriveAtVisitRequest>,
) -> AppResult<Json<StaffVisitResponse>> {
    Ok(Json(service::arrive_at_visit(&state.db, visit_id, user_id, body).await?))
}

/// POST /api/staff/visits/:id/complete
///
/// Mark a visit completed with box count and evidence.
/// Only the assigned staff member or a platform_admin may call this.
async fn complete(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(visit_id):       Path<i32>,
    AppJson(body):        AppJson<CompleteVisitRequest>,
) -> AppResult<Json<StaffVisitResponse>> {
    Ok(Json(service::complete_visit(&state.db, visit_id, user_id, body, &state.event_bus).await?))
}

/// POST /api/staff/visits/:id/quality-assessment
///
/// Submit a quality assessment for a business during a staff visit.
/// Returns 201 on success.
async fn quality_assessment(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(visit_id):       Path<i32>,
    AppJson(body):        AppJson<QualityAssessmentRequest>,
) -> AppResult<(StatusCode, Json<QualityAssessmentRow>)> {
    let resp = service::submit_quality_assessment(
        &state.db, visit_id, user_id, body, &state.event_bus,
    ).await?;
    Ok((StatusCode::CREATED, Json(resp)))
}

// ── Evidence upload + presign (Hardening §3) ──────────────────────────────────

#[derive(Deserialize)]
struct EvidenceUrlQuery {
    path: String,
}

/// POST /api/staff/visits/:id/evidence
///
/// Multipart upload of the evidence package for a visit. Required fields:
/// - `file`           — binary payload (≤ 50 MB)
/// - `evidence_type`  — one of `photo` | `gps` | `combined`
///
/// The server SHA-256s the bytes itself; the returned `evidence_hash` is the
/// canonical replacement for the client-supplied hash that
/// `complete_visit` previously trusted.
async fn upload_evidence(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(visit_id):       Path<i32>,
    mut multipart:        Multipart,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    let storage = require_storage(&state)?;
    require_visit_writer(&state.db, visit_id, user_id).await?;

    let mut file_bytes:    Option<Vec<u8>> = None;
    let mut content_type:  Option<String>  = None;
    let mut evidence_type: Option<String>  = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::bad_request(format!("multipart error: {e}")))?
    {
        match field.name().unwrap_or("") {
            "file" => {
                content_type = field.content_type().map(|s| s.to_string());
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::bad_request(format!("multipart file read failed: {e}")))?;
                if bytes.len() > MAX_EVIDENCE_BYTES {
                    return Err(AppError::bad_request(format!(
                        "file exceeds {} MB limit",
                        MAX_EVIDENCE_BYTES / (1024 * 1024)
                    )));
                }
                file_bytes = Some(bytes.to_vec());
            }
            "evidence_type" => {
                evidence_type = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| AppError::bad_request(format!("evidence_type read failed: {e}")))?,
                );
            }
            _ => { /* ignore unknown fields */ }
        }
    }

    let bytes = file_bytes.ok_or_else(|| AppError::bad_request("missing 'file' field"))?;
    let etype = evidence_type.ok_or_else(|| AppError::bad_request("missing 'evidence_type' field"))?;
    if !["photo", "gps", "combined"].contains(&etype.as_str()) {
        return Err(AppError::bad_request("evidence_type must be photo|gps|combined"));
    }
    let ctype = content_type.unwrap_or_else(|| "application/octet-stream".to_string());

    let evidence_hash = StorageClient::compute_evidence_hash(&bytes);
    let timestamp     = chrono::Utc::now().timestamp();
    let path          = evidence_path(visit_id, timestamp, &etype);
    let size_bytes    = bytes.len();

    storage
        .upload(&path, bytes, &ctype)
        .await
        .map_err(|e| AppError::BadGateway(e.to_string()))?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "path":          path,
            "evidence_hash": evidence_hash,
            "content_type":  ctype,
            "size_bytes":    size_bytes,
        })),
    ))
}

/// GET /api/staff/visits/:id/evidence/url?path=…
///
/// Issue a 15-minute presigned `GET` URL so an assigned reviewer can view
/// the previously-uploaded evidence object. Auth: assigned staff,
/// platform_admin, or any reviewer on an attestation tied to this visit.
async fn evidence_url(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(visit_id):       Path<i32>,
    Query(q):             Query<EvidenceUrlQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let storage = require_storage(&state)?;
    require_visit_reader(&state.db, visit_id, user_id).await?;

    let url = storage
        .presigned_url(&q.path, 0)
        .await
        .map_err(|e| AppError::BadGateway(e.to_string()))?;
    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(900);
    Ok(Json(json!({
        "url":        url,
        "expires_at": expires_at.to_rfc3339(),
    })))
}

// ── Authorization helpers (storage routes) ────────────────────────────────────

fn require_storage(state: &AppState) -> Result<Arc<StorageClient>, AppError> {
    state
        .storage_client
        .clone()
        .ok_or_else(|| AppError::ServiceUnavailable(
            "storage feature disabled — SPACES_* not configured".to_string(),
        ))
}

/// Writer: only the assigned staff or a platform admin may upload evidence.
async fn require_visit_writer(pool: &PgPool, visit_id: i32, user_id: UserId) -> Result<(), AppError> {
    let uid = i32::from(user_id);
    let row: Option<(i32,)> = sqlx::query_as("SELECT staff_id FROM staff_visits WHERE id = $1")
        .bind(visit_id)
        .fetch_optional(pool)
        .await
        .map_err(AppError::Db)?;
    let staff_id = row.ok_or(AppError::NotFound)?.0;
    if staff_id == uid {
        return Ok(());
    }
    let is_admin: Option<bool> = sqlx::query_scalar("SELECT is_platform_admin FROM users WHERE id = $1")
        .bind(uid)
        .fetch_optional(pool)
        .await
        .map_err(AppError::Db)?;
    if matches!(is_admin, Some(true)) {
        return Ok(());
    }
    Err(AppError::Forbidden)
}

/// Reader: assigned staff, platform admin, or any reviewer on an attestation
/// tied to this visit.
async fn require_visit_reader(pool: &PgPool, visit_id: i32, user_id: UserId) -> Result<(), AppError> {
    let uid = i32::from(user_id);
    let row: Option<(i32,)> = sqlx::query_as("SELECT staff_id FROM staff_visits WHERE id = $1")
        .bind(visit_id)
        .fetch_optional(pool)
        .await
        .map_err(AppError::Db)?;
    let staff_id = row.ok_or(AppError::NotFound)?.0;
    if staff_id == uid {
        return Ok(());
    }
    let is_admin: Option<bool> = sqlx::query_scalar("SELECT is_platform_admin FROM users WHERE id = $1")
        .bind(uid)
        .fetch_optional(pool)
        .await
        .map_err(AppError::Db)?;
    if matches!(is_admin, Some(true)) {
        return Ok(());
    }
    let reviewer_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM visit_attestations \
         WHERE visit_id = $1 \
           AND (assigned_reviewer_1_id = $2 OR assigned_reviewer_2_id = $2)"
    )
    .bind(visit_id)
    .bind(uid)
    .fetch_one(pool)
    .await
    .map_err(AppError::Db)?;
    if reviewer_count > 0 {
        return Ok(());
    }
    Err(AppError::Forbidden)
}
