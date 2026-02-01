use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::game::{self, instance::{GameAction, PlayerObservation, SpectatorObservation}, GameManagerHandle};

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
