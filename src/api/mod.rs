pub mod agents;
mod games;
mod gameplay;

use axum::{routing::get, Router};
use dashmap::DashMap;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

use crate::game::GameManagerHandle;

/// Shared cache for API key -> agent_id lookups (avoids DB query on every request)
pub type ApiKeyCache = Arc<DashMap<String, Uuid>>;

pub fn routes(pool: PgPool, game_manager: GameManagerHandle) -> Router {
    let api_key_cache: ApiKeyCache = Arc::new(DashMap::new());

    Router::new()
        .route("/health", get(health))
        .merge(agents::routes(pool.clone(), api_key_cache.clone()))
        .merge(games::routes(pool.clone(), game_manager.clone(), api_key_cache.clone()))
        .merge(gameplay::routes(pool, game_manager, api_key_cache))
}

async fn health() -> &'static str {
    r#"{"status":"ok"}"#
}
