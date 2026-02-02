use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::time::Duration;
use tokio::time::interval;
use uuid::Uuid;

use crate::db::models::Game;
use crate::game::{
    self,
    instance::{GameAction, PlayerObservation, SpectatorObservation},
    GameManagerHandle,
};

use super::agents::extract_api_key;
use super::ApiKeyCache;

#[derive(Clone)]
pub struct GameplayState {
    pub pool: PgPool,
    pub game_manager: GameManagerHandle,
    pub api_key_cache: ApiKeyCache,
}

/// Gets agent_id from API key, checking cache first, then DB (and caching result)
async fn get_agent_id_from_api_key(
    api_key: &str,
    cache: &ApiKeyCache,
    pool: &PgPool,
) -> Result<Uuid, (StatusCode, String)> {
    // Check cache first
    if let Some(agent_id) = cache.get(api_key) {
        return Ok(*agent_id);
    }

    // Cache miss - query DB
    let agent = sqlx::query_as::<_, (Uuid,)>("SELECT id FROM agents WHERE api_key = $1")
        .bind(api_key)
        .fetch_optional(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::UNAUTHORIZED, "Invalid API key".to_string()))?;

    let agent_id = agent.0;

    // Cache the result
    cache.insert(api_key.to_string(), agent_id);

    Ok(agent_id)
}

pub fn routes(pool: PgPool, game_manager: GameManagerHandle, api_key_cache: ApiKeyCache) -> Router {
    let state = GameplayState { pool, game_manager, api_key_cache };

    Router::new()
        .route("/games/{id}/observe", get(observe))
        .route("/games/{id}/spectate", get(spectate))
        .route("/games/{id}/spectate/ws", get(spectate_ws))
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

    let agent_id = get_agent_id_from_api_key(&api_key, &state.api_key_cache, &state.pool).await?;

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

    let agent_id = get_agent_id_from_api_key(&api_key, &state.api_key_cache, &state.pool).await?;

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

    let agent_id = get_agent_id_from_api_key(&api_key, &state.api_key_cache, &state.pool).await?;

    game::queue_input(&state.game_manager, game_id, agent_id, input.input_type, input.data)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    // Return observation instead of simple success message (reduces round-trips)
    let observation = game::get_observation(&state.game_manager, game_id, agent_id)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    Ok(Json(observation))
}

/// WebSocket endpoint for spectating game state in real-time
async fn spectate_ws(
    State(state): State<GameplayState>,
    Path(game_id): Path<Uuid>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_spectate_ws(socket, state, game_id))
}

/// Handle the WebSocket connection for spectating
async fn handle_spectate_ws(socket: WebSocket, state: GameplayState, game_id: Uuid) {
    let (mut sender, mut receiver) = socket.split();

    // First verify game exists in database and get script
    let db_game: Option<(Uuid, Option<String>)> =
        sqlx::query_as("SELECT id, script_code FROM games WHERE id = $1")
            .bind(game_id)
            .fetch_optional(&state.pool)
            .await
            .ok()
            .flatten();

    let Some(db_game) = db_game else {
        let _ = sender
            .send(Message::Text(
                r#"{"error":"Game not found"}"#.to_string().into(),
            ))
            .await;
        return;
    };

    // Auto-start the game instance if not running
    if !game::is_instance_running(&state.game_manager, game_id) {
        game::get_or_create_instance_with_script(&state.game_manager, game_id, db_game.1.as_deref());
    }

    // Send updates at ~30 fps (every 33ms)
    let mut tick_interval = interval(Duration::from_millis(33));
    let mut last_tick: u64 = 0;

    loop {
        tokio::select! {
            // Check for incoming messages (ping/pong, close)
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(data))) => {
                        if sender.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    _ => {}
                }
            }

            // Send game state on tick
            _ = tick_interval.tick() => {
                let observation = game::get_spectator_observation(&state.game_manager, game_id);

                match observation {
                    Ok(obs) => {
                        // Only send if tick changed (avoids duplicate data)
                        if obs.tick != last_tick {
                            last_tick = obs.tick;
                            if let Ok(json) = serde_json::to_string(&obs) {
                                if sender.send(Message::Text(json.into())).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    Err(_) => {
                        // Game instance no longer exists
                        let _ = sender
                            .send(Message::Text(r#"{"error":"Game ended"}"#.to_string().into()))
                            .await;
                        break;
                    }
                }
            }
        }
    }
}
