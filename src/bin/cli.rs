//! Clawblox CLI - init and run games from anywhere

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use clap::{Parser, Subcommand};
use dashmap::DashMap;
use flate2::{write::GzEncoder, Compression};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tokio::time::interval;
use tower_http::cors::{Any, CorsLayer};
use uuid::Uuid;

use clawblox::config::WorldConfig;
use clawblox::game::{
    self, find_or_create_instance,
    instance::{PlayerObservation, SpectatorObservation},
    GameManager, GameManagerHandle,
};

#[derive(Parser)]
#[command(name = "clawblox")]
#[command(about = "Clawblox game engine CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new game project
    Init {
        /// Name for the new project (creates directory)
        name: Option<String>,
    },
    /// Run a game locally without database
    Run {
        /// Path to game directory (default: current directory)
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Port to run the server on
        #[arg(short, long, default_value = "8080")]
        port: u16,
    },
    /// Install clawblox to system PATH
    Install {
        /// Target version (ignored, for compatibility)
        #[arg(default_value = "latest")]
        target: String,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { name } => init_project(name),
        Commands::Run { path, port } => run_game(path, port),
        Commands::Install { target: _ } => install_cli(),
    }
}

// =============================================================================
// Install Command
// =============================================================================

fn install_cli() {
    let current_exe = std::env::current_exe().expect("Failed to get current executable path");

    #[cfg(unix)]
    {
        let install_dir = PathBuf::from("/usr/local/bin");
        let install_path = install_dir.join("clawblox");

        // Try /usr/local/bin first, fall back to ~/.local/bin
        let (install_dir, install_path) = if install_dir.exists() &&
            std::fs::metadata(&install_dir).map(|m| !m.permissions().readonly()).unwrap_or(false) {
            (install_dir, install_path)
        } else {
            let home = home::home_dir().expect("Failed to get home directory");
            let local_bin = home.join(".local/bin");
            std::fs::create_dir_all(&local_bin).expect("Failed to create ~/.local/bin");
            (local_bin.clone(), local_bin.join("clawblox"))
        };

        // Copy binary
        std::fs::copy(&current_exe, &install_path).expect("Failed to copy binary");

        // Make executable
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&install_path, std::fs::Permissions::from_mode(0o755))
            .expect("Failed to set permissions");

        println!("Installed to: {}", install_path.display());

        // Check if in PATH
        let path_env = std::env::var("PATH").unwrap_or_default();
        if !path_env.split(':').any(|p| PathBuf::from(p) == install_dir) {
            println!();
            println!("Add to your shell profile:");
            println!("  export PATH=\"{}:$PATH\"", install_dir.display());
        }
    }

    #[cfg(windows)]
    {
        let home = home::home_dir().expect("Failed to get home directory");
        let install_dir = home.join(".clawblox").join("bin");
        std::fs::create_dir_all(&install_dir).expect("Failed to create install directory");

        let install_path = install_dir.join("clawblox.exe");
        std::fs::copy(&current_exe, &install_path).expect("Failed to copy binary");

        println!("Installed to: {}", install_path.display());

        // Add to user PATH via registry
        let path_env = std::env::var("PATH").unwrap_or_default();
        let install_dir_str = install_dir.to_string_lossy();
        if !path_env.split(';').any(|p| p == install_dir_str) {
            println!();
            println!("Adding to user PATH...");
            let _ = std::process::Command::new("powershell")
                .args([
                    "-Command",
                    &format!(
                        "[Environment]::SetEnvironmentVariable('Path', [Environment]::GetEnvironmentVariable('Path', 'User') + ';{}', 'User')",
                        install_dir_str
                    ),
                ])
                .output();
            println!("Restart your terminal to use 'clawblox' command.");
        }
    }
}

// =============================================================================
// Init Command
// =============================================================================

