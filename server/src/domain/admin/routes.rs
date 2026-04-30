/// Admin â€” operator-authenticated endpoints for shop management.
///
/// Security model:
///   - All routes require a valid user JWT (RequireUser) PLUS a PIN verified
///     at request time via constant-time comparison. The PIN is never stored in
///     the JWT â€” it must be re-supplied on every admin request.
///   - Admin PIN â†’ full access.
///   - Chocolatier PIN â†’ catalog and order management only.
///   - Supplier PIN â†’ read-only order list.
///   - PINs are hashed with bcrypt in production; the config holds the hash.
///     For the MVP the raw value is compared constant-time to avoid timing leaks.
use axum::{
    extract::{Path, Query, State},
    routing::{get, patch, post},
    Json, Router,
};
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use secrecy::ExposeSecret;

use crate::{
    app::AppState,
    error::{AppError, AppResult},
    http::extractors::auth::RequireUser,
    types::{OrderId, UserId},
};

pub fn router() -> Router<AppState> {
    Router::new()
        // Orders
        .route("/api/admin/orders",              get(list_orders))
        .route("/api/admin/orders/{id}/status",   patch(set_order_status))
        .route("/api/admin/orders/{id}/assign",   post(assign_worker))
        // NFC
        .route("/api/admin/nfc/verify/{device_id}", post(admin_nfc_verify))
        // Users
        .route("/api/admin/users",               get(list_users))
        .route("/api/admin/users/{id}/tier",      patch(set_user_tier))
        // Catalog
        .route("/api/admin/varieties",           get(list_varieties_admin))
        .route("/api/admin/varieties/{id}/stock", patch(set_stock))
        // Businesses
        .route("/api/admin/businesses",                       get(list_businesses_admin))
        .route("/api/admin/businesses/{id}/verify",           post(verify_business))
        .route("/api/admin/businesses/{id}/loyalty-config",   post(set_loyalty_config))
}

// â”€â”€ PIN extraction â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Every admin request must include X-Admin-Pin header.
/// On success, emits a structured audit event (method + path + role).
fn require_admin_pin(
    headers: &axum::http::HeaderMap,
    cfg:     &crate::config::Config,
    method:  &str,
    path:    &str,
) -> AppResult<AdminRole> {
    let pin = headers
        .get("x-admin-pin")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::bad_request("X-Admin-Pin header required"))?;

    let role = if constant_time_eq(cfg.admin_pin.expose_secret().as_bytes(), pin.as_bytes()) {
        AdminRole::Admin
    } else if constant_time_eq(cfg.chocolatier_pin.expose_secret().as_bytes(), pin.as_bytes()) {
        AdminRole::Chocolatier
    } else if constant_time_eq(cfg.supplier_pin.expose_secret().as_bytes(), pin.as_bytes()) {
        AdminRole::Supplier
    } else {
        // Log failed attempts — useful for detecting brute-force
        tracing::warn!(method, path, "admin PIN rejected");
        return Err(AppError::Forbidden);
    };

    // Structured audit event — role is logged, PIN is never logged.
    tracing::info!(method, path, role = ?role, "admin action authorised");

    Ok(role)
}

