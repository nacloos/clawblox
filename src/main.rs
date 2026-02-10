mod api;

use clawblox::{db, game, r2};
use game::instance::ErrorMode;

use axum::Router;
use futures_util::FutureExt;
use std::net::SocketAddr;
use std::panic;
use std::thread;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};

use game::GameManager;

#[tokio::main]
async fn main() {
    panic::set_hook(Box::new(|info| {
        let thread = std::thread::current();
        let thread_name = thread.name().unwrap_or("<unnamed>");
        let thread_id = format!("{:?}", thread.id());
        let backtrace = std::backtrace::Backtrace::force_capture();
        eprintln!(
            "[PANIC] thread={} id={} {}\n{}",
            thread_name, thread_id, info, backtrace
        );
    }));

    let pool = db::create_pool()
        .await
        .expect("Failed to connect to database");
    let pool = std::sync::Arc::new(pool);

    // Clean up orphaned instances from previous server session
    if let Err(e) = db::reconcile_instances(&pool).await {
        eprintln!("[Startup] Warning: Failed to reconcile instances: {}", e);
    }

    // Initialize R2 asset storage (optional — asset uploads disabled if not configured)
    let r2_client = r2::R2Client::from_env();
    if r2_client.is_some() {
        println!("R2 asset storage: enabled");
    } else {
        eprintln!("Warning: R2 not configured (R2_ACCOUNT_ID missing). Asset uploads disabled.");
    }

    let (game_manager, game_handle) = GameManager::new(60, pool.clone(), ErrorMode::Continue);

    // Clone handle for background sync task
    let sync_handle = game_handle.clone();
    let sync_pool = pool.clone();
    tokio::spawn(async move {
        let result = panic::AssertUnwindSafe(db::sync_instances_to_db(sync_pool, sync_handle))
            .catch_unwind()
            .await;
        if let Err(payload) = result {
            game::panic_reporting::log_panic(
                "tokio_task",
                "sync_instances_to_db",
                &*payload,
            );
        }
    });

    let game_thread = thread::spawn(move || {
        game_manager.run();
    });
    thread::spawn(move || {
        if let Err(payload) = game_thread.join() {
            game::panic_reporting::log_panic(
                "game_manager_thread",
                "game_manager.run",
                &*payload,
            );
            std::process::exit(1);
        }
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let frontend_dir = ServeDir::new("frontend/dist")
        .not_found_service(ServeFile::new("frontend/dist/index.html"));

    let app = Router::new()
        .nest(
            "/api/v1",
            api::routes((*pool).clone(), game_handle, r2_client),
        )
        .route_service("/skill.md", ServeFile::new("static/skill.md"))
        .route_service("/install.sh", ServeFile::new("scripts/install.sh"))
        .route_service("/install.ps1", ServeFile::new("scripts/install.ps1"))
        .route_service("/install.cmd", ServeFile::new("scripts/install.cmd"))
        .nest_service("/static", ServeDir::new("static"))
        .fallback_service(frontend_dir)
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
