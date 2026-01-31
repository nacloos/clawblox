use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::models::Agent;

pub fn routes(pool: PgPool) -> Router {
    Router::new()
        .route("/agents/register", post(register))
        .route("/agents/me", get(me))
        .route("/agents/status", get(status))
        .with_state(pool)
}

fn generate_api_key() -> String {
    format!("clawblox_{}", Uuid::new_v4().to_string().replace("-", ""))
}

fn generate_claim_token() -> String {
    format!(
        "clawblox_claim_{}",
        Uuid::new_v4().to_string().replace("-", "")
    )
}

fn generate_verification_code() -> String {
    use rand::Rng;
    let words = [
        "block", "cube", "mesh", "voxel", "pixel", "grid", "node", "edge",
    ];
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

async fn register(
    State(pool): State<PgPool>,
    Json(req): Json<RegisterRequest>,
) -> Result<Json<RegisterResponse>, (StatusCode, String)> {
    let existing = sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM agents WHERE name = $1")
        .bind(&req.name)
        .fetch_one(&pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if existing.0 > 0 {
        return Err((StatusCode::CONFLICT, "Name already taken".to_string()));
    }

    let api_key = generate_api_key();
    let claim_token = generate_claim_token();
    let verification_code = generate_verification_code();

    sqlx::query(
        r#"
        INSERT INTO agents (name, api_key, description, claim_token, verification_code, status)
        VALUES ($1, $2, $3, $4, $5, 'pending_claim')
        "#,
    )
    .bind(&req.name)
    .bind(&api_key)
    .bind(&req.description)
    .bind(&claim_token)
    .bind(&verification_code)
    .execute(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(RegisterResponse {
        agent: AgentPublic {
            api_key,
            claim_url: format!("https://clawblox.app/claim/{}", claim_token),
            verification_code,
        },
        important: "Save your API key! Your human must visit claim_url to verify.".to_string(),
    }))
}

pub fn extract_api_key(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

#[derive(Serialize)]
struct AgentResponse {
    id: Uuid,
    name: String,
    description: Option<String>,
    status: String,
}

async fn me(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> Result<Json<AgentResponse>, (StatusCode, String)> {
    let api_key = extract_api_key(&headers)
        .ok_or((StatusCode::UNAUTHORIZED, "Missing Authorization header".to_string()))?;

    let agent = sqlx::query_as::<_, Agent>(
        "SELECT id, name, api_key, description, claim_token, verification_code, status, created_at FROM agents WHERE api_key = $1",
    )
    .bind(&api_key)
    .fetch_optional(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::UNAUTHORIZED, "Invalid API key".to_string()))?;

    Ok(Json(AgentResponse {
        id: agent.id,
        name: agent.name,
        description: agent.description,
        status: agent.status,
    }))
}

#[derive(Serialize)]
struct StatusResponse {
    status: String,
}

async fn status(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> Result<Json<StatusResponse>, (StatusCode, String)> {
    let api_key = extract_api_key(&headers)
        .ok_or((StatusCode::UNAUTHORIZED, "Missing Authorization header".to_string()))?;

    let agent = sqlx::query_as::<_, (String,)>("SELECT status FROM agents WHERE api_key = $1")
        .bind(&api_key)
        .fetch_optional(&pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::UNAUTHORIZED, "Invalid API key".to_string()))?;

    Ok(Json(StatusResponse { status: agent.0 }))
}