/// Constant-time secret comparison that eliminates length-based timing side-channels.
///
/// `ring::constant_time::verify_slices_are_equal` is constant-time for equal-length
/// inputs but returns immediately when lengths differ, leaking PIN length.
/// HMAC-normalising both inputs (same random key → fixed 32-byte MACs) removes
/// that oracle: the final comparison is always between 32-byte values.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    use ring::{
        hmac::{self, Key, HMAC_SHA256},
        rand::{SecureRandom, SystemRandom},
    };
    let rng = SystemRandom::new();
    let mut key_bytes = [0u8; 32];
    // Fill can only fail if the OS entropy source fails — treat that as non-equal.
    if rng.fill(&mut key_bytes).is_err() {
        return false;
    }
    let key   = Key::new(HMAC_SHA256, &key_bytes);
    let mac_a = hmac::sign(&key, a);
    let mac_b = hmac::sign(&key, b);
    // Both MACs are 32 bytes — XOR-accumulate is provably constant-time for equal-length slices.
    mac_a.as_ref().iter().zip(mac_b.as_ref()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum AdminRole {
    Supplier,
    Chocolatier,
    Admin,
}

// â”€â”€ Order management â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[derive(Debug, Deserialize)]
struct OrderFilter {
    status: Option<String>,
    limit:  Option<i64>,
}

#[derive(Debug, sqlx::FromRow, Serialize)]
struct AdminOrderRow {
    id:           OrderId,
    user_id:      UserId,
    variety_id:   i32,
    variety_name: String,
    quantity:     i32,
    total_cents:  i64,
    status:       String,
    worker_id:    Option<i32>,
    slot_time:    Option<String>,
    created_at:   NaiveDateTime,
}

async fn list_orders(
    State(state): State<AppState>,
    RequireUser(_user_id): RequireUser,
    Query(filter): Query<OrderFilter>,
    method: axum::http::Method,
    uri: axum::http::Uri,
    headers: axum::http::HeaderMap,
) -> AppResult<Json<Vec<AdminOrderRow>>> {
    let role = require_admin_pin(&headers, &state.cfg, method.as_str(), uri.path())?;
    // Suppliers and above can read orders.
    let _ = role;

    let limit = filter.limit.unwrap_or(100).min(500);

    let rows: Vec<AdminOrderRow> = sqlx::query_as(
        "SELECT o.id, o.user_id, o.variety_id, v.name AS variety_name,
                o.quantity, o.total_cents, o.status, o.worker_id,
                o.slot_time, o.created_at
         FROM orders o
         JOIN catalog_varieties v ON v.id = o.variety_id
         WHERE ($1::text IS NULL OR o.status = $1)
         ORDER BY o.created_at DESC
         LIMIT $2",
    )
    .bind(filter.status.as_deref())
    .bind(limit)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Db)?;

    Ok(Json(rows))
}

#[derive(Deserialize)]
struct SetStatusBody {
    status: String,
}

const VALID_ORDER_STATUSES: &[&str] = &["pending", "confirmed", "preparing", "ready", "collected", "cancelled"];

async fn set_order_status(
    State(state): State<AppState>,
    RequireUser(_user_id): RequireUser,
    Path(order_id): Path<OrderId>,
    method: axum::http::Method,
    uri: axum::http::Uri,
    headers: axum::http::HeaderMap,
    Json(body): Json<SetStatusBody>,
) -> AppResult<Json<serde_json::Value>> {
    let role = require_admin_pin(&headers, &state.cfg, method.as_str(), uri.path())?;
    if role < AdminRole::Chocolatier {
        return Err(AppError::Forbidden);
    }

    if !VALID_ORDER_STATUSES.contains(&body.status.as_str()) {
        return Err(AppError::bad_request("invalid status value"));
    }

    let result = sqlx::query(
        "UPDATE orders SET status = $1 WHERE id = $2",
    )
    .bind(&body.status)
    .bind(order_id)
    .execute(&state.db)
    .await
    .map_err(AppError::Db)?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(Json(serde_json::json!({ "status": body.status })))
}

#[derive(Deserialize)]
struct AssignWorkerBody {
    worker_id: i32,
}

async fn assign_worker(
    State(state): State<AppState>,
    RequireUser(_user_id): RequireUser,
    Path(order_id): Path<OrderId>,
    method: axum::http::Method,
    uri: axum::http::Uri,
    headers: axum::http::HeaderMap,
    Json(body): Json<AssignWorkerBody>,
) -> AppResult<Json<serde_json::Value>> {
    let role = require_admin_pin(&headers, &state.cfg, method.as_str(), uri.path())?;
    if role < AdminRole::Chocolatier {
        return Err(AppError::Forbidden);
    }

    let result = sqlx::query(
        "UPDATE orders SET worker_id = $1 WHERE id = $2",
    )
    .bind(body.worker_id)
    .bind(order_id)
    .execute(&state.db)
    .await
    .map_err(AppError::Db)?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(Json(serde_json::json!({ "worker_id": body.worker_id })))
}

// â”€â”€ NFC verification â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Admin-level NFC verification â€” marks a device as verified by a shop operator
/// who has physically inspected the NFC tag.
async fn admin_nfc_verify(
    State(state): State<AppState>,
    RequireUser(_user_id): RequireUser,
    Path(device_id): Path<i32>,
    method: axum::http::Method,
    uri: axum::http::Uri,
    headers: axum::http::HeaderMap,
) -> AppResult<Json<serde_json::Value>> {
    let role = require_admin_pin(&headers, &state.cfg, method.as_str(), uri.path())?;
    if role < AdminRole::Admin {
        return Err(AppError::Forbidden);
    }

    let result = sqlx::query(
        "UPDATE device_attestations
         SET nfc_verified = true, nfc_verified_at = NOW()
         WHERE id = $1",
    )
    .bind(device_id)
    .execute(&state.db)
    .await
    .map_err(AppError::Db)?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(Json(serde_json::json!({ "nfc_verified": true })))
}

