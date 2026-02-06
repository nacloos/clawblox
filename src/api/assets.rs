use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::post,
    Json, Router,
};
use axum::extract::DefaultBodyLimit;
use flate2::read::GzDecoder;
use serde::Serialize;
use sqlx::PgPool;
use std::io::Read;
use tar::Archive;
use uuid::Uuid;

use super::agents::extract_api_key;
use super::ApiKeyCache;
use crate::r2::R2Client;

const MAX_EXTRACTED_BYTES: usize = 100 * 1024 * 1024; // 100MB
const MAX_FILE_COUNT: usize = 100;

const ALLOWED_EXTENSIONS: &[&str] = &[
    "glb", "gltf", "png", "jpg", "jpeg", "wav", "mp3", "ogg", "bin",
];

#[derive(Clone)]
pub struct AssetsState {
    pub pool: PgPool,
    pub r2: R2Client,
}

pub fn routes(pool: PgPool, _api_key_cache: ApiKeyCache, r2: R2Client) -> Router {
    let state = AssetsState { pool, r2 };
    Router::new()
        .route("/games/{id}/assets", post(upload_assets))
        .layer(DefaultBodyLimit::max(50 * 1024 * 1024)) // 50MB upload limit
        .with_state(state)
}

#[derive(Serialize)]
struct UploadAssetsResponse {
    uploaded: usize,
    version: i32,
}

fn content_type_for_extension(ext: &str) -> &'static str {
    match ext {
        "glb" => "model/gltf-binary",
        "gltf" => "model/gltf+json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "bin" => "application/octet-stream",
        _ => "application/octet-stream",
    }
}

async fn upload_assets(
    State(state): State<AssetsState>,
    Path(game_id): Path<Uuid>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<UploadAssetsResponse>, (StatusCode, String)> {
    // Auth: extract API key
    let api_key = extract_api_key(&headers)
        .ok_or((StatusCode::UNAUTHORIZED, "Missing Authorization header".to_string()))?;

    // Verify API key and get agent_id
    let agent = sqlx::query_as::<_, (Uuid,)>("SELECT id FROM agents WHERE api_key = $1")
        .bind(&api_key)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::UNAUTHORIZED, "Invalid API key".to_string()))?;

    // Fetch game and verify ownership
    let game: crate::db::models::Game =
        sqlx::query_as("SELECT * FROM games WHERE id = $1")
            .bind(game_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or((StatusCode::NOT_FOUND, "Game not found".to_string()))?;

    if game.creator_id != Some(agent.0) {
        return Err((StatusCode::FORBIDDEN, "You don't own this game".to_string()));
    }

    // Increment asset version atomically
    let (new_version,): (i32,) = sqlx::query_as(
        "UPDATE games SET has_assets = true, asset_version = asset_version + 1 WHERE id = $1 RETURNING asset_version",
    )
    .bind(game_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Extract tar.gz with security validations
    let decoder = GzDecoder::new(body.as_ref());
    let mut archive = Archive::new(decoder);

    let entries = archive
        .entries()
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid archive: {}", e)))?;

    let mut total_bytes: usize = 0;
    let mut file_count: usize = 0;
    let mut uploaded: usize = 0;

    // Collect files first (can't do async inside tar iteration)
    let mut files_to_upload: Vec<(String, Vec<u8>, String)> = Vec::new();

    for entry_result in entries {
        let mut entry = entry_result
            .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid archive entry: {}", e)))?;

        // Skip directories and symlinks
        let entry_type = entry.header().entry_type();
        if entry_type.is_dir() || entry_type.is_symlink() || entry_type.is_hard_link() {
            continue;
        }

        // Only process regular files
        if !entry_type.is_file() {
            continue;
        }

        let path = entry
            .path()
            .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid path in archive: {}", e)))?
            .to_path_buf();

        let path_str = path.to_string_lossy().to_string();

        // Security: reject path traversal
        if path_str.contains("..") || path_str.starts_with('/') {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("Invalid path in archive (path traversal): {}", path_str),
            ));
        }

        // Security: extension allowlist
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();

        if !ALLOWED_EXTENSIONS.contains(&ext.as_str()) {
            return Err((
                StatusCode::BAD_REQUEST,
                format!(
                    "File type '{}' not allowed. Allowed: {}",
                    ext,
                    ALLOWED_EXTENSIONS.join(", ")
                ),
            ));
        }

        // Security: file count limit
        file_count += 1;
        if file_count > MAX_FILE_COUNT {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("Too many files (max {})", MAX_FILE_COUNT),
            ));
        }

        // Read file contents
        let mut data = Vec::new();
        entry
            .read_to_end(&mut data)
            .map_err(|e| (StatusCode::BAD_REQUEST, format!("Failed to read archive entry: {}", e)))?;

        // Security: total extracted size limit
        total_bytes += data.len();
        if total_bytes > MAX_EXTRACTED_BYTES {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("Archive too large (max {} MB extracted)", MAX_EXTRACTED_BYTES / 1024 / 1024),
            ));
        }

        let content_type = content_type_for_extension(&ext).to_string();

        // Sanitize path: strip leading ./ if present
        let clean_path = path_str.strip_prefix("./").unwrap_or(&path_str).to_string();

        files_to_upload.push((clean_path, data, content_type));
    }

    // Upload each file to R2
    for (file_path, data, content_type) in &files_to_upload {
        let key = format!("games/{}/v{}/{}", game_id, new_version, file_path);

        state
            .r2
            .upload(&key, data, content_type)
            .await
            .map_err(|e| {
                eprintln!("R2 upload error for {}: {}", file_path, e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Asset upload failed".to_string(),
                )
            })?;

        uploaded += 1;
    }

    Ok(Json(UploadAssetsResponse {
        uploaded,
        version: new_version,
    }))
}
