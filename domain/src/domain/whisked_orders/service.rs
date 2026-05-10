//! Whisked orders service — placement, lookup, validate-pickup,
//! status transitions for the staff dashboard.

use sqlx::PgPool;

use crate::{
    audit,
    error::{AppResult, DomainError},
    transaction::RlsTransaction,
};
use crate::domain::whisked_menu::repository as menu_repo;
use super::{
    repository,
    types::{
        OrderItemRequest, PlaceOrderRequest, ValidatePickupRequest,
        WhiskedOrderItemRow, WhiskedOrderResponse, WhiskedOrderRow,
    },
};

// ── Pickup code generation ───────────────────────────────────────────────────

const PICKUP_CODE_ALPHABET: &[u8] = b"23456789ABCDEFGHJKMNPQRSTUVWXYZ";
const PICKUP_CODE_LEN:      usize = 4;
const PICKUP_CODE_RETRIES:  usize = 10;

fn generate_pickup_code() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let chars: String = (0..PICKUP_CODE_LEN)
        .map(|_| {
            let idx = rng.gen_range(0..PICKUP_CODE_ALPHABET.len());
            PICKUP_CODE_ALPHABET[idx] as char
        })
        .collect();
    format!("W-{chars}")
}

fn is_unique_violation(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(db) if db.code().as_deref() == Some("23505"))
}

// ── Placement ────────────────────────────────────────────────────────────────

/// Place a new Whisked order. Validates every line item against
/// `whisked_menu_items`, calls Stripe to create a `PaymentIntent` for the
/// total, mints a unique pickup code, persists the order plus its line
/// items in the supplied transaction, and writes a `whisked.order_placed`
/// audit row on `pool` (outside the transaction so the audit lands even
/// if the caller rolls back later).
///
/// `stripe_secret_key` is read from `Config.stripe_secret_key`; passing an
/// empty string short-circuits the Stripe call so unit tests can run
/// without a live key. The PaymentIntent is created with
/// `capture_method = manual` (Stripe's default in this codebase): the card
/// is authorized at order placement and captured later when the order
/// transitions to `collected` (capture-on-collect is a follow-up).
pub async fn place_order(
    tx:                &mut RlsTransaction,
    pool:              &PgPool,
    http:              &reqwest::Client,
    user_id:           i32,
    req:               PlaceOrderRequest,
    stripe_secret_key: &str,
) -> AppResult<WhiskedOrderResponse> {
    // 1. Validate request shape early.
    if req.items.is_empty() {
        return Err(DomainError::invalid_input("cart is empty"));
    }
    for item in &req.items {
        if item.quantity < 1 {
            return Err(DomainError::invalid_input("quantity must be >= 1"));
        }
    }

    // 2. Verify the business exists.
    let biz_exists: Option<bool> = sqlx::query_scalar(
        "SELECT true FROM businesses WHERE id = $1"
    )
    .bind(req.business_id)
    .fetch_optional(tx.as_mut())
    .await
    .map_err(DomainError::Db)?;
    if biz_exists.is_none() {
        return Err(DomainError::NotFound);
    }

    // 3. Fetch every requested menu item; reject if any are missing or
    //    unavailable.
    let ids: Vec<i32> = req.items.iter().map(|i| i.menu_item_id).collect();
    let menu_rows = menu_repo::list_by_ids(tx.as_mut(), &ids).await?;
    if menu_rows.len() != distinct_count(&ids) {
        return Err(DomainError::invalid_input(
            "one or more menu items not found",
        ));
    }
    for m in &menu_rows {
        if !m.available {
            return Err(DomainError::invalid_input(format!(
                "menu item {} is unavailable",
                m.id
            )));
        }
    }

    // 4. Calculate the total. i64 throughout, then cast back to i32 with
    //    a bounds check so a runaway request can't insert a negative total.
    let total_cents = sum_total_cents(&req.items, &menu_rows)?;

    // 5. Create a Stripe PaymentIntent if a key is configured. An empty key
    //    means we're running without Stripe (dev / unit tests) — the order
    //    persists with NULL `stripe_payment_intent_id` and the response
    //    `stripe_client_secret` is `None`.
    let (stripe_pi_id, stripe_client_secret) =
        create_payment_intent_if_configured(http, stripe_secret_key, total_cents, user_id).await?;

    // 6. Mint a pickup code, retry on the (rare) unique-violation collision.
    let order_row = mint_order(tx, user_id, req.business_id, total_cents, stripe_pi_id.as_deref()).await?;

    // 7. Insert line items.
    let mut item_rows: Vec<WhiskedOrderItemRow> = Vec::with_capacity(req.items.len());
    for req_item in &req.items {
        let menu = menu_rows
            .iter()
            .find(|m| m.id == req_item.menu_item_id)
            .expect("validated above");
        let row = repository::create_order_item(
            tx.as_mut(),
            order_row.id,
            req_item.menu_item_id,
            req_item.quantity,
            menu.price_cents,
        )
        .await
        .map_err(DomainError::Db)?;
        item_rows.push(row);
    }

    // 8. Audit (separate connection — survives rollback).
    audit::write(
        pool,
        Some(user_id),
        None,
        "whisked.order_placed",
        serde_json::json!({
            "order_id":                 order_row.id,
            "business_id":              req.business_id,
            "total_cents":              total_cents,
            "item_count":               item_rows.len(),
            "stripe_payment_intent_id": order_row.stripe_payment_intent_id,
        }),
    )
    .await;

    Ok(WhiskedOrderResponse {
        pickup_code:          order_row.pickup_code.clone(),
        order:                order_row,
        items:                item_rows,
        stripe_client_secret,
    })
}