// â”€â”€ User management â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[derive(Debug, Deserialize)]
struct UserFilter {
    q:     Option<String>,
    limit: Option<i64>,
}

#[derive(Debug, sqlx::FromRow, Serialize)]
struct AdminUserRow {
    id:                i32,
    email:             Option<String>,
    display_name:      Option<String>,
    tier:              Option<String>,
    identity_verified: bool,
    created_at:        NaiveDateTime,
}

async fn list_users(
    State(state): State<AppState>,
    RequireUser(_user_id): RequireUser,
    Query(filter): Query<UserFilter>,
    method: axum::http::Method,
    uri: axum::http::Uri,
    headers: axum::http::HeaderMap,
) -> AppResult<Json<Vec<AdminUserRow>>> {
    let role = require_admin_pin(&headers, &state.cfg, method.as_str(), uri.path())?;
    if role < AdminRole::Admin {
        return Err(AppError::Forbidden);
    }

    let limit = filter.limit.unwrap_or(100).min(500);
    let q = filter.q.as_deref().map(|s| format!("%{s}%"));

    let rows: Vec<AdminUserRow> = sqlx::query_as(
        "SELECT id, email, display_name, tier, identity_verified, created_at
         FROM users
         WHERE ($1::text IS NULL OR email ILIKE $1 OR display_name ILIKE $1)
         ORDER BY created_at DESC
         LIMIT $2",
    )
    .bind(q.as_deref())
    .bind(limit)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Db)?;

    Ok(Json(rows))
}

#[derive(Deserialize)]
struct SetTierBody {
    tier: String,
}

const VALID_TIERS: &[&str] = &["explorer", "maison", "reserve", "atelier", "distillery", "commune"];

async fn set_user_tier(
    State(state): State<AppState>,
    RequireUser(_user_id): RequireUser,
    Path(target_id): Path<i32>,
    method: axum::http::Method,
    uri: axum::http::Uri,
    headers: axum::http::HeaderMap,
    Json(body): Json<SetTierBody>,
) -> AppResult<Json<serde_json::Value>> {
    let role = require_admin_pin(&headers, &state.cfg, method.as_str(), uri.path())?;
    if role < AdminRole::Admin {
        return Err(AppError::Forbidden);
    }

    if !VALID_TIERS.contains(&body.tier.as_str()) {
        return Err(AppError::bad_request("invalid tier value"));
    }

    let result = sqlx::query("UPDATE users SET tier = $1 WHERE id = $2")
        .bind(&body.tier)
        .bind(target_id)
        .execute(&state.db)
        .await
        .map_err(AppError::Db)?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(Json(serde_json::json!({ "tier": body.tier })))
}

// â”€â”€ Catalog management â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[derive(Debug, sqlx::FromRow, Serialize)]
struct AdminVarietyRow {
    id:           i32,
    name:         String,
    price_cents:  i64,
    stock:        i32,
    available:    bool,
    business_id:  Option<i32>,
}

async fn list_varieties_admin(
    State(state): State<AppState>,
    RequireUser(_user_id): RequireUser,
    method: axum::http::Method,
    uri: axum::http::Uri,
    headers: axum::http::HeaderMap,
) -> AppResult<Json<Vec<AdminVarietyRow>>> {
    let role = require_admin_pin(&headers, &state.cfg, method.as_str(), uri.path())?;
    if role < AdminRole::Chocolatier {
        return Err(AppError::Forbidden);
    }

    let rows: Vec<AdminVarietyRow> = sqlx::query_as(
        "SELECT id, name, price_cents, stock, available, business_id
         FROM catalog_varieties
         ORDER BY name ASC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Db)?;

    Ok(Json(rows))
}

#[derive(Deserialize)]
struct SetStockBody {
    stock: i32,
}

