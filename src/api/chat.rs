use axum::{
    extract::{DefaultBodyLimit, Multipart, Path, Query, State},
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
use crate::r2::R2Client;

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
    r2: Option<R2Client>,
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

const MAX_AUDIO_SIZE: usize = 5 * 1024 * 1024; // 5MB

pub fn routes(
    pool: PgPool,
    game_manager: GameManagerHandle,
    api_key_cache: ApiKeyCache,
    r2: Option<R2Client>,
) -> Router {
    let state = ChatState {
        pool,
        game_manager,
        api_key_cache,
        r2: r2.clone(),
    };

    // Rate limit: 1 message/second per agent, burst of 3
    let governor_conf = GovernorConfigBuilder::default()
        .key_extractor(ChatApiKeyExtractor)
        .per_second(1)
        .burst_size(3)
        .use_headers()
        .finish()
        .unwrap();

    let mut agent_routes = Router::new()
        .route("/games/{id}/chat", post(send_message));

    if r2.is_some() {
        agent_routes = agent_routes.route(
            "/games/{id}/chat/voice",
            post(send_voice_message)
                .layer(DefaultBodyLimit::max(MAX_AUDIO_SIZE)),
        );
    }

    agent_routes = agent_routes.layer(GovernorLayer::new(governor_conf));

    let public_routes = Router::new()
        .route("/games/{id}/chat/messages", get(get_messages));

    agent_routes.merge(public_routes).with_state(state)
}

#[derive(Deserialize)]
struct SendMessageRequest {
    content: String,
    media_url: Option<String>,
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

    // Validate media_url if provided
    let (message_type, media_url) = if let Some(ref url_str) = body.media_url {
        if url_str.len() > 2048 {
            return Err((
                StatusCode::BAD_REQUEST,
                "media_url exceeds 2048 characters".to_string(),
            ));
        }
        if !url_str.starts_with("https://") {
            return Err((
                StatusCode::BAD_REQUEST,
                "media_url must use HTTPS".to_string(),
            ));
        }
        ("voice".to_string(), Some(url_str.clone()))
    } else {
        ("text".to_string(), None)
    };

    // Resolve agent's instance
    let instance_id = game::get_player_instance(&state.game_manager, agent_id, game_id)
        .ok_or((
            StatusCode::BAD_REQUEST,
            "Agent is not in a game instance".to_string(),
        ))?;

    let row: (Uuid, DateTime<Utc>) = sqlx::query_as(
        r#"INSERT INTO chat_messages (game_id, instance_id, agent_id, agent_name, content, message_type, media_url)
           VALUES ($1, $2, $3, $4, $5, $6, $7)
           RETURNING id, created_at"#,
    )
    .bind(game_id)
    .bind(instance_id)
    .bind(agent_id)
    .bind(&agent_name)
    .bind(&content)
    .bind(&message_type)
    .bind(&media_url)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(SendMessageResponse {
        id: row.0,
        created_at: row.1,
    }))
}

#[derive(Serialize)]
struct VoiceMessageResponse {
    id: Uuid,
    media_url: String,
    created_at: DateTime<Utc>,
}

async fn send_voice_message(
    State(state): State<ChatState>,
    Path(game_id): Path<Uuid>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<VoiceMessageResponse>, (StatusCode, String)> {
    let r2 = state.r2.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Voice messages are not available (R2 not configured)".to_string(),
    ))?;

    let api_key = extract_api_key(&headers)
        .ok_or((StatusCode::UNAUTHORIZED, "Missing Authorization header".to_string()))?;

    let (agent_id, agent_name) =
        get_agent_from_api_key(&api_key, &state.api_key_cache, &state.pool).await?;

    let mut content: Option<String> = None;
    let mut audio_data: Option<Vec<u8>> = None;
    let mut audio_content_type: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid multipart data: {e}")))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "content" => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, format!("Failed to read content field: {e}")))?;
                content = Some(text);
            }
            "audio" => {
                let ct = field
                    .content_type()
                    .unwrap_or("application/octet-stream")
                    .to_string();

                if !matches!(ct.as_str(), "audio/mpeg" | "audio/wav" | "audio/ogg") {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        format!("Unsupported audio type: {ct}. Must be audio/mpeg, audio/wav, or audio/ogg"),
                    ));
                }

                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, format!("Failed to read audio field: {e}")))?;

                if bytes.is_empty() {
                    return Err((StatusCode::BAD_REQUEST, "Audio file is empty".to_string()));
                }

                audio_content_type = Some(ct);
                audio_data = Some(bytes.to_vec());
            }
            _ => {
                // Skip unknown fields
            }
        }
    }

    // Validate required fields
    let content = content
        .ok_or((StatusCode::BAD_REQUEST, "Missing 'content' field".to_string()))?;
    let content = content.trim().to_string();
    if content.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Message content cannot be empty".to_string()));
    }
    if content.len() > 500 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Message content exceeds 500 characters".to_string(),
        ));
    }

    let audio_data = audio_data
        .ok_or((StatusCode::BAD_REQUEST, "Missing 'audio' field".to_string()))?;
    let audio_content_type = audio_content_type.unwrap(); // safe: set alongside audio_data

    // Resolve agent's instance
    let instance_id = game::get_player_instance(&state.game_manager, agent_id, game_id)
        .ok_or((
            StatusCode::BAD_REQUEST,
            "Agent is not in a game instance".to_string(),
        ))?;

    // Determine file extension from content type
    let ext = match audio_content_type.as_str() {
        "audio/mpeg" => "mp3",
        "audio/wav" => "wav",
        "audio/ogg" => "ogg",
        _ => unreachable!(),
    };

    let file_id = Uuid::new_v4();
    let key = format!("chat-audio/{game_id}/{file_id}.{ext}");

    let media_url = r2
        .upload(&key, &audio_data, &audio_content_type)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let row: (Uuid, DateTime<Utc>) = sqlx::query_as(
        r#"INSERT INTO chat_messages (game_id, instance_id, agent_id, agent_name, content, message_type, media_url)
           VALUES ($1, $2, $3, $4, $5, 'voice', $6)
           RETURNING id, created_at"#,
    )
    .bind(game_id)
    .bind(instance_id)
    .bind(agent_id)
    .bind(&agent_name)
    .bind(&content)
    .bind(&media_url)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(VoiceMessageResponse {
        id: row.0,
        media_url,
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
    message_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    media_url: Option<String>,
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

    let messages: Vec<(Uuid, Uuid, String, String, String, Option<String>, DateTime<Utc>)> = if let Some(after) = query.after {
        sqlx::query_as(
            r#"SELECT id, agent_id, agent_name, content, message_type, media_url, created_at
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
            r#"SELECT id, agent_id, agent_name, content, message_type, media_url, created_at FROM (
                 SELECT id, agent_id, agent_name, content, message_type, media_url, created_at
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
        .map(|(id, agent_id, agent_name, content, message_type, media_url, created_at)| ChatMessageResponse {
            id,
            agent_id,
            agent_name,
            content,
            message_type,
            media_url,
            created_at,
        })
        .collect();

    Ok(Json(GetMessagesResponse { messages }))
}
