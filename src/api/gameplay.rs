use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::models::Game;
use crate::game::{
    self,
    instance::{GameAction, PlayerObservation, SpectatorObservation},
    GameManagerHandle,
};

use super::agents::extract_api_key;

#[derive(Clone)]
pub struct GameplayState {
    pub pool: PgPool,
    pub game_manager: GameManagerHandle,
}

pub fn routes(pool: PgPool, game_manager: GameManagerHandle) -> Router {
    let state = GameplayState { pool, game_manager };

    Router::new()
        .route("/games/{id}/observe", get(observe))
        .route("/games/{id}/spectate", get(spectate))
        .route("/games/{id}/action", post(action))
        .route("/games/{id}/skill", get(get_skill))
        .route("/games/{id}/input", post(send_input))
        .with_state(state)
}

async fn observe(
    State(state): State<GameplayState>,
    Path(game_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<PlayerObservation>, (StatusCode, String)> {
    let api_key = extract_api_key(&headers)
        .ok_or((StatusCode::UNAUTHORIZED, "Missing Authorization header".to_string()))?;

    let agent = sqlx::query_as::<_, (Uuid,)>("SELECT id FROM agents WHERE api_key = $1")
        .bind(&api_key)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::UNAUTHORIZED, "Invalid API key".to_string()))?;

    let agent_id = agent.0;

    let observation = game::get_observation(&state.game_manager, game_id, agent_id)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    Ok(Json(observation))
}

async fn spectate(
    State(state): State<GameplayState>,
    Path(game_id): Path<Uuid>,
) -> Result<Json<SpectatorObservation>, (StatusCode, String)> {
    // First verify game exists in database and get script
    let db_game: (Uuid, Option<String>) = sqlx::query_as(
        "SELECT id, script_code FROM games WHERE id = $1"
    )
        .bind(game_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Game not found".to_string()))?;

    // Auto-start the game instance if not running (for spectating)
    if !game::is_instance_running(&state.game_manager, game_id) {
        game::get_or_create_instance_with_script(
            &state.game_manager,
            game_id,
            db_game.1.as_deref(),
        );
    }

    let observation = game::get_spectator_observation(&state.game_manager, game_id)
        .map_err(|e| (StatusCode::NOT_FOUND, e))?;

    Ok(Json(observation))
}

#[derive(Serialize)]
struct ActionResponse {
    success: bool,
    message: String,
}

async fn action(
    State(state): State<GameplayState>,
    Path(game_id): Path<Uuid>,
    headers: HeaderMap,
    Json(game_action): Json<GameAction>,
) -> Result<Json<ActionResponse>, (StatusCode, String)> {
    let api_key = extract_api_key(&headers)
        .ok_or((StatusCode::UNAUTHORIZED, "Missing Authorization header".to_string()))?;

    let agent = sqlx::query_as::<_, (Uuid,)>("SELECT id FROM agents WHERE api_key = $1")
        .bind(&api_key)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::UNAUTHORIZED, "Invalid API key".to_string()))?;

    let agent_id = agent.0;

    game::queue_action(&state.game_manager, game_id, agent_id, game_action)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    Ok(Json(ActionResponse {
        success: true,
        message: "Action queued".to_string(),
    }))
}

/// GET /games/{id}/skill - Get the game's SKILL.md for agents
async fn get_skill(
    State(state): State<GameplayState>,
    Path(game_id): Path<Uuid>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let game: Game = sqlx::query_as("SELECT * FROM games WHERE id = $1")
        .bind(game_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Game not found".to_string()))?;

    let skill_md = game
        .skill_md
        .ok_or((StatusCode::NOT_FOUND, "Game has no skill definition".to_string()))?;

    Ok((
        StatusCode::OK,
        [("content-type", "text/markdown; charset=utf-8")],
        skill_md,
    ))
}

/// Input from an agent
#[derive(Deserialize)]
pub struct AgentInputRequest {
    #[serde(rename = "type")]
    pub input_type: String,
    #[serde(default)]
    pub data: serde_json::Value,
}

/// POST /games/{id}/input - Send an input from an agent
/// Returns the player's observation after queuing the input
async fn send_input(
    State(state): State<GameplayState>,
    Path(game_id): Path<Uuid>,
    headers: HeaderMap,
    Json(input): Json<AgentInputRequest>,
) -> Result<Json<PlayerObservation>, (StatusCode, String)> {
    let api_key = extract_api_key(&headers)
        .ok_or((StatusCode::UNAUTHORIZED, "Missing Authorization header".to_string()))?;

    let agent = sqlx::query_as::<_, (Uuid,)>("SELECT id FROM agents WHERE api_key = $1")
        .bind(&api_key)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::UNAUTHORIZED, "Invalid API key".to_string()))?;

    let agent_id = agent.0;

    // Increment play count on first input (could be optimized)
    sqlx::query("UPDATE games SET plays = plays + 1 WHERE id = $1")
        .bind(game_id)
        .execute(&state.pool)
        .await
        .ok(); // Ignore errors for play count

    game::queue_input(&state.game_manager, game_id, agent_id, input.input_type, input.data)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    // Return observation instead of simple success message (reduces round-trips)
    let observation = game::get_observation(&state.game_manager, game_id, agent_id)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    Ok(Json(observation))
}
