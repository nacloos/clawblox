pub mod agents;
mod assets;
mod chat;
mod games;
mod gameplay;

use axum::{routing::get, Router};
use dashmap::DashMap;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

use crate::game::GameManagerHandle;
use crate::r2::R2Client;

/// Shared cache for API key -> (agent_id, agent_name) lookups (avoids DB query on every request)
pub type ApiKeyCache = Arc<DashMap<String, (Uuid, String)>>;

pub fn routes(
    pool: PgPool,
    game_manager: GameManagerHandle,
    r2_client: Option<R2Client>,
) -> Router {
    let api_key_cache: ApiKeyCache = Arc::new(DashMap::new());
    let r2_public_url = r2_client.as_ref().map(|r2| r2.base_url().to_string());

    let mut router = Router::new()
        .route("/health", get(health))
        .merge(agents::routes(pool.clone(), api_key_cache.clone()))
        .merge(games::routes(
            pool.clone(),
            game_manager.clone(),
            api_key_cache.clone(),
        ))
        .merge(gameplay::routes(
            pool.clone(),
            game_manager.clone(),
            api_key_cache.clone(),
            r2_public_url,
        ))
        .merge(chat::routes(
            pool.clone(),
            game_manager,
            api_key_cache.clone(),
            r2_client.clone(),
        ));

    if let Some(r2) = r2_client {
        router = router.merge(assets::routes(pool, api_key_cache, r2));
    }

    router
}

async fn health() -> &'static str {
    r#"{"status":"ok"}"#
}
