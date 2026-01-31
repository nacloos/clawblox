pub mod agents;
mod games;
mod gameplay;

use axum::{routing::get, Router};
use sqlx::PgPool;

use crate::game::GameManagerHandle;

pub fn routes(pool: PgPool, game_manager: GameManagerHandle) -> Router {
    Router::new()
        .route("/health", get(health))
        .merge(agents::routes(pool.clone()))
        .merge(games::routes(pool.clone(), game_manager.clone()))
        .merge(gameplay::routes(pool, game_manager))
}

async fn health() -> &'static str {
    r#"{"status":"ok"}"#
}