async fn set_stock(
    State(state): State<AppState>,
    RequireUser(_user_id): RequireUser,
    Path(variety_id): Path<i32>,
    method: axum::http::Method,
    uri: axum::http::Uri,
    headers: axum::http::HeaderMap,
    Json(body): Json<SetStockBody>,
) -> AppResult<Json<serde_json::Value>> {
    let role = require_admin_pin(&headers, &state.cfg, method.as_str(), uri.path())?;
    if role < AdminRole::Chocolatier {
        return Err(AppError::Forbidden);
    }

    if body.stock < 0 {
        return Err(AppError::bad_request("stock cannot be negative"));
    }

    let result = sqlx::query(
        "UPDATE catalog_varieties SET stock = $1 WHERE id = $2",
    )
    .bind(body.stock)
    .bind(variety_id)
    .execute(&state.db)
    .await
    .map_err(AppError::Db)?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(Json(serde_json::json!({ "stock": body.stock })))
}

// â”€â”€ Business verification â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[derive(Debug, sqlx::FromRow, Serialize)]
struct AdminBusinessRow {
    id:       i32,
    name:     String,
    verified: bool,
    owner_id: i32,
    created_at: NaiveDateTime,
}

async fn list_businesses_admin(
    State(state): State<AppState>,
    RequireUser(_user_id): RequireUser,
    method: axum::http::Method,
    uri: axum::http::Uri,
    headers: axum::http::HeaderMap,
) -> AppResult<Json<Vec<AdminBusinessRow>>> {
    let role = require_admin_pin(&headers, &state.cfg, method.as_str(), uri.path())?;
    if role < AdminRole::Admin {
        return Err(AppError::Forbidden);
    }

    let rows: Vec<AdminBusinessRow> = sqlx::query_as(
        "SELECT id, name, verified, owner_id, created_at
         FROM businesses
         ORDER BY created_at DESC
         LIMIT 200",
    )
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Db)?;

    Ok(Json(rows))
}

async fn verify_business(
    State(state): State<AppState>,
    RequireUser(_user_id): RequireUser,
    Path(business_id): Path<i32>,
    method: axum::http::Method,
    uri: axum::http::Uri,
    headers: axum::http::HeaderMap,
) -> AppResult<Json<serde_json::Value>> {
    let role = require_admin_pin(&headers, &state.cfg, method.as_str(), uri.path())?;
    if role < AdminRole::Admin {
        return Err(AppError::Forbidden);
    }

    let result = sqlx::query(
        "UPDATE businesses SET verified = true WHERE id = $1",
    )
    .bind(business_id)
    .execute(&state.db)
    .await
    .map_err(AppError::Db)?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(Json(serde_json::json!({ "verified": true })))
}

// ── Loyalty configuration ─────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SetLoyaltyConfigBody {
    steeps_per_reward:  i32,
    reward_description: String,
}

/// Creates or replaces the loyalty programme configuration for a business.
/// Required before any customer can earn steeps at that business.
async fn set_loyalty_config(
    State(state):      State<AppState>,
    RequireUser(_uid): RequireUser,
    Path(business_id): Path<i32>,
    method:            axum::http::Method,
    uri:               axum::http::Uri,
    headers:           axum::http::HeaderMap,
    Json(body):        Json<SetLoyaltyConfigBody>,
) -> AppResult<Json<serde_json::Value>> {
    let role = require_admin_pin(&headers, &state.cfg, method.as_str(), uri.path())?;
    if role < AdminRole::Admin {
        return Err(AppError::Forbidden);
    }

    if body.steeps_per_reward < 1 || body.steeps_per_reward > 100 {
        return Err(AppError::bad_request("steeps_per_reward must be between 1 and 100"));
    }
    let description = body.reward_description.trim();
    if description.is_empty() {
        return Err(AppError::bad_request("reward_description cannot be empty"));
    }

    sqlx::query(
        "INSERT INTO business_loyalty_config (business_id, steeps_per_reward, reward_description)
         VALUES ($1, $2, $3)
         ON CONFLICT (business_id) DO UPDATE
             SET steeps_per_reward  = EXCLUDED.steeps_per_reward,
                 reward_description = EXCLUDED.reward_description,
                 updated_at         = now()"
    )
    .bind(business_id)
    .bind(body.steeps_per_reward)
    .bind(description)
    .execute(&state.db)
    .await
    .map_err(AppError::Db)?;

    Ok(Json(serde_json::json!({
        "business_id":        business_id,
        "steeps_per_reward":  body.steeps_per_reward,
        "reward_description": description,
    })))
}
