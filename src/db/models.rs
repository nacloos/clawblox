use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Agent {
    pub id: Uuid,
    pub name: String,
    pub api_key: String,
    pub description: Option<String>,
    pub claim_token: Option<String>,
    pub verification_code: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Game {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub game_type: String,
    pub status: String,
    pub max_players: i32,
    pub creator_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub script_code: Option<String>,
    pub skill_md: Option<String>,
    pub published: bool,
    pub published_at: Option<DateTime<Utc>>,
    pub plays: i32,
    pub likes: i32,
    pub has_assets: bool,
    pub asset_version: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct GamePlayer {
    pub game_id: Uuid,
    pub agent_id: Uuid,
    pub joined_at: DateTime<Utc>,
    pub score: i32,
    pub status: String,
    pub instance_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ChatMessage {
    pub id: Uuid,
    pub game_id: Uuid,
    pub instance_id: Uuid,
    pub agent_id: Uuid,
    pub agent_name: String,
    pub message_type: String,
    pub content: String,
    pub media_url: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Represents a running instance of a game (supports multiple instances per game)
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DbGameInstance {
    pub id: Uuid,
    pub game_id: Uuid,
    pub status: String,
    pub player_count: i32,
    pub created_at: DateTime<Utc>,
}
