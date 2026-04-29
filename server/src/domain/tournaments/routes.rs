use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};

use crate::{
    app::AppState,
    error::{AppError, AppResult},
    http::extractors::auth::RequireUser,
};

use super::types::{
    EnterTournamentBody, PlayCardBody, RegisterDeckBody, TournamentEntryRow, TournamentPlayRow,
    TournamentRow,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/tournaments",                             get(list_tournaments))
        .route("/api/tournaments/:id",                         get(get_tournament))
        .route("/api/tournaments/:id/enter",                   post(enter))
        .route("/api/tournaments/:id/deck",                    post(register_deck))
        .route("/api/tournaments/:id/play",                    post(play_card))
        .route("/api/tournaments/:id/standings",               get(standings))
        .route("/api/tournaments/:id/declare/:winner_id",      post(declare_winner))
}

// ── List / detail ─────────────────────────────────────────────────────────────

async fn list_tournaments(
    State(state): State<AppState>,
) -> AppResult<Json<Vec<TournamentRow>>> {
    let rows: Vec<TournamentRow> = sqlx::query_as(
        "SELECT id, name, entry_fee_cents, status, max_players, starts_at, created_at
         FROM tournaments
         ORDER BY created_at DESC
         LIMIT 50",
    )
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Db)?;

    Ok(Json(rows))
}

async fn get_tournament(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> AppResult<Json<serde_json::Value>> {
    let tournament: Option<TournamentRow> = sqlx::query_as(
        "SELECT id, name, entry_fee_cents, status, max_players, starts_at, created_at
         FROM tournaments WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Db)?;

    let tournament = tournament.ok_or(AppError::NotFound)?;

    let entries: Vec<TournamentEntryRow> = sqlx::query_as(
        "SELECT id, tournament_id, user_id, deck_json, status, created_at
         FROM tournament_entries
         WHERE tournament_id = $1
         ORDER BY created_at ASC",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Db)?;

    Ok(Json(serde_json::json!({
        "tournament": tournament,
        "entries":    entries,
    })))
}

// ── Enter ─────────────────────────────────────────────────────────────────────

/// Enter a tournament by paying the entry fee via Stripe.
///
/// If entry_fee_cents is 0, the entry row is created immediately as 'registered'.
/// Otherwise a Stripe PI is returned for the client to complete payment, and the
/// entry is pre-created as 'pending' — confirmed by webhook.
async fn enter(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(tournament_id): Path<i32>,
    Json(body): Json<EnterTournamentBody>,
) -> AppResult<Json<serde_json::Value>> {
    let tournament: Option<TournamentRow> = sqlx::query_as(
        "SELECT id, name, entry_fee_cents, status, max_players, starts_at, created_at
         FROM tournaments WHERE id = $1",
    )
    .bind(tournament_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Db)?;

    let tournament = tournament.ok_or(AppError::NotFound)?;

    if tournament.status != "open" {
        return Err(AppError::bad_request("tournament is not accepting entries"));
    }

    // Capacity check.
    if let Some(max) = tournament.max_players {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM tournament_entries
             WHERE tournament_id = $1 AND status != 'pending'",
        )
        .bind(tournament_id)
        .fetch_one(&state.db)
        .await
        .map_err(AppError::Db)?;

        if count >= max as i64 {
            return Err(AppError::bad_request("tournament is full"));
        }
    }

    // Idempotency — reject duplicate entry.
    let already_entered: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM tournament_entries
             WHERE tournament_id = $1 AND user_id = $2
               AND status != 'eliminated'
         )",
    )
    .bind(tournament_id)
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .map_err(AppError::Db)?;

    if already_entered {
        return Err(AppError::bad_request("already entered this tournament"));
    }

    if tournament.entry_fee_cents == 0 {
        // Free entry — create immediately.
        let entry: TournamentEntryRow = sqlx::query_as(
            "INSERT INTO tournament_entries (tournament_id, user_id, deck_json, status)
             VALUES ($1, $2, $3, 'registered')
             RETURNING id, tournament_id, user_id, deck_json, status, created_at",
        )
        .bind(tournament_id)
        .bind(user_id)
        .bind(body.deck_json.as_ref())
        .fetch_one(&state.db)
        .await
        .map_err(AppError::Db)?;

        return Ok(Json(serde_json::json!({ "entry": entry })));
    }

    // Paid entry — create Stripe PI then pre-create pending entry.
    let pi = state
        .stripe()
        .create_payment_intent(
            tournament.entry_fee_cents,
            "cad",
            None,
            &[
                ("type", "tournament_entry"),
                ("tournament_id", &tournament_id.to_string()),
                ("user_id", &user_id.to_string()),
            ],
        )
        .await?;

    sqlx::query(
        "INSERT INTO tournament_entries (tournament_id, user_id, deck_json, status)
         VALUES ($1, $2, $3, 'pending')
         ON CONFLICT (tournament_id, user_id) DO NOTHING",
    )
    .bind(tournament_id)
    .bind(user_id)
    .bind(body.deck_json.as_ref())
    .execute(&state.db)
    .await
    .map_err(AppError::Db)?;

    Ok(Json(serde_json::json!({
        "client_secret":    pi.client_secret,
        "entry_fee_cents":  tournament.entry_fee_cents,
    })))
}

// ── Deck registration ─────────────────────────────────────────────────────────

