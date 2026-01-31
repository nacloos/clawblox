mod agents;
mod world;

use axum::{routing::get, Router};

pub fn routes() -> Router {
    Router::new()
        .route("/health", get(health))
        .merge(agents::routes())
        .merge(world::routes())
}

async fn health() -> &'static str {
    r#"{"status":"ok"}"#
}
