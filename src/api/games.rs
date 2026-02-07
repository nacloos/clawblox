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
use super::ApiKeyCache;

#[derive(Clone)]
pub struct GamesState {
    pub pool: PgPool,
    pub game_manager: GameManagerHandle,
    pub api_key_cache: ApiKeyCache,
}

pub fn routes(pool: PgPool, game_manager: GameManagerHandle, api_key_cache: ApiKeyCache) -> Router {
    let state = GamesState { pool, game_manager, api_key_cache };

    Router::new()
        .route("/games", get(list_games).post(create_game))
        .route("/games/{id}", get(get_game).put(update_game))
        .route("/games/{id}/join", post(join_game))
        .route("/games/{id}/leave", post(leave_game))
        .with_state(state)
}

async fn get_agent_info_from_api_key(
    api_key: &str,
    cache: &ApiKeyCache,
    pool: &PgPool,
) -> Result<(Uuid, String), (StatusCode, String)> {
    if let Some(entry) = cache.get(api_key) {
        return Ok(entry.clone());
    }

    let agent = sqlx::query_as::<_, (Uuid, String)>("SELECT id, name FROM agents WHERE api_key = $1")
        .bind(api_key)
        .fetch_optional(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::UNAUTHORIZED, "Invalid API key".to_string()))?;

    cache.insert(api_key.to_string(), agent.clone());
    Ok(agent)
}

async fn get_agent_id_from_api_key(
    api_key: &str,
    cache: &ApiKeyCache,
    pool: &PgPool,
) -> Result<Uuid, (StatusCode, String)> {
    let (agent_id, _) = get_agent_info_from_api_key(api_key, cache, pool).await?;
    Ok(agent_id)
}

// =============================================================================
// Game CRUD
// =============================================================================

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
    published: bool,
    plays: i32,
    likes: i32,
}

#[derive(Serialize)]
struct ListGamesResponse {
    games: Vec<GameListItem>,
}

async fn list_games(
    State(state): State<GamesState>,
) -> Result<Json<ListGamesResponse>, (StatusCode, String)> {
    let db_games: Vec<Game> = sqlx::query_as("SELECT * FROM games ORDER BY created_at DESC")
        .fetch_all(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let running_games = game::list_games(&state.game_manager);
    let running_map: std::collections::HashMap<Uuid, &game::GameInfo> = running_games
        .iter()
        .map(|g| (g.id, g))
        .collect();

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
                published: g.published,
                plays: g.plays,
                likes: g.likes,
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
    skill_md: Option<String>,
    #[serde(default = "default_max_players")]
    max_players: i32,
}

fn default_game_type() -> String {
    "shooter".to_string()
}

fn default_max_players() -> i32 {
    8
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

    sqlx::query(
        "INSERT INTO games (id, name, description, game_type, creator_id, status, script_code, skill_md, max_players)
         VALUES ($1, $2, $3, $4, $5, 'waiting', $6, $7, $8)",
    )
    .bind(game_id)
    .bind(&payload.name)
    .bind(&payload.description)
    .bind(&payload.game_type)
    .bind(agent.0)
    .bind(&payload.script_code)
    .bind(&payload.skill_md)
    .bind(payload.max_players)
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
    let db_game: Game = sqlx::query_as("SELECT * FROM games WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Game not found".to_string()))?;

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
        published: db_game.published,
        plays: db_game.plays,
        likes: db_game.likes,
    }))
}

#[derive(Deserialize)]
struct UpdateGameRequest {
    name: Option<String>,
    description: Option<String>,
    game_type: Option<String>,
    script_code: Option<String>,
    skill_md: Option<String>,
    max_players: Option<i32>,
}

#[derive(Serialize)]
struct UpdateGameResponse {
    success: bool,
    message: String,
}

