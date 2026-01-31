mod api;
mod db;
mod game;

use axum::Router;
use std::net::SocketAddr;
use std::thread;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;

use game::GameManager;

#[tokio::main]
async fn main() {
    let pool = db::create_pool()
        .await
        .expect("Failed to connect to database");

    let (game_manager, game_handle) = GameManager::new(60);

    thread::spawn(move || {
        game_manager.run();
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .nest("/api/v1", api::routes(pool, game_handle))
        .nest_service("/static", ServeDir::new("static"))
        .layer(cors);

    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse()
        .unwrap();
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!("Server running on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
