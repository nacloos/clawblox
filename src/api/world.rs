use axum::{routing::{get, post}, Json, Router};
use serde::{Deserialize, Serialize};

pub fn routes() -> Router {
    Router::new()
        .route("/world", get(get_world))
        .route("/agent/action", post(action))
}

#[derive(Serialize)]
struct Entity {
    id: u32,
    r#type: String,
    position: [f32; 3],
}

#[derive(Serialize)]
struct WorldState {
    tick: u64,
    entities: Vec<Entity>,
}

async fn get_world() -> Json<WorldState> {
    // Mock world state for now
    Json(WorldState {
        tick: 0,
        entities: vec![
            Entity {
                id: 1,
                r#type: "cube".to_string(),
                position: [0.0, 5.0, 0.0],
            },
            Entity {
                id: 2,
                r#type: "ground".to_string(),
                position: [0.0, 0.0, 0.0],
            },
        ],
    })
}

#[derive(Deserialize)]
struct ActionRequest {
    action: String,
}

#[derive(Serialize)]
struct ActionResponse {
    success: bool,
    message: String,
}

async fn action(Json(req): Json<ActionRequest>) -> Json<ActionResponse> {
    // Mock action handler for now
    Json(ActionResponse {
        success: true,
        message: format!("Action '{}' queued", req.action),
    })
}