fn init_project(name: Option<String>) {
    let project_name = name.unwrap_or_else(|| "my-game".to_string());
    let project_dir = PathBuf::from(&project_name);

    if project_dir.exists() {
        eprintln!("Error: Directory '{}' already exists", project_name);
        std::process::exit(1);
    }

    std::fs::create_dir_all(&project_dir).expect("Failed to create project directory");

    // Create world.toml
    let world_toml = format!(
        r#"# {} - World Configuration
name = "{}"
description = "A new Clawblox game"
max_players = 8
game_type = "lua"

[scripts]
main = "main.lua"
skill = "SKILL.md"
"#,
        project_name, project_name
    );
    std::fs::write(project_dir.join("world.toml"), world_toml).expect("Failed to create world.toml");

    // Create main.lua
    let main_lua = r#"-- Game entry point
local RunService = game:GetService("RunService")
local Players = game:GetService("Players")
local AgentInputService = game:GetService("AgentInputService")

--------------------------------------------------------------------------------
-- CONFIGURATION
--------------------------------------------------------------------------------

local MAP_SIZE = 100

--------------------------------------------------------------------------------
-- GAME STATE
--------------------------------------------------------------------------------

local playerData = {}

--------------------------------------------------------------------------------
-- HELPER FUNCTIONS
--------------------------------------------------------------------------------

local function getHumanoid(player)
    local character = player.Character
    if character then
        return character:FindFirstChild("Humanoid")
    end
    return nil
end

--------------------------------------------------------------------------------
-- MAP CREATION
--------------------------------------------------------------------------------

local function createMap()
    -- Floor
    local floor = Instance.new("Part")
    floor.Name = "Floor"
    floor.Size = Vector3.new(MAP_SIZE, 2, MAP_SIZE)
    floor.Position = Vector3.new(0, -1, 0)
    floor.Anchored = true
    floor.Color = Color3.fromRGB(100, 150, 100)
    floor.Parent = Workspace

    print("Map created: " .. MAP_SIZE .. "x" .. MAP_SIZE .. " studs")
end

--------------------------------------------------------------------------------
-- PLAYER MANAGEMENT
--------------------------------------------------------------------------------

local function setupPlayer(player)
    playerData[player.UserId] = {
        name = player.Name,
    }

    -- Move to spawn
    local character = player.Character
    if character then
        local hrp = character:FindFirstChild("HumanoidRootPart")
        if hrp then
            hrp.Position = Vector3.new(0, 3, 0)
        end
    end

    print("Player joined: " .. player.Name)
end

local function cleanupPlayer(player)
    playerData[player.UserId] = nil
    print("Player left: " .. player.Name)
end

--------------------------------------------------------------------------------
-- INPUT HANDLING
--------------------------------------------------------------------------------

if AgentInputService then
    AgentInputService.InputReceived:Connect(function(player, inputType, inputData)
        if inputType == "MoveTo" and inputData and inputData.position then
            local humanoid = getHumanoid(player)
            if humanoid then
                local pos = inputData.position
                humanoid:MoveTo(Vector3.new(pos[1], pos[2], pos[3]))
            end
        end
    end)
end

--------------------------------------------------------------------------------
-- GAME LOOP
--------------------------------------------------------------------------------

Players.PlayerAdded:Connect(setupPlayer)
Players.PlayerRemoving:Connect(cleanupPlayer)

for _, player in ipairs(Players:GetPlayers()) do
    setupPlayer(player)
end

createMap()

RunService.Heartbeat:Connect(function(dt)
    -- Game logic here
end)

print("Game initialized")
"#;
    std::fs::write(project_dir.join("main.lua"), main_lua).expect("Failed to create main.lua");

    // Create SKILL.md
    let skill_md = format!(
        r#"# {}

A new Clawblox game.

## Actions

### MoveTo
Move to a position on the map.

```json
{{"type": "MoveTo", "data": {{"position": [x, y, z]}}}}
```

## Map
- {}x{} stud floor
- Center at (0, 0, 0)
"#,
        project_name, "100", "100"
    );
    std::fs::write(project_dir.join("SKILL.md"), skill_md).expect("Failed to create SKILL.md");

    // Create docs/ folder with README
    let docs_dir = project_dir.join("docs");
    std::fs::create_dir_all(&docs_dir).expect("Failed to create docs directory");

    let docs_readme = format!(
        r#"# {} Documentation

## Overview

This is a Clawblox game project.

## Game Mechanics

Describe your game mechanics here.

## API Reference

### Inputs

| Type | Data | Description |
|------|------|-------------|
| MoveTo | `{{"position": [x, y, z]}}` | Move player to position |

## Development

```bash
# Run locally
clawblox run

# Run on a specific port
clawblox run --port 9000
```
"#,
        project_name
    );
    std::fs::write(docs_dir.join("README.md"), docs_readme).expect("Failed to create docs/README.md");

    println!("Created new game project: {}", project_name);
    println!();
    println!("  cd {}", project_name);
    println!("  clawblox run");
}

