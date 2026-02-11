use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    http::{self, HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use tower_governor::{
    governor::GovernorConfigBuilder,
    key_extractor::KeyExtractor,
    GovernorError, GovernorLayer,
};
use flate2::{write::GzEncoder, Compression};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::io::Write;
use std::time::Duration;
use tokio::time::interval;
use uuid::Uuid;

use crate::db::models::Game;
use crate::game::{
    self,
    instance::{MapInfo, PlayerObservation, SpectatorObservation},
    GameManagerHandle,
};

use super::agents::extract_api_key;
use super::ApiKeyCache;

/// Extracts API key from Authorization header for rate limiting
#[derive(Clone)]
struct ApiKeyExtractor;

impl KeyExtractor for ApiKeyExtractor {
    type Key = String;

    fn extract<T>(&self, req: &http::Request<T>) -> Result<Self::Key, GovernorError> {
        req.headers()
            .get("Authorization")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .map(|s| s.to_string())
            .ok_or(GovernorError::UnableToExtractKey)
    }
}

/// Compress data with gzip. Returns None if compression fails or data is too small.
fn gzip_compress(data: &[u8]) -> Option<Vec<u8>> {
    const MIN_SIZE_FOR_COMPRESSION: usize = 1024;
    if data.len() < MIN_SIZE_FOR_COMPRESSION {
        return None;
    }
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(data).ok()?;
    encoder.finish().ok()
}

#[derive(Clone)]
pub struct GameplayState {
    pub pool: PgPool,
    pub game_manager: GameManagerHandle,
    pub api_key_cache: ApiKeyCache,
    pub r2_public_url: Option<String>,
}

/// Gets agent_id from API key, checking cache first, then DB (and caching result)
async fn get_agent_id_from_api_key(
    api_key: &str,
    cache: &ApiKeyCache,
    pool: &PgPool,
) -> Result<Uuid, (StatusCode, String)> {
    // Check cache first
    if let Some(entry) = cache.get(api_key) {
        return Ok(entry.0);
    }

    // Cache miss - query DB for both id and name
    let agent = sqlx::query_as::<_, (Uuid, String)>("SELECT id, name FROM agents WHERE api_key = $1")
        .bind(api_key)
        .fetch_optional(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::UNAUTHORIZED, "Invalid API key".to_string()))?;

    // Cache the result
    cache.insert(api_key.to_string(), agent.clone());

    Ok(agent.0)
}

pub fn routes(
    pool: PgPool,
    game_manager: GameManagerHandle,
    api_key_cache: ApiKeyCache,
    r2_public_url: Option<String>,
) -> Router {
    let state = GameplayState { pool, game_manager, api_key_cache, r2_public_url };

    // Rate limit: 10 requests/second per agent, burst of 20
    // per_millisecond(100) = 1 token every 100ms = 10 tokens/second
    let governor_conf = GovernorConfigBuilder::default()
        .key_extractor(ApiKeyExtractor)
        .per_millisecond(100)
        .burst_size(20)
        .use_headers() // Adds x-ratelimit-* headers for debugging
        .finish()
        .unwrap();

    // AGENT ROUTES: Require auth, are rate-limited
    // Add new authenticated endpoints here
    let agent_routes = Router::new()
        .route("/games/{id}/observe", get(observe))
        .route("/games/{id}/input", post(send_input))
        .layer(GovernorLayer::new(governor_conf));

    // PUBLIC ROUTES: No auth, no rate limit
    // Add new public endpoints here
    let public_routes = Router::new()
        .route("/games/{id}/spectate", get(spectate))
        .route("/games/{id}/spectate/ws", get(spectate_ws))
        .route("/games/{id}/play/ws", get(play_ws))
        .route("/games/{id}/skill.md", get(get_skill))
        .route("/games/{id}/leaderboard", get(get_leaderboard))
        .route("/games/{id}/map", get(get_map));

    // DO NOT add routes here - use agent_routes or public_routes above
    Router::new()
        .merge(agent_routes)
        .merge(public_routes)
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

    let mut observation = game::get_observation(&state.game_manager, game_id, agent_id)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    if let Some(asset_info) = get_asset_info(&state, game_id).await? {
        resolve_player_observation_assets(
            &mut observation,
            &asset_info.r2_public_url,
            game_id,
            asset_info.asset_version,
        );
    }

    Ok(Json(observation))
}

