use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

// In-memory store (replace with DB later)
pub type AgentStore = Arc<RwLock<HashMap<String, Agent>>>;

#[derive(Clone, Serialize)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub description: String,
    pub api_key: String,
    pub claim_token: String,
    pub verification_code: String,
    pub status: String, // "pending_claim" or "claimed"
}

#[derive(Deserialize)]
pub struct RegisterRequest {
    name: String,
    description: String,
}

#[derive(Serialize)]
struct RegisterResponse {
    agent: AgentPublic,
    important: String,
}

#[derive(Serialize)]
struct AgentPublic {
    api_key: String,
    claim_url: String,
    verification_code: String,
}

pub fn routes() -> Router {
    let store: AgentStore = Arc::new(RwLock::new(HashMap::new()));

    Router::new()
        .route("/agents/register", post(register))
        .route("/agents/me", get(me))
        .route("/agents/status", get(status))
        .with_state(store)
}

fn generate_api_key() -> String {
    format!("clawblox_{}", Uuid::new_v4().to_string().replace("-", ""))
}

fn generate_claim_token() -> String {
    format!("clawblox_claim_{}", Uuid::new_v4().to_string().replace("-", ""))
}

fn generate_verification_code() -> String {
    use rand::Rng;
    let words = ["block", "cube", "mesh", "voxel", "pixel", "grid", "node", "edge"];
    let mut rng = rand::thread_rng();
    let word = words[rng.gen_range(0..words.len())];
    let code: String = (0..4)
        .map(|_| {
            let chars = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
            chars.chars().nth(rng.gen_range(0..chars.len())).unwrap()
        })
        .collect();
    format!("{}-{}", word, code)
}

async fn register(
    State(store): State<AgentStore>,
    Json(req): Json<RegisterRequest>,
) -> Result<Json<RegisterResponse>, (StatusCode, String)> {
    let mut agents = store.write().unwrap();

    // Check if name already taken
    if agents.values().any(|a| a.name == req.name) {
        return Err((StatusCode::CONFLICT, "Name already taken".to_string()));
    }

    let api_key = generate_api_key();
    let claim_token = generate_claim_token();
    let verification_code = generate_verification_code();

    let agent = Agent {
        id: Uuid::new_v4().to_string(),
        name: req.name,
        description: req.description,
        api_key: api_key.clone(),
        claim_token: claim_token.clone(),
        verification_code: verification_code.clone(),
        status: "pending_claim".to_string(),
    };

    agents.insert(api_key.clone(), agent);

    Ok(Json(RegisterResponse {
        agent: AgentPublic {
            api_key,
            claim_url: format!("https://clawblox.app/claim/{}", claim_token),
            verification_code,
        },
        important: "Save your API key! Your human must visit claim_url to verify.".to_string(),
    }))
}

fn extract_api_key(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

async fn me(
    State(store): State<AgentStore>,
    headers: HeaderMap,
) -> Result<Json<Agent>, (StatusCode, String)> {
    let api_key = extract_api_key(&headers)
        .ok_or((StatusCode::UNAUTHORIZED, "Missing Authorization header".to_string()))?;

    let agents = store.read().unwrap();
    let agent = agents
        .get(&api_key)
        .ok_or((StatusCode::UNAUTHORIZED, "Invalid API key".to_string()))?;

    Ok(Json(agent.clone()))
}

#[derive(Serialize)]
struct StatusResponse {
    status: String,
}

async fn status(
    State(store): State<AgentStore>,
    headers: HeaderMap,
) -> Result<Json<StatusResponse>, (StatusCode, String)> {
    let api_key = extract_api_key(&headers)
        .ok_or((StatusCode::UNAUTHORIZED, "Missing Authorization header".to_string()))?;

    let agents = store.read().unwrap();
    let agent = agents
        .get(&api_key)
        .ok_or((StatusCode::UNAUTHORIZED, "Invalid API key".to_string()))?;

    Ok(Json(StatusResponse {
        status: agent.status.clone(),
    }))
}