/// Build a PaymentIntent when a Stripe key is supplied; return `(None, None)`
/// otherwise so callers can persist `NULL` in `stripe_payment_intent_id` and
/// surface `None` for `stripe_client_secret` in the response. Stripe failures
/// surface as `ExternalServiceError` with a generic message — the underlying
/// error is logged at warn level.
async fn create_payment_intent_if_configured(
    http:              &reqwest::Client,
    stripe_secret_key: &str,
    total_cents:       i32,
    user_id:           i32,
) -> AppResult<(Option<String>, Option<String>)> {
    if stripe_secret_key.is_empty() {
        return Ok((None, None));
    }
    let user_id_str = user_id.to_string();
    let metadata: [(&str, &str); 2] = [
        ("metadata[order_type]", "whisked"),
        ("metadata[user_id]",    user_id_str.as_str()),
    ];
    let client = box_fraise_integrations::stripe::StripeClient::new(stripe_secret_key, http);
    let pi = client
        .create_payment_intent(total_cents as i64, "cad", None, &metadata)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "stripe.create_payment_intent failed for whisked order");
            DomainError::ExternalServiceError("Failed to create payment intent".to_string())
        })?;
    Ok((Some(pi.id), pi.client_secret))
}

async fn mint_order(
    tx:                       &mut RlsTransaction,
    user_id:                  i32,
    business_id:              i32,
    total_cents:              i32,
    stripe_payment_intent_id: Option<&str>,
) -> AppResult<WhiskedOrderRow> {
    for _ in 0..PICKUP_CODE_RETRIES {
        let code = generate_pickup_code();
        match repository::create_order(
            tx.as_mut(), user_id, business_id, total_cents, &code, stripe_payment_intent_id,
        ).await {
            Ok(row)                                 => return Ok(row),
            Err(e) if is_unique_violation(&e)       => continue,
            Err(e)                                  => return Err(DomainError::Db(e)),
        }
    }
    Err(DomainError::Internal(anyhow::anyhow!(
        "failed to mint a unique pickup code after {PICKUP_CODE_RETRIES} attempts"
    )))
}

fn sum_total_cents(
    items:     &[OrderItemRequest],
    menu_rows: &[crate::domain::whisked_menu::types::WhiskedMenuItemRow],
) -> AppResult<i32> {
    let mut total: i64 = 0;
    for req_item in items {
        let menu = menu_rows
            .iter()
            .find(|m| m.id == req_item.menu_item_id)
            .ok_or_else(|| DomainError::invalid_input("missing menu item"))?;
        total = total
            .checked_add(menu.price_cents as i64 * req_item.quantity as i64)
            .ok_or_else(|| DomainError::invalid_input("order total overflow"))?;
    }
    if total <= 0 || total > i32::MAX as i64 {
        return Err(DomainError::invalid_input("order total out of range"));
    }
    Ok(total as i32)
}

fn distinct_count(ids: &[i32]) -> usize {
    let mut seen = std::collections::HashSet::new();
    ids.iter().filter(|i| seen.insert(**i)).count()
}

// ── Lookup ───────────────────────────────────────────────────────────────────

