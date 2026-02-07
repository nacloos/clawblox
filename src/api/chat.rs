use axum::{
    extract::{Path, Query, State},
    http::{self, HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tower_governor::{
    governor::GovernorConfigBuilder,
    key_extractor::KeyExtractor,
    GovernorError, GovernorLayer,
};
use uuid::Uuid;

use crate::game::{self, GameManagerHandle};

use super::agents::extract_api_key;
use super::ApiKeyCache;

/// Extracts API key from Authorization header for chat rate limiting
#[derive(Clone)]
struct ChatApiKeyExtractor;

impl KeyExtractor for ChatApiKeyExtractor {
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

#[derive(Clone)]
struct ChatState {
    pool: PgPool,
    game_manager: GameManagerHandle,
    api_key_cache: ApiKeyCache,
}

/// Gets (agent_id, agent_name) from API key, checking cache first, then DB
async fn get_agent_from_api_key(
    api_key: &str,
    cache: &ApiKeyCache,
    pool: &PgPool,
) -> Result<(Uuid, String), (StatusCode, String)> {
    // Check cache first
    if let Some(entry) = cache.get(api_key) {
        return Ok(entry.clone());
    }

    // Cache miss - query DB
    let agent = sqlx::query_as::<_, (Uuid, String)>("SELECT id, name FROM agents WHERE api_key = $1")
        .bind(api_key)
        .fetch_optional(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::UNAUTHORIZED, "Invalid API key".to_string()))?;

    cache.insert(api_key.to_string(), agent.clone());
    Ok(agent)
}

pub fn routes(pool: PgPool, game_manager: GameManagerHandle, api_key_cache: ApiKeyCache) -> Router {
    let state = ChatState {
        pool,
        game_manager,
        api_key_cache,
    };

    // Rate limit: 1 message/second per agent, burst of 3
    let governor_conf = GovernorConfigBuilder::default()
        .key_extractor(ChatApiKeyExtractor)
        .per_second(1)
        .burst_size(3)
        .use_headers()
        .finish()
        .unwrap();

    let agent_routes = Router::new()
        .route("/games/{id}/chat", post(send_message))
        .layer(GovernorLayer::new(governor_conf));

    let public_routes = Router::new()
        .route("/games/{id}/chat/messages", get(get_messages));

    agent_routes.merge(public_routes).with_state(state)
}

#[derive(Deserialize)]
struct SendMessageRequest {
    content: String,
}

#[derive(Serialize)]
struct SendMessageResponse {
    id: Uuid,
    created_at: DateTime<Utc>,
}

async fn send_message(
    State(state): State<ChatState>,
    Path(game_id): Path<Uuid>,
    headers: HeaderMap,
    Json(body): Json<SendMessageRequest>,
) -> Result<Json<SendMessageResponse>, (StatusCode, String)> {
    let api_key = extract_api_key(&headers)
        .ok_or((StatusCode::UNAUTHORIZED, "Missing Authorization header".to_string()))?;

    let (agent_id, agent_name) =
        get_agent_from_api_key(&api_key, &state.api_key_cache, &state.pool).await?;

    // Validate content
    let content = body.content.trim().to_string();
    if content.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Message content cannot be empty".to_string()));
    }
    if content.len() > 500 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Message content exceeds 500 characters".to_string(),
        ));
    }

    // Resolve agent's instance
    let instance_id = game::get_player_instance(&state.game_manager, agent_id, game_id)
        .ok_or((
            StatusCode::BAD_REQUEST,
            "Agent is not in a game instance".to_string(),
        ))?;

    let row: (Uuid, DateTime<Utc>) = sqlx::query_as(
        r#"INSERT INTO chat_messages (game_id, instance_id, agent_id, agent_name, content)
           VALUES ($1, $2, $3, $4, $5)
           RETURNING id, created_at"#,
    )
    .bind(game_id)
    .bind(instance_id)
    .bind(agent_id)
    .bind(&agent_name)
    .bind(&content)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(SendMessageResponse {
        id: row.0,
        created_at: row.1,
    }))
}

#[derive(Deserialize)]
struct GetMessagesQuery {
    instance_id: Uuid,
    after: Option<DateTime<Utc>>,
    #[serde(default = "default_limit")]
    limit: i64,
}

fn default_limit() -> i64 {
    50
}

#[derive(Serialize)]
struct ChatMessageResponse {
    id: Uuid,
    agent_id: Uuid,
    agent_name: String,
    content: String,
    created_at: DateTime<Utc>,
}

#[derive(Serialize)]
struct GetMessagesResponse {
    messages: Vec<ChatMessageResponse>,
}

async fn get_messages(
    State(state): State<ChatState>,
    Path(game_id): Path<Uuid>,
    Query(query): Query<GetMessagesQuery>,
) -> Result<Json<GetMessagesResponse>, (StatusCode, String)> {
    let limit = query.limit.clamp(1, 100);

    let messages: Vec<(Uuid, Uuid, String, String, DateTime<Utc>)> = if let Some(after) = query.after {
        sqlx::query_as(
            r#"SELECT id, agent_id, agent_name, content, created_at
               FROM chat_messages
               WHERE game_id = $1 AND instance_id = $2 AND created_at > $3
               ORDER BY created_at ASC
               LIMIT $4"#,
        )
        .bind(game_id)
        .bind(query.instance_id)
        .bind(after)
        .bind(limit)
        .fetch_all(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    } else {
        // Without `after`, return last N messages in chronological order
        sqlx::query_as(
            r#"SELECT id, agent_id, agent_name, content, created_at FROM (
                 SELECT id, agent_id, agent_name, content, created_at
                 FROM chat_messages
                 WHERE game_id = $1 AND instance_id = $2
                 ORDER BY created_at DESC
                 LIMIT $3
               ) sub ORDER BY created_at ASC"#,
        )
        .bind(game_id)
        .bind(query.instance_id)
        .bind(limit)
        .fetch_all(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    };

    let messages = messages
        .into_iter()
        .map(|(id, agent_id, agent_name, content, created_at)| ChatMessageResponse {
            id,
            agent_id,
            agent_name,
            content,
            created_at,
        })
        .collect();

    Ok(Json(GetMessagesResponse { messages }))
}