/// Register or update the player's deck before the tournament starts.
async fn register_deck(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(tournament_id): Path<i32>,
    Json(body): Json<RegisterDeckBody>,
) -> AppResult<Json<serde_json::Value>> {
    let result = sqlx::query(
        "UPDATE tournament_entries SET deck_json = $1
         WHERE tournament_id = $2 AND user_id = $3
           AND status = 'registered'",
    )
    .bind(&body.deck_json)
    .bind(tournament_id)
    .bind(user_id)
    .execute(&state.db)
    .await
    .map_err(AppError::Db)?;

    if result.rows_affected() == 0 {
        return Err(AppError::bad_request(
            "no registered entry found — enter the tournament first",
        ));
    }

    Ok(Json(serde_json::json!({ "status": "deck_registered" })))
}

// ── Play ──────────────────────────────────────────────────────────────────────

/// Record a card play — validates the player is still active in the tournament.
async fn play_card(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(tournament_id): Path<i32>,
    Json(body): Json<PlayCardBody>,
) -> AppResult<Json<TournamentPlayRow>> {
    // Verify the tournament is active and the caller is an active participant.
    let is_active: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1
             FROM tournament_entries te
             JOIN tournaments t ON t.id = te.tournament_id
             WHERE te.tournament_id = $1
               AND te.user_id = $2
               AND te.status = 'active'
               AND t.status = 'active'
         )",
    )
    .bind(tournament_id)
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .map_err(AppError::Db)?;

    if !is_active {
        return Err(AppError::Forbidden);
    }

    // Get the current round.
    let round: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(round), 1) FROM tournament_plays WHERE tournament_id = $1",
    )
    .bind(tournament_id)
    .fetch_one(&state.db)
    .await
    .map_err(AppError::Db)?;

    let row: TournamentPlayRow = sqlx::query_as(
        "INSERT INTO tournament_plays (tournament_id, round, player_id, card_id, target_id)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id, tournament_id, round, player_id, card_id, target_id, played_at",
    )
    .bind(tournament_id)
    .bind(round)
    .bind(user_id)
    .bind(&body.card_id)
    .bind(body.target_id.as_deref())
    .fetch_one(&state.db)
    .await
    .map_err(AppError::Db)?;

    Ok(Json(row))
}

// ── Standings ─────────────────────────────────────────────────────────────────

async fn standings(
    State(state): State<AppState>,
    Path(tournament_id): Path<i32>,
) -> AppResult<Json<Vec<TournamentEntryRow>>> {
    let rows: Vec<TournamentEntryRow> = sqlx::query_as(
        "SELECT id, tournament_id, user_id, deck_json, status, created_at
         FROM tournament_entries
         WHERE tournament_id = $1
         ORDER BY
             CASE status
                 WHEN 'winner'     THEN 0
                 WHEN 'active'     THEN 1
                 WHEN 'registered' THEN 2
                 WHEN 'eliminated' THEN 3
                 ELSE 4
             END,
             created_at ASC",
    )
    .bind(tournament_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Db)?;

    Ok(Json(rows))
}

// ── Declare winner ────────────────────────────────────────────────────────────

/// Declare a tournament winner and distribute prize pool.
///
/// Prize = sum of all entry fees (handled outside Stripe as platform credits
/// or via off-session charges). This endpoint is admin / operator only —
/// production auth will gate it to an admin role. For now it verifies the
/// tournament is active and the winner is a participant, then marks the
/// tournament completed.
async fn declare_winner(
    State(state): State<AppState>,
    RequireUser(_operator_id): RequireUser,
    Path((tournament_id, winner_id)): Path<(i32, i32)>,
) -> AppResult<Json<serde_json::Value>> {
    let mut tx = state.db.begin().await.map_err(AppError::Db)?;

    // Lock the tournament row.
    let status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM tournaments WHERE id = $1 FOR UPDATE",
    )
    .bind(tournament_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::Db)?;

    match status.as_deref() {
        None => return Err(AppError::NotFound),
        Some(s) if s != "active" => {
            return Err(AppError::bad_request("tournament is not active"));
        }
        _ => {}
    }

    // Verify winner is a registered participant.
    let is_participant: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM tournament_entries
             WHERE tournament_id = $1 AND user_id = $2 AND status = 'active'
         )",
    )
    .bind(tournament_id)
    .bind(winner_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(AppError::Db)?;

    if !is_participant {
        return Err(AppError::bad_request("specified user is not an active participant"));
    }

    // Mark all non-winning active players as eliminated.
    sqlx::query(
        "UPDATE tournament_entries SET status = 'eliminated'
         WHERE tournament_id = $1 AND user_id != $2 AND status = 'active'",
    )
    .bind(tournament_id)
    .bind(winner_id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::Db)?;

    // Mark winner.
    sqlx::query(
        "UPDATE tournament_entries SET status = 'winner'
         WHERE tournament_id = $1 AND user_id = $2",
    )
    .bind(tournament_id)
    .bind(winner_id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::Db)?;

    // Close the tournament.
    sqlx::query("UPDATE tournaments SET status = 'completed' WHERE id = $1")
        .bind(tournament_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::Db)?;

    // Calculate prize pool (sum of confirmed entries).
    let prize_cents: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(t.entry_fee_cents), 0)
         FROM tournament_entries te
         JOIN tournaments t ON t.id = te.tournament_id
         WHERE te.tournament_id = $1 AND te.status IN ('winner', 'eliminated')",
    )
    .bind(tournament_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(AppError::Db)?;

    tx.commit().await.map_err(AppError::Db)?;

    Ok(Json(serde_json::json!({
        "winner_id":    winner_id,
        "prize_cents":  prize_cents,
        "status":       "completed",
    })))
}