// =============================================================================
// Run Command
// =============================================================================

fn run_game(path: PathBuf, port: u16) {
    let path = std::fs::canonicalize(&path).unwrap_or_else(|_| {
        eprintln!("Error: Path '{}' does not exist", path.display());
        std::process::exit(1);
    });

    // Load world config
    let config = WorldConfig::from_game_dir(&path).unwrap_or_else(|e| {
        eprintln!("Error loading world.toml: {}", e);
        std::process::exit(1);
    });

    // Load the Lua script
    let script_path = path.join(&config.scripts.main);
    let script = std::fs::read_to_string(&script_path).unwrap_or_else(|e| {
        eprintln!("Error loading {}: {}", script_path.display(), e);
        std::process::exit(1);
    });

    // Load skill.md if present
    let skill_md = config
        .scripts
        .skill
        .as_ref()
        .and_then(|skill_file| std::fs::read_to_string(path.join(skill_file)).ok());

    println!("Starting {} (max {} players)", config.name, config.max_players);
    println!("Script: {}", config.scripts.main);

    // Create game manager without database
    let (game_manager, game_handle) = GameManager::new_without_db(60);

    // Use a random game_id for this session
    let game_id = Uuid::new_v4();

    // Create the instance
    let result = find_or_create_instance(&game_handle, game_id, config.max_players, Some(&script));
    let instance_id = result.instance_id;
    println!("Instance: {}", instance_id);

    // Run game loop in background thread
    thread::spawn(move || {
        game_manager.run();
    });

    // Start HTTP server
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let state = LocalState {
            game_id,
            instance_id,
            game_handle,
            skill_md,
            sessions: Arc::new(DashMap::new()),
        };

        let cors = CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any);

        let app = Router::new()
            .route("/spectate", get(local_spectate))
            .route("/spectate/ws", get(local_spectate_ws))
            .route("/join", post(local_join))
            .route("/input", post(local_input))
            .route("/observe", get(local_observe))
            .route("/skill.md", get(local_skill))
            .with_state(state)
            .layer(cors);

        let addr = SocketAddr::from(([0, 0, 0, 0], port));
        println!();
        println!("Server running on http://localhost:{}", port);
        println!();
        println!("Endpoints:");
        println!("  GET  /spectate     - Full game state (JSON)");
        println!("  GET  /spectate/ws  - Real-time updates (WebSocket)");
        println!("  POST /join?name=X  - Join game, returns session token");
        println!("  POST /input        - Send input (requires X-Session header)");
        println!("  GET  /observe      - Player observation (requires X-Session header)");
        println!("  GET  /skill.md     - Game skill definition");

        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
        axum::serve(listener, app).await.unwrap();
    });
}

// =============================================================================
// Local Server State & Handlers
// =============================================================================

#[derive(Clone)]
struct LocalState {
    game_id: Uuid,
    instance_id: Uuid,
    game_handle: GameManagerHandle,
    skill_md: Option<String>,
    sessions: Arc<DashMap<String, (Uuid, String)>>, // token -> (agent_id, name)
}

#[derive(Deserialize)]
struct JoinQuery {
    name: String,
}

#[derive(Serialize)]
struct JoinResponse {
    session: String,
    agent_id: Uuid,
}

async fn local_join(
    State(state): State<LocalState>,
    Query(query): Query<JoinQuery>,
) -> Result<Json<JoinResponse>, (axum::http::StatusCode, String)> {
    let agent_id = Uuid::new_v4();
    let session_token = Uuid::new_v4().to_string();

    // Join the instance
    game::join_instance(
        &state.game_handle,
        state.instance_id,
        state.game_id,
        agent_id,
        &query.name,
    )
    .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e))?;

    // Store session
    state.sessions.insert(session_token.clone(), (agent_id, query.name));

    Ok(Json(JoinResponse {
        session: session_token,
        agent_id,
    }))
}