async fn update_game(
    State(state): State<GamesState>,
    Path(game_id): Path<Uuid>,
    headers: HeaderMap,
    Json(payload): Json<UpdateGameRequest>,
) -> Result<Json<UpdateGameResponse>, (StatusCode, String)> {
    let api_key = extract_api_key(&headers)
        .ok_or((StatusCode::UNAUTHORIZED, "Missing Authorization header".to_string()))?;

    let agent = sqlx::query_as::<_, (Uuid,)>("SELECT id FROM agents WHERE api_key = $1")
        .bind(&api_key)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::UNAUTHORIZED, "Invalid API key".to_string()))?;

    let game: Game = sqlx::query_as("SELECT * FROM games WHERE id = $1")
        .bind(game_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Game not found".to_string()))?;

    if game.creator_id != Some(agent.0) {
        return Err((StatusCode::FORBIDDEN, "You don't own this game".to_string()));
    }

    let mut updates = Vec::new();
    let mut param_idx = 1;

    if payload.name.is_some() {
        param_idx += 1;
        updates.push(format!("name = ${}", param_idx));
    }
    if payload.description.is_some() {
        param_idx += 1;
        updates.push(format!("description = ${}", param_idx));
    }
    if payload.game_type.is_some() {
        param_idx += 1;
        updates.push(format!("game_type = ${}", param_idx));
    }
    if payload.script_code.is_some() {
        param_idx += 1;
        updates.push(format!("script_code = ${}", param_idx));
    }
    if payload.skill_md.is_some() {
        param_idx += 1;
        updates.push(format!("skill_md = ${}", param_idx));
    }
    if payload.max_players.is_some() {
        param_idx += 1;
        updates.push(format!("max_players = ${}", param_idx));
    }

    if updates.is_empty() {
        return Ok(Json(UpdateGameResponse {
            success: true,
            message: "No changes".to_string(),
        }));
    }

    let query = format!("UPDATE games SET {} WHERE id = $1", updates.join(", "));
    let mut q = sqlx::query(&query).bind(game_id);

    if let Some(ref name) = payload.name {
        q = q.bind(name);
    }
    if let Some(ref description) = payload.description {
        q = q.bind(description);
    }
    if let Some(ref game_type) = payload.game_type {
        q = q.bind(game_type);
    }
    if let Some(ref script_code) = payload.script_code {
        q = q.bind(script_code);
    }
    if let Some(ref skill_md) = payload.skill_md {
        q = q.bind(skill_md);
    }
    if let Some(max_players) = payload.max_players {
        q = q.bind(max_players);
    }

    q.execute(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(UpdateGameResponse {
        success: true,
        message: "Game updated".to_string(),
    }))
}

// =============================================================================
// Join / Leave
// =============================================================================

#[derive(Serialize)]
struct JoinGameResponse {
    success: bool,
    message: String,
    instance_id: Uuid,
}

async fn join_game(
    State(state): State<GamesState>,
    Path(game_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<JoinGameResponse>, (StatusCode, String)> {
    let api_key = extract_api_key(&headers)
        .ok_or((StatusCode::UNAUTHORIZED, "Missing Authorization header".to_string()))?;

    let (agent_id, agent_name) = get_agent_info_from_api_key(&api_key, &state.api_key_cache, &state.pool).await?;

    // Kick from old instance of same game (second tab scenario)
    if let Some(existing_instance_id) = game::get_player_instance(&state.game_manager, agent_id, game_id) {
        let _ = game::leave_instance(&state.game_manager, existing_instance_id, agent_id);
    }

    // Get game config from database
    let db_game: Game = sqlx::query_as("SELECT * FROM games WHERE id = $1")
        .bind(game_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Game not found".to_string()))?;

    let max_players = db_game.max_players as u32;

    // Find or create instance with capacity
    let result = game::find_or_create_instance(
        &state.game_manager,
        game_id,
        max_players,
        db_game.script_code.as_deref(),
    );

    // Join the instance
    game::join_instance(
        &state.game_manager,
        result.instance_id,
        game_id,
        agent_id,
        &agent_name,
    )
    .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    // Update database
    let pool = state.pool.clone();
    let instance_id = result.instance_id;
    tokio::spawn(async move {
        let _ = sqlx::query(
            "INSERT INTO game_players (game_id, agent_id, instance_id) VALUES ($1, $2, $3)
             ON CONFLICT (game_id, agent_id) DO UPDATE SET instance_id = $3",
        )
        .bind(game_id)
        .bind(agent_id)
        .bind(instance_id)
        .execute(&pool)
        .await;
    });

    Ok(Json(JoinGameResponse {
        success: true,
        message: if result.created {
            "Joined new instance".to_string()
        } else {
            "Joined existing instance".to_string()
        },
        instance_id: result.instance_id,
    }))
}

#[derive(Serialize)]
struct LeaveGameResponse {
    success: bool,
    message: String,
}

async fn leave_game(
    State(state): State<GamesState>,
    Path(game_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<LeaveGameResponse>, (StatusCode, String)> {
    let api_key = extract_api_key(&headers)
        .ok_or((StatusCode::UNAUTHORIZED, "Missing Authorization header".to_string()))?;

    let agent_id = get_agent_id_from_api_key(&api_key, &state.api_key_cache, &state.pool).await?;

    game::leave_game(&state.game_manager, game_id, agent_id)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    let pool = state.pool.clone();
    tokio::spawn(async move {
        let _ = sqlx::query("DELETE FROM game_players WHERE game_id = $1 AND agent_id = $2")
            .bind(game_id)
            .bind(agent_id)
            .execute(&pool)
            .await;
    });

    Ok(Json(LeaveGameResponse {
        success: true,
        message: "Left game".to_string(),
    }))
}

