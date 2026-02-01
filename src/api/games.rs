use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::models::Game;
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
struct GameListItem {
    id: Uuid,
    name: String,
    description: Option<String>,
    game_type: String,
    status: String,
    max_players: i32,
    player_count: Option<usize>,
    is_running: bool,
}

#[derive(Serialize)]
struct ListGamesResponse {
    games: Vec<GameListItem>,
}

async fn list_games(
    State(state): State<GamesState>,
) -> Result<Json<ListGamesResponse>, (StatusCode, String)> {
    // Query game definitions from database
    let db_games: Vec<Game> = sqlx::query_as("SELECT * FROM games ORDER BY created_at DESC")
        .fetch_all(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Get running instance info from memory
    let running_games = game::list_games(&state.game_manager);
    let running_map: std::collections::HashMap<Uuid, &game::GameInfo> = running_games
        .iter()
        .map(|g| (g.id, g))
        .collect();

    // Merge DB definitions with runtime info
    let games = db_games
        .into_iter()
        .map(|g| {
            let running = running_map.get(&g.id);
            GameListItem {
                id: g.id,
                name: g.name,
                description: g.description,
                game_type: g.game_type,
                status: running.map(|r| r.status.clone()).unwrap_or(g.status),
                max_players: g.max_players,
                player_count: running.map(|r| r.player_count),
                is_running: running.is_some(),
            }
        })
        .collect();

    Ok(Json(ListGamesResponse { games }))
}

#[derive(Deserialize)]
struct CreateGameRequest {
    name: String,
    description: Option<String>,
    #[serde(default = "default_game_type")]
    game_type: String,
    script_code: Option<String>,
}

fn default_game_type() -> String {
    "shooter".to_string()
}

#[derive(Serialize)]
struct CreateGameResponse {
    game_id: Uuid,
    name: String,
    status: String,
}

async fn create_game(
    State(state): State<GamesState>,
    headers: HeaderMap,
    Json(payload): Json<CreateGameRequest>,
) -> Result<Json<CreateGameResponse>, (StatusCode, String)> {
    let api_key = extract_api_key(&headers)
        .ok_or((StatusCode::UNAUTHORIZED, "Missing Authorization header".to_string()))?;

    let agent = sqlx::query_as::<_, (Uuid,)>("SELECT id FROM agents WHERE api_key = $1")
        .bind(&api_key)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::UNAUTHORIZED, "Invalid API key".to_string()))?;

    let game_id = Uuid::new_v4();

    // Create game definition in database only (no memory instance yet)
    sqlx::query(
        "INSERT INTO games (id, name, description, game_type, creator_id, status, script_code) VALUES ($1, $2, $3, $4, $5, 'waiting', $6)",
    )
    .bind(game_id)
    .bind(&payload.name)
    .bind(&payload.description)
    .bind(&payload.game_type)
    .bind(agent.0)
    .bind(&payload.script_code)
    .execute(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(CreateGameResponse {
        game_id,
        name: payload.name,
        status: "waiting".to_string(),
    }))
}

async fn get_game(
    State(state): State<GamesState>,
    Path(id): Path<Uuid>,
) -> Result<Json<GameListItem>, (StatusCode, String)> {
    // Get from database first
    let db_game: Game = sqlx::query_as("SELECT * FROM games WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Game not found".to_string()))?;

    // Check if running instance exists
    let running_info = game::get_game_info(&state.game_manager, id);

    Ok(Json(GameListItem {
        id: db_game.id,
        name: db_game.name,
        description: db_game.description,
        game_type: db_game.game_type,
        status: running_info
            .as_ref()
            .map(|r| r.status.clone())
            .unwrap_or(db_game.status),
        max_players: db_game.max_players,
        player_count: running_info.as_ref().map(|r| r.player_count),
        is_running: running_info.is_some(),
    }))
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

    // Get game from database including script
    let db_game: Game = sqlx::query_as("SELECT * FROM games WHERE id = $1")
        .bind(game_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Game not found".to_string()))?;

    // Get or create the running instance with script if available
    game::get_or_create_instance_with_script(
        &state.game_manager,
        game_id,
        db_game.script_code.as_deref(),
    );

    // Join the instance
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

    // First check for games with waiting status and room for players
    let waiting_game: Option<Game> = sqlx::query_as(
        "SELECT * FROM games WHERE status = 'waiting' ORDER BY created_at ASC LIMIT 1",
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Some(db_game) = waiting_game {
        let game_id = db_game.id;
        // Check if instance has room (create if needed)
        game::get_or_create_instance_with_script(
            &state.game_manager,
            game_id,
            db_game.script_code.as_deref(),
        );
        if game::get_game_info(&state.game_manager, game_id).is_some() {
            return Ok(Json(MatchmakeResponse {
                game_id,
                created: false,
            }));
        }
    }

    // No suitable game found, create a new one
    let game_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO games (id, name, status) VALUES ($1, 'Matchmade Game', 'waiting')",
    )
    .bind(game_id)
    .execute(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    game::get_or_create_instance(&state.game_manager, game_id);

    Ok(Json(MatchmakeResponse {
        game_id,
        created: true,
    }))
}