/// Fetch a single Whisked order plus its line items. Caller must own the
/// order (Whisked surface has no shared visibility — staff use the dedicated
/// dashboard endpoints instead).
pub async fn get_order(
    tx:       &mut RlsTransaction,
    _pool:    &PgPool,
    user_id:  i32,
    order_id: i32,
) -> AppResult<WhiskedOrderResponse> {
    let order = repository::get_by_id(tx.as_mut(), order_id)
        .await?
        .ok_or(DomainError::NotFound)?;
    if order.user_id != user_id {
        return Err(DomainError::Forbidden);
    }
    let items = repository::list_items_for_order(tx.as_mut(), order_id).await?;
    Ok(WhiskedOrderResponse {
        pickup_code:          order.pickup_code.clone(),
        order,
        items,
        // Returned only at placement time. The iOS client caches the
        // client_secret locally for the in-flight order; subsequent fetches
        // don't re-issue the secret.
        stripe_client_secret: None,
    })
}

// ── Pickup validation (staff path) ───────────────────────────────────────────

/// Atomically consume the pickup code and mark the order collected.
///
/// Pre-checks return specific errors so the iOS staff app can surface
/// distinct UX states (not-ready, wrong code, already-used). The final
/// `UPDATE … WHERE pickup_code_used_at IS NULL AND status = 'ready'` is the
/// race-safe step — if a concurrent request consumed the code between our
/// pre-check and the update, we surface `Conflict` rather than a stale 200.
pub async fn validate_pickup(
    pool:             &PgPool,
    business_user_id: i32,
    order_id:         i32,
    pickup_code:      &str,
) -> AppResult<WhiskedOrderRow> {
    let mut conn = pool.acquire().await.map_err(DomainError::Db)?;

    let _biz = repository::assert_business_owner(&mut conn, order_id, business_user_id).await?;

    let order = repository::get_by_id(&mut conn, order_id)
        .await?
        .ok_or(DomainError::NotFound)?;

    // Check ordering matters when an order has already been collected — its
    // status has moved past `ready`. We surface "already used" (Conflict)
    // for double-validate rather than "not ready" (InvalidInput) so the
    // staff app can distinguish a re-scan from a too-early scan.
    if order.pickup_code != pickup_code {
        return Err(DomainError::Forbidden);
    }
    if order.pickup_code_used_at.is_some() {
        return Err(DomainError::conflict("Pickup code already used"));
    }
    if order.status != "ready" {
        return Err(DomainError::invalid_input(
            "Order is not ready for pickup",
        ));
    }

    let updated = repository::collect_with_pickup_code(&mut conn, order_id, pickup_code).await?;

    let row = updated.ok_or_else(|| {
        // The pre-checks all passed but the atomic UPDATE matched zero rows
        // — another request consumed the code between our SELECT and the
        // UPDATE. Surface that as a fresh Conflict.
        DomainError::conflict("Pickup code already used")
    })?;

    audit::write(
        pool,
        Some(business_user_id),
        None,
        "whisked.order_collected",
        serde_json::json!({
            "order_id":    order_id,
            "business_id": row.business_id,
            "user_id":     row.user_id,
        }),
    )
    .await;

    Ok(row)
}

// ── Staff status transitions ─────────────────────────────────────────────────

/// Move an order along the prep timeline: `pending → preparing → ready`.
/// `collected` is only set via `validate_pickup`; `cancelled` lands on a
/// separate (future) cancel endpoint. Caller must own the business.
pub async fn update_order_status(
    pool:             &PgPool,
    business_user_id: i32,
    order_id:         i32,
    new_status:       &str,
) -> AppResult<WhiskedOrderRow> {
    let mut conn = pool.acquire().await.map_err(DomainError::Db)?;
    let _ = repository::assert_business_owner(&mut conn, order_id, business_user_id).await?;
    let order = repository::get_by_id(&mut conn, order_id)
        .await?
        .ok_or(DomainError::NotFound)?;
    let allowed = matches!(
        (order.status.as_str(), new_status),
        ("pending", "preparing") | ("preparing", "ready")
    );
    if !allowed {
        return Err(DomainError::invalid_input(format!(
            "cannot transition order from '{}' to '{}'",
            order.status, new_status,
        )));
    }
    let updated = repository::update_status(&mut conn, order_id, new_status).await?;
    audit::write(
        pool,
        Some(business_user_id),
        None,
        "whisked.order_status_changed",
        serde_json::json!({
            "order_id":   order_id,
            "from":       order.status,
            "to":         new_status,
        }),
    )
    .await;
    Ok(updated)
}