fn resolve_asset_url(url: &mut String, r2_public_url: &str, game_id: Uuid, asset_version: i32) {
    if let Some(path) = url.strip_prefix("asset://") {
        *url = format!(
            "{}/games/{}/v{}/{}",
            r2_public_url.trim_end_matches('/'),
            game_id,
            asset_version,
            path
        );
    }
}

fn resolve_model_url_attrs(
    attrs: &mut std::collections::HashMap<String, serde_json::Value>,
    r2_public_url: &str,
    game_id: Uuid,
    asset_version: i32,
) {
    for key in ["ModelUrl", "model_url"] {
        if let Some(value) = attrs.get_mut(key) {
            if let Some(path) = value.as_str().and_then(|url| url.strip_prefix("asset://")) {
                *value = serde_json::Value::String(format!(
                    "{}/games/{}/v{}/{}",
                    r2_public_url.trim_end_matches('/'),
                    game_id,
                    asset_version,
                    path
                ));
            }
        }
    }
}

/// Resolve asset:// URLs in a SpectatorObservation to actual CDN URLs.
/// - Production: asset://path -> {r2_public_url}/games/{game_id}/v{version}/path
/// - /static/ and https:// URLs pass through unchanged
fn resolve_observation_assets(
    obs: &mut SpectatorObservation,
    r2_public_url: &str,
    game_id: Uuid,
    asset_version: i32,
) {
    for entity in &mut obs.entities {
        if let Some(ref mut url) = entity.model_url {
            resolve_asset_url(url, r2_public_url, game_id, asset_version);
            // /static/ and https:// pass through unchanged
        }
    }
}

fn resolve_player_observation_assets(
    obs: &mut PlayerObservation,
    r2_public_url: &str,
    game_id: Uuid,
    asset_version: i32,
) {
    for entity in &mut obs.world.entities {
        if let Some(attrs) = entity.attributes.as_mut() {
            resolve_model_url_attrs(attrs, r2_public_url, game_id, asset_version);
        }
    }
}

struct AssetInfo {
    r2_public_url: String,
    asset_version: i32,
}

#[derive(sqlx::FromRow)]
struct GameAssetInfoRow {
    has_assets: bool,
    asset_version: i32,
}

