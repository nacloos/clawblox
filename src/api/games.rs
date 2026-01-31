use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::game::{self, GameManagerHandle};

use super::agents::extract_api_key;

#[derive(Clone)]
pub struct GamesState {
    pub pool: PgPool,
    pub game_manager: GameManagerHandle,
}

pub fn routes(pool: PgPool, game_manager: GameManagerHandle) -> Router {
    let state = GamesState { pool, game_manager };

    Router::new()
        .route("/games", get(list_games).post(create_game))
        .route("/games/{id}", get(get_game))
        .route("/games/{id}/join", post(join_game))
        .route("/games/{id}/leave", post(leave_game))
        .route("/matchmake", post(matchmake))
        .with_state(state)
}

#[derive(Serialize)]
struct ListGamesResponse {
    games: Vec<game::GameInfo>,
}

async fn list_games(State(state): State<GamesState>) -> Json<ListGamesResponse> {
    let games = game::list_games(&state.game_manager);
    Json(ListGamesResponse { games })
}

#[derive(Serialize)]
struct CreateGameResponse {
    game_id: Uuid,
    status: String,
}

async fn create_game(
    State(state): State<GamesState>,
    headers: HeaderMap,
) -> Result<Json<CreateGameResponse>, (StatusCode, String)> {
    let api_key = extract_api_key(&headers)
        .ok_or((StatusCode::UNAUTHORIZED, "Missing Authorization header".to_string()))?;

    sqlx::query_as::<_, (Uuid,)>("SELECT id FROM agents WHERE api_key = $1")
        .bind(&api_key)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::UNAUTHORIZED, "Invalid API key".to_string()))?;

    let game_id = game::create_game(&state.game_manager);

    sqlx::query("INSERT INTO games (id, status) VALUES ($1, 'waiting')")
        .bind(game_id)
        .execute(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(CreateGameResponse {
        game_id,
        status: "waiting".to_string(),
    }))
}

async fn get_game(
    State(state): State<GamesState>,
    Path(id): Path<Uuid>,
) -> Result<Json<game::GameInfo>, (StatusCode, String)> {
    let info = game::get_game_info(&state.game_manager, id)
        .ok_or((StatusCode::NOT_FOUND, "Game not found".to_string()))?;

    Ok(Json(info))
}

#[derive(Serialize)]
struct JoinGameResponse {
    success: bool,
    message: String,
}

async fn join_game(
    State(state): State<GamesState>,
    Path(game_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<JoinGameResponse>, (StatusCode, String)> {
    let api_key = extract_api_key(&headers)
        .ok_or((StatusCode::UNAUTHORIZED, "Missing Authorization header".to_string()))?;

    let agent = sqlx::query_as::<_, (Uuid,)>("SELECT id FROM agents WHERE api_key = $1")
        .bind(&api_key)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::UNAUTHORIZED, "Invalid API key".to_string()))?;

    let agent_id = agent.0;

    game::join_game(&state.game_manager, game_id, agent_id)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    sqlx::query(
        "INSERT INTO game_players (game_id, agent_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(game_id)
    .bind(agent_id)
    .execute(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(JoinGameResponse {
        success: true,
        message: "Joined game".to_string(),
    }))
}

async fn leave_game(
    State(state): State<GamesState>,
    Path(game_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<JoinGameResponse>, (StatusCode, String)> {
    let api_key = extract_api_key(&headers)
        .ok_or((StatusCode::UNAUTHORIZED, "Missing Authorization header".to_string()))?;

    let agent = sqlx::query_as::<_, (Uuid,)>("SELECT id FROM agents WHERE api_key = $1")
        .bind(&api_key)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::UNAUTHORIZED, "Invalid API key".to_string()))?;

    let agent_id = agent.0;

    game::leave_game(&state.game_manager, game_id, agent_id)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    sqlx::query("DELETE FROM game_players WHERE game_id = $1 AND agent_id = $2")
        .bind(game_id)
        .bind(agent_id)
        .execute(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(JoinGameResponse {
        success: true,
        message: "Left game".to_string(),
    }))
}

#[derive(Serialize)]
struct MatchmakeResponse {
    game_id: Uuid,
    created: bool,
}

async fn matchmake(
    State(state): State<GamesState>,
    headers: HeaderMap,
) -> Result<Json<MatchmakeResponse>, (StatusCode, String)> {
    let api_key = extract_api_key(&headers)
        .ok_or((StatusCode::UNAUTHORIZED, "Missing Authorization header".to_string()))?;

    let _ = sqlx::query_as::<_, (Uuid,)>("SELECT id FROM agents WHERE api_key = $1")
        .bind(&api_key)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::UNAUTHORIZED, "Invalid API key".to_string()))?;

    let (game_id, created) = game::matchmake(&state.game_manager);

    if created {
        sqlx::query("INSERT INTO games (id, status) VALUES ($1, 'waiting')")
            .bind(game_id)
            .execute(&state.pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    Ok(Json(MatchmakeResponse { game_id, created }))
}