// ── Business dashboard ──────────────────────────────────────────────────────

/// All in-flight orders for a business (`pending`, `preparing`, `ready`).
/// Caller must own the business.
pub async fn list_active_for_business(
    pool:             &PgPool,
    business_user_id: i32,
    business_id:      i32,
) -> AppResult<Vec<WhiskedOrderResponse>> {
    let mut conn = pool.acquire().await.map_err(DomainError::Db)?;
    repository::assert_business_owner_for_business(&mut conn, business_id, business_user_id).await?;
    let rows = repository::list_active_for_business(&mut conn, business_id).await?;
    let mut out = Vec::with_capacity(rows.len());
    for order in rows {
        let items = repository::list_items_for_order(&mut conn, order.id).await?;
        out.push(WhiskedOrderResponse {
            pickup_code:          order.pickup_code.clone(),
            order,
            items,
            stripe_client_secret: None,
        });
    }
    Ok(out)
}

// ── ValidatePickup-request convenience ───────────────────────────────────────

/// Adapter so route handlers can use the `ValidatePickupRequest` type
/// directly without unpacking.
pub async fn validate_pickup_request(
    pool:             &PgPool,
    business_user_id: i32,
    order_id:         i32,
    req:              ValidatePickupRequest,
) -> AppResult<WhiskedOrderRow> {
    validate_pickup(pool, business_user_id, order_id, &req.pickup_code).await
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaction::RlsTransaction;
    use sqlx::PgPool;

    /// Bare-minimum fixture: one user, one location, one business owned by
    /// that user. Returns `(user_id, business_id)`. Mirrors the fixture
    /// pattern used by the billing tests.
    async fn make_user_and_business(pool: &PgPool) -> (i32, i32) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let (uid,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified, verification_status) \
             VALUES ($1, true, 'attested') RETURNING id"
        )
        .bind(&SafeEmail().fake::<String>())
        .fetch_one(pool).await.unwrap();
        let (loc_id,): (i32,) = sqlx::query_as(
            "INSERT INTO locations (name, location_type, address, timezone) \
             VALUES ('Whisked Loc', 'partner_business', '1 Whisk', 'America/Edmonton') \
             RETURNING id"
        )
        .fetch_one(pool).await.unwrap();
        let (biz_id,): (i32,) = sqlx::query_as(
            "INSERT INTO businesses (location_id, primary_holder_id, name, verification_status) \
             VALUES ($1, $2, 'Whisked Biz', 'active') RETURNING id"
        )
        .bind(loc_id).bind(uid)
        .fetch_one(pool).await.unwrap();
        (uid, biz_id)
    }

    /// Empty Stripe key → service short-circuits the API call; tests stay
    /// network-free. The shared `reqwest::Client` is harmless in that mode
    /// because no requests are made.
    fn test_http() -> reqwest::Client {
        reqwest::Client::new()
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn place_order_creates_order_with_pickup_code(pool: PgPool) {
        let (uid, biz_id) = make_user_and_business(&pool).await;
        let http = test_http();

        let mut tx = RlsTransaction::begin(&pool, uid).await.unwrap();
        let resp = place_order(
            &mut tx, &pool, &http, uid,
            PlaceOrderRequest {
                business_id: biz_id,
                items: vec![
                    OrderItemRequest { menu_item_id: 1, quantity: 1 },
                    OrderItemRequest { menu_item_id: 2, quantity: 2 },
                ],
            },
            "",
        ).await.expect("place_order must succeed");
        tx.commit().await.unwrap();

        // Pickup code shape: "W-XXXX" with chars from the safe alphabet.
        assert!(resp.pickup_code.starts_with("W-"));
        assert_eq!(resp.pickup_code.len(), 6);
        let body = &resp.pickup_code[2..];
        assert!(body.chars().all(|c| PICKUP_CODE_ALPHABET.contains(&(c as u8))));
        // 1 ceremonial (650) + 2 lattes (750 * 2 = 1500) = 2150 cents.
        assert_eq!(resp.order.total_cents, 2150);
        assert_eq!(resp.items.len(), 2);
        assert_eq!(resp.order.status, "pending");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn validate_pickup_marks_order_collected(pool: PgPool) {
        let (uid, biz_id) = make_user_and_business(&pool).await;
        let http = test_http();

        let resp = {
            let mut tx = RlsTransaction::begin(&pool, uid).await.unwrap();
            let r = place_order(
                &mut tx, &pool, &http, uid,
                PlaceOrderRequest {
                    business_id: biz_id,
                    items: vec![OrderItemRequest { menu_item_id: 1, quantity: 1 }],
                },
                "",
            ).await.unwrap();
            tx.commit().await.unwrap();
            r
        };

        // Manually advance to .ready (the real path goes through the staff
        // dashboard's PATCH /status).
        let mut conn = pool.acquire().await.unwrap();
        repository::_test_force_status(&mut conn, resp.order.id, "ready").await.unwrap();
        drop(conn);

        let updated = validate_pickup(&pool, uid, resp.order.id, &resp.pickup_code)
            .await
            .expect("validate_pickup must succeed for ready order with correct code");

        assert_eq!(updated.status, "collected");
        assert!(updated.pickup_code_used_at.is_some(), "pickup_code_used_at must be stamped");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn validate_pickup_rejects_wrong_code(pool: PgPool) {
        let (uid, biz_id) = make_user_and_business(&pool).await;
        let http = test_http();

        let resp = {
            let mut tx = RlsTransaction::begin(&pool, uid).await.unwrap();
            let r = place_order(
                &mut tx, &pool, &http, uid,
                PlaceOrderRequest {
                    business_id: biz_id,
                    items: vec![OrderItemRequest { menu_item_id: 1, quantity: 1 }],
                },
                "",
            ).await.unwrap();
            tx.commit().await.unwrap();
            r
        };

        let mut conn = pool.acquire().await.unwrap();
        repository::_test_force_status(&mut conn, resp.order.id, "ready").await.unwrap();
        drop(conn);

        let err = validate_pickup(&pool, uid, resp.order.id, "W-WRNG")
            .await
            .unwrap_err();
        assert!(matches!(err, DomainError::Forbidden), "wrong code must be Forbidden, got {err:?}");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn validate_pickup_rejects_already_used_code(pool: PgPool) {
        let (uid, biz_id) = make_user_and_business(&pool).await;
        let http = test_http();

        let resp = {
            let mut tx = RlsTransaction::begin(&pool, uid).await.unwrap();
            let r = place_order(
                &mut tx, &pool, &http, uid,
                PlaceOrderRequest {
                    business_id: biz_id,
                    items: vec![OrderItemRequest { menu_item_id: 1, quantity: 1 }],
                },
                "",
            ).await.unwrap();
            tx.commit().await.unwrap();
            r
        };

        let mut conn = pool.acquire().await.unwrap();
        repository::_test_force_status(&mut conn, resp.order.id, "ready").await.unwrap();
        drop(conn);

        // First call succeeds.
        validate_pickup(&pool, uid, resp.order.id, &resp.pickup_code).await.unwrap();
        // Second call must fail with Conflict.
        let err = validate_pickup(&pool, uid, resp.order.id, &resp.pickup_code)
            .await
            .unwrap_err();
        assert!(matches!(err, DomainError::Conflict(_)), "double-validate must be Conflict, got {err:?}");
    }

    /// Empty `stripe_secret_key` short-circuits the Stripe call so the order
    /// persists with `NULL stripe_payment_intent_id` and the response carries
    /// `stripe_client_secret: None`. This is the path every unit test exercises;
    /// pinning it here protects future test additions from accidentally
    /// requiring a live Stripe key.
    #[sqlx::test(migrations = "../server/migrations")]
    async fn place_order_without_stripe_key_has_null_payment_intent(pool: PgPool) {
        let (uid, biz_id) = make_user_and_business(&pool).await;
        let http = test_http();

        let mut tx = RlsTransaction::begin(&pool, uid).await.unwrap();
        let resp = place_order(
            &mut tx, &pool, &http, uid,
            PlaceOrderRequest {
                business_id: biz_id,
                items: vec![OrderItemRequest { menu_item_id: 1, quantity: 1 }],
            },
            "",
        ).await.expect("place_order with empty stripe key must succeed");
        tx.commit().await.unwrap();

        assert!(
            resp.order.stripe_payment_intent_id.is_none(),
            "stripe_payment_intent_id must be NULL when no stripe key is set",
        );
        assert!(
            resp.stripe_client_secret.is_none(),
            "stripe_client_secret must be None when no stripe key is set",
        );
    }
}