async fn get_asset_info(
    state: &GameplayState,
    game_id: Uuid,
) -> Result<Option<AssetInfo>, (StatusCode, String)> {
    let game: GameAssetInfoRow = sqlx::query_as("SELECT has_assets, asset_version FROM games WHERE id = $1")
        .bind(game_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Game not found".to_string()))?;

    if !game.has_assets {
        return Ok(None);
    }

    let Some(r2_public_url) = state.r2_public_url.clone() else {
        return Ok(None);
    };

    Ok(Some(AssetInfo {
        r2_public_url,
        asset_version: game.asset_version,
    }))
}

async fn spectate(
    State(state): State<GameplayState>,
    Path(game_id): Path<Uuid>,
) -> Result<Json<SpectatorObservation>, (StatusCode, String)> {
    // First verify game exists in database and get script + max_players
    let db_game: Game = sqlx::query_as("SELECT * FROM games WHERE id = $1")
        .bind(game_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Game not found".to_string()))?;

    // Auto-start the game instance if not running (for spectating)
    if !game::is_instance_running(&state.game_manager, game_id) {
        game::find_or_create_instance(
            &state.game_manager,
            game_id,
            db_game.max_players as u32,
            db_game.script_code.as_deref(),
        );
    }

    let mut observation = game::get_spectator_observation(&state.game_manager, game_id)
        .map_err(|e| (StatusCode::NOT_FOUND, e))?;

    if db_game.has_assets {
        if let Some(ref r2_url) = state.r2_public_url {
            resolve_observation_assets(&mut observation, r2_url, game_id, db_game.asset_version);
        }
    }

    Ok(Json(observation))
}

/// GET /games/{id}/map - Get static map geometry (one-time fetch)
async fn get_map(
    State(state): State<GameplayState>,
    Path(game_id): Path<Uuid>,
) -> Result<Json<MapInfo>, (StatusCode, String)> {
    // Verify game exists in database and get script + max_players
    let db_game: Game = sqlx::query_as("SELECT * FROM games WHERE id = $1")
        .bind(game_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Game not found".to_string()))?;

    // Auto-start the game instance if not running
    if !game::is_instance_running(&state.game_manager, game_id) {
        game::find_or_create_instance(
            &state.game_manager,
            game_id,
            db_game.max_players as u32,
            db_game.script_code.as_deref(),
        );
    }

    let map_info = game::get_map(&state.game_manager, game_id)
        .map_err(|e| (StatusCode::NOT_FOUND, e))?;

    Ok(Json(map_info))
}

/// GET /games/{id}/skill.md - Get the game's SKILL.md for agents
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
    let mut observation = game::get_observation(&state.game_manager, game_id, agent_id)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    if let Some(asset_info) = get_asset_info(&state, game_id).await? {
        resolve_player_observation_assets(
            &mut observation,
            &asset_info.r2_public_url,
            game_id,
            asset_info.asset_version,
        );
    }

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

fn anon_play_ws_enabled() -> bool {
    if cfg!(debug_assertions) {
        return true;
    }

    match std::env::var("CLAWBLOX_ENABLE_ANON_PLAY_WS") {
        Ok(v) => matches!(v.as_str(), "1" | "true" | "TRUE"),
        Err(_) => false,
    }
}

#[derive(Deserialize)]
struct PlayWsQuery {
    name: Option<String>,
}

#[derive(Deserialize, Default)]
struct PlayButtons {
    #[serde(default)]
    jump: bool,
    #[serde(default)]
    dive: bool,
}

#[derive(Deserialize)]
struct PlayIntentMessage {
    #[serde(default)]
    seq: Option<u64>,
    #[serde(default, rename = "move")]
    move_axis: Option<[f32; 2]>,
    #[serde(default)]
    buttons: PlayButtons,
    #[serde(default)]
    camera_yaw: f32,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum PlayClientMessage {
    Intent(PlayIntentMessage),
}

#[derive(Serialize)]
struct PlayJoinedMessage {
    #[serde(rename = "type")]
    message_type: &'static str,
    player_id: Uuid,
    instance_id: Uuid,
}

#[derive(Serialize)]
struct PlayAckMessage {
    #[serde(rename = "type")]
    message_type: &'static str,
    seq: u64,
}

#[derive(Serialize)]
struct PlayErrorMessage {
    #[serde(rename = "type")]
    message_type: &'static str,
    message: String,
}

#[derive(Default)]
struct PlayIntentState {
    move_x: f32,
    move_z: f32,
    camera_yaw: f32,
    jump_pressed: bool,
    dive_pressed: bool,
    jump_pending: bool,
    dive_pending: bool,
    was_moving: bool,
}

fn sanitized_player_name(name: Option<&str>, fallback_id: Uuid) -> String {
    let fallback = format!("player-{}", &fallback_id.to_string()[..8]);
    let Some(raw) = name else {
        return fallback;
    };

    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return fallback;
    }

    let cleaned: String = trimmed
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == ' ')
        .take(24)
        .collect();

    if cleaned.trim().is_empty() {
        fallback
    } else {
        cleaned
    }
}

fn camera_relative_move_to_world(move_x: f32, move_z: f32, camera_yaw: f32) -> (f32, f32) {
    let cos_yaw = camera_yaw.cos();
    let sin_yaw = camera_yaw.sin();

    let world_x = move_x * cos_yaw + move_z * sin_yaw;
    let world_z = -move_x * sin_yaw + move_z * cos_yaw;
    (world_x, world_z)
}

fn normalize_clamped_axis(v: [f32; 2]) -> (f32, f32) {
    let mut x = v[0].clamp(-1.0, 1.0);
    let mut z = v[1].clamp(-1.0, 1.0);
    let len = (x * x + z * z).sqrt();
    if len > 1.0 {
        x /= len;
        z /= len;
    }
    (x, z)
}

async fn send_play_error(sender: &mut futures_util::stream::SplitSink<WebSocket, Message>, message: &str) {
    let payload = PlayErrorMessage {
        message_type: "error",
        message: message.to_string(),
    };
    if let Ok(json) = serde_json::to_string(&payload) {
        let _ = sender.send(Message::Text(json.into())).await;
    }
}

async fn play_ws(
    State(state): State<GameplayState>,
    Path(game_id): Path<Uuid>,
    Query(query): Query<PlayWsQuery>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    if !anon_play_ws_enabled() {
        return Err((
            StatusCode::FORBIDDEN,
            "Anonymous play WebSocket is disabled. Set CLAWBLOX_ENABLE_ANON_PLAY_WS=1 to enable.".to_string(),
        ));
    }

    Ok(ws.on_upgrade(move |socket| handle_play_ws(socket, state, game_id, query.name)))
}

/// Query parameters for leaderboard endpoint
#[derive(Deserialize)]
struct LeaderboardQuery {
    /// The OrderedDataStore name (default: "Leaderboard")
    #[serde(default = "default_store_name")]
    store: String,
    /// Maximum number of entries to return (default: 10, max: 100)
    #[serde(default = "default_limit")]
    limit: i32,
}

fn default_store_name() -> String {
    "Leaderboard".to_string()
}

fn default_limit() -> i32 {
    10
}

/// A single leaderboard entry
#[derive(Serialize)]
struct LeaderboardEntry {
    rank: i32,
    key: String,
    score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

/// Leaderboard response
#[derive(Serialize)]
struct LeaderboardResponse {
    entries: Vec<LeaderboardEntry>,
}

/// GET /games/{id}/leaderboard - Get sorted leaderboard entries
async fn get_leaderboard(
    State(state): State<GameplayState>,
    Path(game_id): Path<Uuid>,
    Query(query): Query<LeaderboardQuery>,
) -> Result<Json<LeaderboardResponse>, (StatusCode, String)> {
    // Clamp limit to reasonable bounds
    let limit = query.limit.clamp(1, 100);

    // Query the data_stores table directly for sorted entries
    let results: Vec<(String, serde_json::Value)> = sqlx::query_as(
        r#"
        SELECT key, value
        FROM data_stores
        WHERE game_id = $1 AND store_name = $2 AND value ? 'score'
        ORDER BY (value->>'score')::numeric DESC NULLS LAST
        LIMIT $3
        "#,
    )
    .bind(game_id)
    .bind(&query.store)
    .bind(limit)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Convert to LeaderboardEntry structs
    let entries: Vec<LeaderboardEntry> = results
        .into_iter()
        .enumerate()
        .map(|(i, (key, value))| {
            let score = value
                .get("score")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let name = value
                .get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            LeaderboardEntry {
                rank: (i + 1) as i32,
                key,
                score,
                name,
            }
        })
        .collect();

    Ok(Json(LeaderboardResponse { entries }))
}

/// Handle the WebSocket connection for spectating
async fn handle_spectate_ws(socket: WebSocket, state: GameplayState, game_id: Uuid) {
    let (mut sender, mut receiver) = socket.split();

    // First verify game exists in database and get script + max_players
    let db_game: Option<Game> =
        sqlx::query_as("SELECT * FROM games WHERE id = $1")
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
        game::find_or_create_instance(
            &state.game_manager,
            game_id,
            db_game.max_players as u32,
            db_game.script_code.as_deref(),
        );
    }

    // Capture asset info for URL resolution
    let asset_info = if db_game.has_assets {
        state
            .r2_public_url
            .as_ref()
            .map(|url| (url.clone(), db_game.asset_version))
    } else {
        None
    };

    // Send updates at ~30 fps (every 33ms)
    let mut tick_interval = interval(Duration::from_millis(33));
    let mut last_tick: u64 = 0;
    let mut same_tick_count: u32 = 0;

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
                        let mut obs = obs;
                        if let Some((ref r2_url, version)) = asset_info {
                            resolve_observation_assets(&mut obs, r2_url, game_id, version);
                        }

                        if obs.tick != last_tick {
                            // New tick - send immediately
                            last_tick = obs.tick;
                            same_tick_count = 0;
                            if let Ok(json) = serde_json::to_vec(&obs) {
                                let msg = if let Some(compressed) = gzip_compress(&json) {
                                    Message::Binary(compressed.into())
                                } else {
                                    Message::Text(String::from_utf8_lossy(&json).into_owned().into())
                                };
                                if sender.send(msg).await.is_err() {
                                    break;
                                }
                            }
                        } else {
                            // Same tick - send occasionally to keep connection alive
                            // and help client detect if updates have stopped
                            same_tick_count += 1;
                            // Send every ~5th check (~150ms) when no new ticks
                            if same_tick_count >= 5 {
                                same_tick_count = 0;
                                if let Ok(json) = serde_json::to_vec(&obs) {
                                    let msg = if let Some(compressed) = gzip_compress(&json) {
                                        Message::Binary(compressed.into())
                                    } else {
                                        Message::Text(String::from_utf8_lossy(&json).into_owned().into())
                                    };
                                    if sender.send(msg).await.is_err() {
                                        break;
                                    }
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

async fn handle_play_ws(
    socket: WebSocket,
    state: GameplayState,
    game_id: Uuid,
    requested_name: Option<String>,
) {
    const INPUT_TICK_MS: u64 = 16;
    const MOVE_DEADZONE: f32 = 0.01;
    const MOVE_TARGET_DISTANCE: f32 = 10.0;

    let (mut sender, mut receiver) = socket.split();

    // First verify game exists in database and get script + max_players
    let db_game: Option<Game> =
        sqlx::query_as("SELECT * FROM games WHERE id = $1")
            .bind(game_id)
            .fetch_optional(&state.pool)
            .await
            .ok()
            .flatten();

    let Some(db_game) = db_game else {
        send_play_error(&mut sender, "Game not found").await;
        return;
    };

    // Auto-start the game instance if not running
    if !game::is_instance_running(&state.game_manager, game_id) {
        game::find_or_create_instance(
            &state.game_manager,
            game_id,
            db_game.max_players as u32,
            db_game.script_code.as_deref(),
        );
    }

    let find_result = game::find_or_create_instance(
        &state.game_manager,
        game_id,
        db_game.max_players as u32,
        db_game.script_code.as_deref(),
    );

    let player_id = Uuid::new_v4();
    let player_name = sanitized_player_name(requested_name.as_deref(), player_id);

    if let Err(err) = game::join_instance(
        &state.game_manager,
        find_result.instance_id,
        game_id,
        player_id,
        &player_name,
    ) {
        send_play_error(&mut sender, &format!("Failed to join instance: {err}")).await;
        return;
    }

    let joined_payload = PlayJoinedMessage {
        message_type: "joined",
        player_id,
        instance_id: find_result.instance_id,
    };
    if let Ok(json) = serde_json::to_string(&joined_payload) {
        if sender.send(Message::Text(json.into())).await.is_err() {
            let _ = game::leave_instance(&state.game_manager, find_result.instance_id, player_id);
            return;
        }
    }

    let mut intent = PlayIntentState::default();
    let mut tick_interval = interval(Duration::from_millis(INPUT_TICK_MS));

    loop {
        tokio::select! {
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(data))) => {
                        if sender.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<PlayClientMessage>(&text) {
                            Ok(PlayClientMessage::Intent(incoming)) => {
                                if let Some(move_axis) = incoming.move_axis {
                                    let (mx, mz) = normalize_clamped_axis(move_axis);
                                    intent.move_x = mx;
                                    intent.move_z = mz;
                                }

                                if incoming.camera_yaw.is_finite() {
                                    intent.camera_yaw = incoming.camera_yaw;
                                }

                                if incoming.buttons.jump && !intent.jump_pressed {
                                    intent.jump_pending = true;
                                }
                                intent.jump_pressed = incoming.buttons.jump;

                                if incoming.buttons.dive && !intent.dive_pressed {
                                    intent.dive_pending = true;
                                }
                                intent.dive_pressed = incoming.buttons.dive;

                                if let Some(seq) = incoming.seq {
                                    let ack = PlayAckMessage {
                                        message_type: "ack",
                                        seq,
                                    };
                                    if let Ok(json) = serde_json::to_string(&ack) {
                                        if sender.send(Message::Text(json.into())).await.is_err() {
                                            break;
                                        }
                                    }
                                }
                            }
                            Err(_) => {
                                send_play_error(&mut sender, "Invalid play message").await;
                            }
                        }
                    }
                    Some(Ok(Message::Binary(data))) => {
                        if let Ok(text) = String::from_utf8(data.to_vec()) {
                            match serde_json::from_str::<PlayClientMessage>(&text) {
                                Ok(PlayClientMessage::Intent(incoming)) => {
                                    if let Some(move_axis) = incoming.move_axis {
                                        let (mx, mz) = normalize_clamped_axis(move_axis);
                                        intent.move_x = mx;
                                        intent.move_z = mz;
                                    }

                                    if incoming.camera_yaw.is_finite() {
                                        intent.camera_yaw = incoming.camera_yaw;
                                    }

                                    if incoming.buttons.jump && !intent.jump_pressed {
                                        intent.jump_pending = true;
                                    }
                                    intent.jump_pressed = incoming.buttons.jump;

                                    if incoming.buttons.dive && !intent.dive_pressed {
                                        intent.dive_pending = true;
                                    }
                                    intent.dive_pressed = incoming.buttons.dive;

                                    if let Some(seq) = incoming.seq {
                                        let ack = PlayAckMessage {
                                            message_type: "ack",
                                            seq,
                                        };
                                        if let Ok(json) = serde_json::to_string(&ack) {
                                            if sender.send(Message::Text(json.into())).await.is_err() {
                                                break;
                                            }
                                        }
                                    }
                                }
                                Err(_) => {
                                    send_play_error(&mut sender, "Invalid play message").await;
                                }
                            }
                        } else {
                            send_play_error(&mut sender, "Play messages must be utf-8 JSON").await;
                        }
                    }
                    _ => {}
                }
            }

            _ = tick_interval.tick() => {
                let moving = intent.move_x.abs() > MOVE_DEADZONE || intent.move_z.abs() > MOVE_DEADZONE;

                if moving {
                    let obs = match game::get_observation(&state.game_manager, game_id, player_id) {
                        Ok(o) => o,
                        Err(err) => {
                            send_play_error(&mut sender, &format!("Observation failed: {err}")).await;
                            break;
                        }
                    };

                    let [px, py, pz] = obs.player.position;
                    let (world_x, world_z) = camera_relative_move_to_world(intent.move_x, intent.move_z, intent.camera_yaw);
                    let len = (world_x * world_x + world_z * world_z).sqrt();
                    if len > 0.0001 {
                        let target_x = px + (world_x / len) * MOVE_TARGET_DISTANCE;
                        let target_z = pz + (world_z / len) * MOVE_TARGET_DISTANCE;

                        if let Err(err) = game::queue_input(
                            &state.game_manager,
                            game_id,
                            player_id,
                            "MoveTo".to_string(),
                            serde_json::json!({ "position": [target_x, py, target_z] }),
                        ) {
                            send_play_error(&mut sender, &format!("MoveTo failed: {err}")).await;
                            break;
                        }
                        intent.was_moving = true;
                    }
                } else if intent.was_moving {
                    if let Err(err) = game::queue_input(
                        &state.game_manager,
                        game_id,
                        player_id,
                        "Stop".to_string(),
                        serde_json::json!({}),
                    ) {
                        send_play_error(&mut sender, &format!("Stop failed: {err}")).await;
                        break;
                    }
                    intent.was_moving = false;
                }

                if intent.jump_pending {
                    if let Err(err) = game::queue_input(
                        &state.game_manager,
                        game_id,
                        player_id,
                        "Jump".to_string(),
                        serde_json::json!({}),
                    ) {
                        send_play_error(&mut sender, &format!("Jump failed: {err}")).await;
                        break;
                    }
                    intent.jump_pending = false;
                }

                if intent.dive_pending {
                    if let Err(err) = game::queue_input(
                        &state.game_manager,
                        game_id,
                        player_id,
                        "Dive".to_string(),
                        serde_json::json!({}),
                    ) {
                        send_play_error(&mut sender, &format!("Dive failed: {err}")).await;
                        break;
                    }
                    intent.dive_pending = false;
                }
            }
        }
    }

    let _ = game::leave_instance(&state.game_manager, find_result.instance_id, player_id);
}