fn get_session(
    state: &LocalState,
    headers: &axum::http::HeaderMap,
) -> Result<(Uuid, String), (axum::http::StatusCode, String)> {
    let token = headers
        .get("X-Session")
        .and_then(|h| h.to_str().ok())
        .ok_or((
            axum::http::StatusCode::UNAUTHORIZED,
            "Missing X-Session header".to_string(),
        ))?;

    state
        .sessions
        .get(token)
        .map(|r| r.clone())
        .ok_or((
            axum::http::StatusCode::UNAUTHORIZED,
            "Invalid session".to_string(),
        ))
}

#[derive(Deserialize)]
struct InputRequest {
    #[serde(rename = "type")]
    input_type: String,
    #[serde(default)]
    data: serde_json::Value,
}

async fn local_input(
    State(state): State<LocalState>,
    headers: axum::http::HeaderMap,
    Json(input): Json<InputRequest>,
) -> Result<Json<PlayerObservation>, (axum::http::StatusCode, String)> {
    let (agent_id, _) = get_session(&state, &headers)?;

    game::queue_input(
        &state.game_handle,
        state.game_id,
        agent_id,
        input.input_type,
        input.data,
    )
    .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e))?;

    // Return observation
    let observation = game::get_observation(&state.game_handle, state.game_id, agent_id)
        .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e))?;

    Ok(Json(observation))
}

async fn local_observe(
    State(state): State<LocalState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<PlayerObservation>, (axum::http::StatusCode, String)> {
    let (agent_id, _) = get_session(&state, &headers)?;

    let observation = game::get_observation(&state.game_handle, state.game_id, agent_id)
        .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e))?;

    Ok(Json(observation))
}

async fn local_spectate(
    State(state): State<LocalState>,
) -> Result<Json<SpectatorObservation>, (axum::http::StatusCode, String)> {
    let observation =
        game::get_spectator_observation_for_instance(&state.game_handle, state.instance_id)
            .map_err(|e| (axum::http::StatusCode::NOT_FOUND, e))?;

    Ok(Json(observation))
}

async fn local_skill(State(state): State<LocalState>) -> impl IntoResponse {
    match state.skill_md {
        Some(content) => (
            axum::http::StatusCode::OK,
            [("content-type", "text/markdown; charset=utf-8")],
            content,
        ),
        None => (
            axum::http::StatusCode::NOT_FOUND,
            [("content-type", "text/plain")],
            "No skill.md found".to_string(),
        ),
    }
}

async fn local_spectate_ws(
    State(state): State<LocalState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_spectate_ws(socket, state))
}

fn gzip_compress(data: &[u8]) -> Option<Vec<u8>> {
    const MIN_SIZE_FOR_COMPRESSION: usize = 1024;
    if data.len() < MIN_SIZE_FOR_COMPRESSION {
        return None;
    }
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(data).ok()?;
    encoder.finish().ok()
}

async fn handle_spectate_ws(socket: WebSocket, state: LocalState) {
    let (mut sender, mut receiver) = socket.split();

    let mut tick_interval = interval(Duration::from_millis(33));
    let mut last_tick: u64 = 0;
    let mut same_tick_count: u32 = 0;

    loop {
        tokio::select! {
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(data))) => {
                        if sender.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    _ => {}
                }
            }

            _ = tick_interval.tick() => {
                let observation =
                    game::get_spectator_observation_for_instance(&state.game_handle, state.instance_id);

                match observation {
                    Ok(obs) => {
                        if obs.tick != last_tick {
                            last_tick = obs.tick;
                            same_tick_count = 0;
                            if let Ok(json) = serde_json::to_vec(&obs) {
                                let msg = if let Some(compressed) = gzip_compress(&json) {
                                    Message::Binary(compressed.into())
                                } else {
                                    Message::Text(String::from_utf8_lossy(&json).into_owned().into())
                                };
                                if sender.send(msg).await.is_err() {
                                    break;
                                }
                            }
                        } else {
                            same_tick_count += 1;
                            if same_tick_count >= 5 {
                                same_tick_count = 0;
                                if let Ok(json) = serde_json::to_vec(&obs) {
                                    let msg = if let Some(compressed) = gzip_compress(&json) {
                                        Message::Binary(compressed.into())
                                    } else {
                                        Message::Text(String::from_utf8_lossy(&json).into_owned().into())
                                    };
                                    if sender.send(msg).await.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    Err(_) => {
                        let _ = sender
                            .send(Message::Text(r#"{"error":"Game ended"}"#.to_string().into()))
                            .await;
                        break;
                    }
                }
            }
        }
    }
}
