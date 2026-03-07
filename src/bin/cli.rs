//! Clawblox CLI - init and run games from anywhere

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path as AxumPath, Query, State,
    },
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use clap::{Parser, Subcommand, ValueEnum};
use dashmap::DashMap;
use flate2::{write::GzEncoder, Compression};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use chrono::{SecondsFormat, Utc};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::fs::OpenOptions;
use std::io::Write;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, watch};
use tokio::task::JoinHandle;
use tokio::time::interval;
use tower_http::cors::{Any, CorsLayer};
use uuid::Uuid;

#[cfg(unix)]
use libc;

use include_dir::{include_dir, Dir};
use tower_http::services::ServeDir;

use clawblox::{
    db,
    recording, replay,
    game::{
        constants::audio::{
            DEFAULT_BYTES_PER_SAMPLE, DEFAULT_CHANNELS, DEFAULT_PLAYBACK_SPEED,
            DEFAULT_SAMPLE_RATE_HZ, MIN_PLAYBACK_SPEED, MIN_POSITIVE_U16, MIN_POSITIVE_U32,
            SERVER_PREDICTED_END_REASON, STREAM_TTL_SECS,
        },
        self,
        async_bridge::AsyncBridge,
        find_or_create_instance,
        instance::{ErrorMode, PlayerObservation, SpectatorObservation},
        script_bundle::load_world_and_entry_script,
        GameManager, GameManagerHandle,
    },
};

static DOCS_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/docs");
static LOCAL_RUNTIME_UI_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/frontend_runtime/dist");

#[derive(Copy, Clone, Debug, ValueEnum)]
enum LocalDatastore {
    Sqlite,
    Postgres,
    #[value(name = "none")]
    NoDb,
}

impl LocalDatastore {
    fn as_arg(self) -> &'static str {
        match self {
            Self::Sqlite => "sqlite",
            Self::Postgres => "postgres",
            Self::NoDb => "none",
        }
    }
}

#[derive(Parser)]
#[command(name = "clawblox")]
#[command(about = "Clawblox game engine CLI", long_about = None, version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new game project
    Init {
        /// Name for the new project (creates subdirectory). Omit to init in current directory.
        name: Option<String>,
    },
    /// Run a game locally
    Run {
        /// Path to game directory (default: current directory)
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Port to run the server on
        #[arg(short, long, default_value = "8080")]
        port: u16,
        /// Run as daemon (internal use only)
        #[arg(long, hide = true)]
        daemon: bool,
        /// Datastore backend for local DataStoreService
        #[arg(long, value_enum, default_value_t = LocalDatastore::Sqlite)]
        datastore: LocalDatastore,
        /// Record session to a .clawrec file for later replay
        #[arg(long)]
        record: bool,
    },
    /// Replay a recorded session
    Replay {
        /// Path to .clawrec recording file
        file: PathBuf,
        /// Playback speed multiplier
        #[arg(long, default_value = "1.0")]
        speed: f32,
        /// Port to run the replay server on
        #[arg(short, long, default_value = "8080")]
        port: u16,
        /// Override the world directory (for serving assets)
        #[arg(long)]
        world_dir: Option<PathBuf>,
    },
    /// Start a game locally in background (daemon mode)
    Start {
        /// Path to game directory (default: current directory)
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Port to run the server on
        #[arg(short, long, default_value = "8080")]
        port: u16,
        /// Datastore backend for local DataStoreService
        #[arg(long, value_enum, default_value_t = LocalDatastore::Sqlite)]
        datastore: LocalDatastore,
    },
    /// Stop a background game server started with `clawblox start`
    Stop {
        /// Port of the server to stop
        #[arg(short, long, default_value = "8080")]
        port: u16,
    },
    /// Log in or register an account on clawblox.com
    Login {
        /// Your name (registers a new account)
        name: Option<String>,
        /// Use an existing API key instead of registering
        #[arg(long)]
        api_key: Option<String>,
        /// Server URL
        #[arg(long, default_value = "https://clawblox.com")]
        server: String,
    },
    /// Deploy a game to clawblox.com
    Deploy {
        /// Path to game directory
        #[arg(default_value = ".")]
        path: PathBuf,
        /// API key (overrides stored credentials)
        #[arg(long, env = "CLAWBLOX_API_KEY")]
        api_key: Option<String>,
        /// Server URL (overrides stored credentials)
        #[arg(long)]
        server: Option<String>,
    },
    /// Install clawblox to system PATH
    Install {
        /// Target version (ignored, for compatibility)
        #[arg(default_value = "latest")]
        target: String,
    },
    /// Fetch latest engine docs from GitHub into ./docs/
    Docs {
        /// Use only bundled local docs (skip GitHub fetch)
        #[arg(long)]
        local: bool,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { name } => init_project(name),
        Commands::Run {
            path,
            port,
            daemon,
            datastore,
            record,
        } => run_game(path, port, daemon, datastore, record),
        Commands::Replay {
            file,
            speed,
            port,
            world_dir,
        } => run_replay(file, speed, port, world_dir),
        Commands::Start {
            path,
            port,
            datastore,
        } => start_game(path, port, datastore),
        Commands::Stop { port } => stop_game(port),
        Commands::Login {
            name,
            api_key,
            server,
        } => login(name, api_key, server),
        Commands::Deploy {
            path,
            api_key,
            server,
        } => deploy_game(path, api_key, server),
        Commands::Install { target: _ } => install_cli(),
        Commands::Docs { local } => fetch_docs(local),
    }
}

// =============================================================================
// Install Command
// =============================================================================

fn install_cli() {
    let current_exe = std::env::current_exe().expect("Failed to get current executable path");
    let version = env!("CARGO_PKG_VERSION");

    #[cfg(unix)]
    {
        let home = home::home_dir().expect("Failed to get home directory");

        // Install to ~/.local/share/clawblox/versions/X.X.X
        let versions_dir = home.join(".local/share/clawblox/versions");
        std::fs::create_dir_all(&versions_dir).expect("Failed to create versions directory");
        let version_path = versions_dir.join(version);
        std::fs::copy(&current_exe, &version_path).expect("Failed to copy binary");

        // Make executable
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&version_path, std::fs::Permissions::from_mode(0o755))
            .expect("Failed to set permissions");

        // Create symlink in ~/.local/bin
        let bin_dir = home.join(".local/bin");
        std::fs::create_dir_all(&bin_dir).expect("Failed to create ~/.local/bin");
        let symlink_path = bin_dir.join("clawblox");

        // Remove old symlink if exists
        let _ = std::fs::remove_file(&symlink_path);
        std::os::unix::fs::symlink(&version_path, &symlink_path).expect("Failed to create symlink");

        println!("Installed to: {}", symlink_path.display());

        // Check if in PATH
        let path_env = std::env::var("PATH").unwrap_or_default();
        if !path_env.split(':').any(|p| PathBuf::from(p) == bin_dir) {
            println!();
            println!("Add to your shell profile:");
            println!("  export PATH=\"{}:$PATH\"", bin_dir.display());
        }
    }

    #[cfg(windows)]
    {
        let home = home::home_dir().expect("Failed to get home directory");

        // Install to ~/.local/share/clawblox/versions/X.X.X
        let versions_dir = home
            .join(".local")
            .join("share")
            .join("clawblox")
            .join("versions");
        std::fs::create_dir_all(&versions_dir).expect("Failed to create versions directory");
        let version_path = versions_dir.join(format!("{}.exe", version));
        std::fs::copy(&current_exe, &version_path).expect("Failed to copy binary");

        // Create launcher in ~/.local/bin (same as Claude Code)
        let install_dir = home.join(".local").join("bin");
        std::fs::create_dir_all(&install_dir).expect("Failed to create bin directory");
        let install_path = install_dir.join("clawblox.exe");
        std::fs::copy(&version_path, &install_path).expect("Failed to copy to bin");

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
// Docs Command
// =============================================================================

fn fetch_docs(local: bool) {
    let docs_dir = std::env::current_dir()
        .expect("Failed to get current directory")
        .join("docs");

    if local {
        write_embedded_dir(&DOCS_DIR, &docs_dir);
        println!(
            "Docs written from bundled local copy to {}",
            docs_dir.display()
        );
        return;
    }

    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    match rt.block_on(fetch_docs_from_github(&docs_dir)) {
        Ok(()) => println!("Docs updated in {}", docs_dir.display()),
        Err(e) => {
            eprintln!("Error: Failed to fetch docs from GitHub: {}", e);
            std::process::exit(1);
        }
    }
}

// =============================================================================
// Init Command
// =============================================================================

fn write_embedded_dir(dir: &Dir, target: &Path) {
    std::fs::create_dir_all(target).expect("Failed to create directory");
    for file in dir.files() {
        let dest = target.join(file.path().file_name().unwrap());
        std::fs::write(&dest, file.contents()).expect("Failed to write file");
    }
    for subdir in dir.dirs() {
        let subdir_name = subdir.path().file_name().unwrap();
        write_embedded_dir(subdir, &target.join(subdir_name));
    }
}

const GITHUB_DOCS_API_URL: &str = "https://api.github.com/repos/nacloos/clawblox/contents/docs";

/// Fetch docs from GitHub and write them to `target_dir`.
/// Returns Ok(()) on success, Err with description on any failure.
async fn fetch_docs_from_github(target_dir: &Path) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .user_agent("clawblox-cli")
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    // List files in docs/ via GitHub Contents API
    let resp = client
        .get(GITHUB_DOCS_API_URL)
        .send()
        .await
        .map_err(|e| format!("Failed to reach GitHub API: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("GitHub API returned {}", resp.status()));
    }

    let entries: Vec<serde_json::Value> = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse GitHub API response: {}", e))?;

    std::fs::create_dir_all(target_dir)
        .map_err(|e| format!("Failed to create docs directory: {}", e))?;

    for entry in &entries {
        let entry_type = entry["type"].as_str().unwrap_or("");
        if entry_type != "file" {
            continue;
        }
        let name = entry["name"]
            .as_str()
            .ok_or("Missing 'name' in GitHub API entry")?;
        let download_url = entry["download_url"]
            .as_str()
            .ok_or(format!("Missing 'download_url' for {}", name))?;

        let file_resp = client
            .get(download_url)
            .send()
            .await
            .map_err(|e| format!("Failed to download {}: {}", name, e))?;

        if !file_resp.status().is_success() {
            return Err(format!(
                "Failed to download {} ({})",
                name,
                file_resp.status()
            ));
        }

        let content = file_resp
            .bytes()
            .await
            .map_err(|e| format!("Failed to read {}: {}", name, e))?;

        std::fs::write(target_dir.join(name), &content)
            .map_err(|e| format!("Failed to write {}: {}", name, e))?;
    }

    Ok(())
}

fn init_project(name: Option<String>) {
    let (project_name, project_dir) = match name {
        Some(n) => {
            let dir = PathBuf::from(&n);
            if dir.exists() {
                eprintln!("Error: Directory '{}' already exists", n);
                std::process::exit(1);
            }
            (n, dir)
        }
        None => {
            let cwd = std::env::current_dir().expect("Failed to get current directory");
            let dir_name = cwd
                .file_name()
                .expect("Failed to get directory name")
                .to_string_lossy()
                .to_string();
            if cwd.join("world.toml").exists() {
                eprintln!("Error: world.toml already exists in current directory");
                std::process::exit(1);
            }
            (dir_name, cwd)
        }
    };

    std::fs::create_dir_all(&project_dir).expect("Failed to create project directory");

    // Create world.toml
    let world_toml = format!(
        r#"# {} - World Configuration
name = "{}"
description = "A new Clawblox game"
max_players = 8

[scripts]
main = "main.lua"
skill = "SKILL.md"

[renderer]
name = "Default Game Renderer"
mode = "module"
api_version = 1
entry = "dist/renderer.bundle.js"
capabilities = []

[observation]
radius = 100
"#,
        project_name, project_name
    );
    std::fs::write(project_dir.join("world.toml"), world_toml)
        .expect("Failed to create world.toml");

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

    // Fetch latest docs from GitHub, fall back to embedded copy
    {
        let docs_dir = project_dir.join("docs");
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
        match rt.block_on(fetch_docs_from_github(&docs_dir)) {
            Ok(()) => {}
            Err(e) => {
                eprintln!(
                    "Warning: Could not fetch docs from GitHub ({}), using bundled copy",
                    e
                );
                write_embedded_dir(&DOCS_DIR, &docs_dir);
            }
        }
    }

    // Create assets/ directory for game assets (GLB models, images, audio)
    std::fs::create_dir_all(project_dir.join("assets")).expect("Failed to create assets directory");

    // Create renderer/ package for game-specific frontend modules
    let renderer_dir = project_dir.join("renderer");
    std::fs::create_dir_all(renderer_dir.join("src")).expect("Failed to create renderer directory");
    let renderer_package_json = format!(
        r#"{{
  "name": "@clawblox-renderers/{project_name}",
  "private": true,
  "type": "module",
  "clawbloxRenderer": {{
    "apiVersion": 1,
    "entry": "./src/index.js",
    "outFile": "./dist/renderer.bundle.js",
    "capabilities": []
  }}
}}
"#
    );
    std::fs::write(renderer_dir.join("package.json"), renderer_package_json)
        .expect("Failed to create renderer/package.json");
    let renderer_js = r#"import * as THREE from 'https://esm.sh/three@0.160.0'

export function createRenderer(ctx) {
  const scene = new THREE.Scene()
  scene.background = new THREE.Color(0x0b1020)

  const camera = new THREE.PerspectiveCamera(75, 1, 0.1, 500)
  const webgl = new THREE.WebGLRenderer({ canvas: ctx.canvas, antialias: true })
  webgl.shadowMap.enabled = true
  webgl.shadowMap.type = THREE.PCFSoftShadowMap

  const ambient = new THREE.AmbientLight(0xffffff, 0.7)
  const dir = new THREE.DirectionalLight(0xffffff, 0.9)
  dir.position.set(20, 30, 10)
  dir.castShadow = true
  scene.add(ambient, dir)

  const stateBuffer = ctx.runtime.state.createSnapshotBuffer({
    maxSnapshots: 12,
    interpolationDelayMs: 90,
  })
  const cameraModes = ctx.runtime.three.createCameraModeController(THREE, camera, {
    follow: { followDistance: 9, followHeight: 2, shoulderOffset: 1.0 },
    firstPersonHeight: 1.7,
  })

  const presetLib = ctx.runtime.three.createPresetMaterialLibrary({
    'example/floor': { roughness: 0.85, metalness: 0.1 },
    'example/wall': { roughness: 0.7, metalness: 0.2 },
    'example/spawn': { roughness: 0.2, metalness: 0.25, emissiveIntensity: 0.5 },
  })

  const entities = ctx.runtime.three.createEntityStore(scene)
  const tmpForward = new THREE.Vector3(0, 0, 1)
  let inputBinding = null
  let inputClient = null
  let localControlPlayer = null

  return {
    mount() {
      // Optional local control bridge for quick prototyping:
      // W -> MoveTo ahead, Space -> Jump
      inputClient = ctx.runtime.input.createLocalInputClient({ playerName: 'renderer-local' })
      inputBinding = ctx.runtime.input.bindKeyboardActions(inputClient, {
        Space: { mode: 'tap', type: 'Jump', data: {} },
      })
    },

    onResize({ width, height }) {
      camera.aspect = width / Math.max(1, height)
      camera.updateProjectionMatrix()
      webgl.setSize(width, height, false)
    },

    onState(state) {
      stateBuffer.push(state)
      const obs = stateBuffer.interpolated() || state
      const activeIds = new Set()

      for (const entity of obs.entities || []) {
        activeIds.add(entity.id)
        entities.upsert(
          entity,
          (next) => ctx.runtime.three.buildEntityMesh(THREE, next, presetLib),
          (mesh, next) => ctx.runtime.three.applyEntityTransform(THREE, mesh, next),
        )
      }
      entities.prune(activeIds)

      // Follow first available player for now.
      const target = (obs.players || [])[0]
      if (target && Array.isArray(target.position)) {
        const trackState = ctx.runtime.three.classifyAnimationTracks(target)
        const mode = trackState.reload ? 'spectator' : 'follow'
        cameraModes.update({
          mode,
          targetPosition: new THREE.Vector3(...target.position),
          targetForward: tmpForward,
          dt: 1 / 60,
        })

        // Move local bound player a bit ahead when holding W.
        // This is intentionally simplistic as a scaffold example.
        if (inputClient && localControlPlayer !== target.id && inputClient.session()) {
          localControlPlayer = target.id
        }
      }

      webgl.render(scene, camera)
    },

    unmount() {
      if (inputBinding) inputBinding.dispose()
      entities.clear()
      webgl.dispose()
    },
  }
}
"#;
    std::fs::write(renderer_dir.join("src/index.js"), renderer_js)
        .expect("Failed to create renderer/src/index.js");

    // Create .gitignore
    let gitignore = ".clawblox/\n.clawblox.log\n";
    let gitignore_path = project_dir.join(".gitignore");
    if !gitignore_path.exists() {
        std::fs::write(&gitignore_path, gitignore).expect("Failed to create .gitignore");
    }

    println!("Created new game project: {}", project_name);
    println!();
    if project_dir != std::env::current_dir().unwrap_or_default() {
        println!("  cd {}", project_name);
    }
    println!("  clawblox run");
}

// =============================================================================
// Credentials
// =============================================================================

#[derive(Serialize, Deserialize)]
struct Credentials {
    api_key: String,
    server: String,
}

fn credentials_path() -> PathBuf {
    let home = home::home_dir().expect("Failed to get home directory");
    home.join(".clawblox/credentials.toml")
}

fn load_credentials() -> Option<Credentials> {
    let path = credentials_path();
    let content = std::fs::read_to_string(&path).ok()?;
    toml::from_str(&content).ok()
}

fn save_credentials(creds: &Credentials) {
    let path = credentials_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("Failed to create ~/.clawblox directory");
    }
    let content = toml::to_string_pretty(creds).expect("Failed to serialize credentials");
    std::fs::write(&path, content).expect("Failed to write credentials file");
}

// =============================================================================
// Login Command
// =============================================================================

fn login(name: Option<String>, api_key: Option<String>, server: String) {
    if let Some(key) = api_key {
        save_credentials(&Credentials {
            api_key: key,
            server: server.clone(),
        });
        println!("API key saved to {}", credentials_path().display());
        return;
    }

    let name = name.unwrap_or_else(|| {
        eprintln!("Error: Provide a name to register, or use --api-key to store an existing key");
        eprintln!("  clawblox login my-name");
        eprintln!("  clawblox login --api-key clawblox_xxx");
        std::process::exit(1);
    });

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let client = reqwest::Client::new();
        let url = format!("{}/api/v1/agents/register", server);

        let resp = client
            .post(&url)
            .json(&serde_json::json!({
                "name": name,
                "description": format!("{}'s game developer account", name),
            }))
            .send()
            .await
            .unwrap_or_else(|e| {
                eprintln!("Error connecting to {}: {}", server, e);
                std::process::exit(1);
            });

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();

            // Idempotent login behavior: if name already exists, reuse existing credentials
            // for this server when possible.
            if status == reqwest::StatusCode::CONFLICT {
                if let Some(stored) = load_credentials() {
                    if stored.server == server {
                        let me_url = format!("{}/api/v1/agents/me", server);
                        let me_resp = client
                            .get(&me_url)
                            .header("Authorization", format!("Bearer {}", stored.api_key))
                            .send()
                            .await;

                        if let Ok(me_resp) = me_resp {
                            if me_resp.status().is_success() {
                                let me_body: serde_json::Value =
                                    me_resp.json().await.unwrap_or_default();
                                let stored_name =
                                    me_body["name"].as_str().unwrap_or_default().to_string();
                                if stored_name == name {
                                    println!("Already registered as: {}", name);
                                    println!(
                                        "Using existing API key from {}",
                                        credentials_path().display()
                                    );
                                    return;
                                }
                            }
                        }
                    }
                }

                eprintln!("Registration failed ({}): {}", status, body);
                eprintln!(
                    "Name '{}' already exists. Re-run with --api-key or use a different name.",
                    name
                );
                std::process::exit(1);
            }

            eprintln!("Registration failed ({}): {}", status, body);
            std::process::exit(1);
        }

        let body: serde_json::Value = resp.json().await.unwrap_or_else(|e| {
            eprintln!("Error parsing response: {}", e);
            std::process::exit(1);
        });

        let key = body["agent"]["api_key"]
            .as_str()
            .expect("Missing api_key in response");
        let claim_url = body["agent"]["claim_url"]
            .as_str()
            .expect("Missing claim_url in response");

        save_credentials(&Credentials {
            api_key: key.to_string(),
            server: server.clone(),
        });

        println!("Registered as: {}", name);
        println!("API key saved to {}", credentials_path().display());
        println!();
        println!("Claim your account: {}", claim_url);
        if let Some(code) = body["agent"]["verification_code"].as_str() {
            println!("Verification code: {}", code);
        }
    });
}

// =============================================================================
// Deploy Command
// =============================================================================

fn deploy_game(path: PathBuf, api_key_override: Option<String>, server_override: Option<String>) {
    let path = std::fs::canonicalize(&path).unwrap_or_else(|_| {
        eprintln!("Error: Path '{}' does not exist", path.display());
        std::process::exit(1);
    });

    // Resolve credentials: flag > env > stored
    let stored = load_credentials();
    let api_key = api_key_override
        .or_else(|| stored.as_ref().map(|c| c.api_key.clone()))
        .unwrap_or_else(|| {
            eprintln!("Error: No API key found. Run `clawblox login` first, or pass --api-key");
            std::process::exit(1);
        });
    let server = server_override
        .or_else(|| stored.as_ref().map(|c| c.server.clone()))
        .unwrap_or_else(|| "https://clawblox.com".to_string());

    // Load world config + bundled runtime script (main.lua + optional scripts/ tree)
    let (config, script_code) = load_world_and_entry_script(&path).unwrap_or_else(|e| {
        eprintln!("{}", e);
        std::process::exit(1);
    });

    // Read SKILL.md if configured
    let skill_md = config.scripts.skill.as_ref().and_then(|skill_file| {
        let p = path.join(skill_file);
        match std::fs::read_to_string(&p) {
            Ok(content) => Some(content),
            Err(e) => {
                eprintln!("Warning: Could not read {}: {}", p.display(), e);
                None
            }
        }
    });

    // Check for existing game_id
    let game_id_path = path.join(".clawblox/game_id");
    let existing_game_id = std::fs::read_to_string(&game_id_path)
        .ok()
        .and_then(|s| s.trim().parse::<Uuid>().ok());

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let client = reqwest::Client::new();

        let game_id = if let Some(id) = existing_game_id {
            // Update existing game
            println!("Updating game {}...", id);
            let url = format!("{}/api/v1/games/{}", server, id);
            let resp = client
                .put(&url)
                .header("Authorization", format!("Bearer {}", api_key))
                .json(&serde_json::json!({
                    "name": config.name,
                    "description": config.description,
                    "game_type": "lua",
                    "script_code": script_code,
                    "skill_md": skill_md,
                    "max_players": config.max_players as i32,
                    "observation_radius": config.observation.radius,
                }))
                .send()
                .await
                .unwrap_or_else(|e| {
                    eprintln!("Error connecting to {}: {}", server, e);
                    std::process::exit(1);
                });

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                eprintln!("Update failed ({}): {}", status, body);
                std::process::exit(1);
            }

            id
        } else {
            // Create new game
            println!("Creating new game...");
            let url = format!("{}/api/v1/games", server);
            let resp = client
                .post(&url)
                .header("Authorization", format!("Bearer {}", api_key))
                .json(&serde_json::json!({
                    "name": config.name,
                    "description": config.description.unwrap_or_default(),
                    "game_type": "lua",
                    "script_code": script_code,
                    "skill_md": skill_md,
                    "max_players": config.max_players as i32,
                    "observation_radius": config.observation.radius,
                }))
                .send()
                .await
                .unwrap_or_else(|e| {
                    eprintln!("Error connecting to {}: {}", server, e);
                    std::process::exit(1);
                });

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                eprintln!("Deploy failed ({}): {}", status, body);
                std::process::exit(1);
            }

            let body: serde_json::Value = resp.json().await.unwrap_or_else(|e| {
                eprintln!("Error parsing response: {}", e);
                std::process::exit(1);
            });

            let id_str = body["game_id"]
                .as_str()
                .expect("Missing game_id in response");
            id_str.parse::<Uuid>().expect("Invalid game_id in response")
        };

        // Save game_id for future deploys
        let clawblox_dir = path.join(".clawblox");
        std::fs::create_dir_all(&clawblox_dir).expect("Failed to create .clawblox directory");
        std::fs::write(game_id_path, game_id.to_string()).expect("Failed to save game_id");

        // Upload assets if assets/ directory exists and has files
        let assets_dir = path.join("assets");
        if assets_dir.is_dir() {
            let entries: Vec<_> = std::fs::read_dir(&assets_dir)
                .map(|rd| rd.filter_map(|e| e.ok()).collect())
                .unwrap_or_default();

            if !entries.is_empty() {
                println!("Uploading assets...");

                let enc = GzEncoder::new(Vec::new(), Compression::default());
                let mut tar_builder = tar::Builder::new(enc);
                tar_builder
                    .append_dir_all(".", &assets_dir)
                    .unwrap_or_else(|e| {
                        eprintln!("Error creating asset archive: {}", e);
                        std::process::exit(1);
                    });
                let enc = tar_builder.into_inner().unwrap_or_else(|e| {
                    eprintln!("Error finalizing tar: {}", e);
                    std::process::exit(1);
                });
                let targz_bytes = enc.finish().unwrap_or_else(|e| {
                    eprintln!("Error finalizing gzip: {}", e);
                    std::process::exit(1);
                });

                let url = format!("{}/api/v1/games/{}/assets", server, game_id);
                let resp = client
                    .post(&url)
                    .header("Authorization", format!("Bearer {}", api_key))
                    .header("Content-Type", "application/gzip")
                    .body(targz_bytes)
                    .send()
                    .await
                    .unwrap_or_else(|e| {
                        eprintln!("Error uploading assets: {}", e);
                        std::process::exit(1);
                    });

                if resp.status().is_success() {
                    let body: serde_json::Value = resp.json().await.unwrap_or_default();
                    let count = body["uploaded"].as_u64().unwrap_or(0);
                    let version = body["version"].as_i64().unwrap_or(0);
                    println!("Uploaded {} asset(s) (v{})", count, version);
                } else {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    eprintln!("Warning: Asset upload failed ({}): {}", status, body);
                }
            }
        }

        println!("Game deployed: {}/game/{}", server, game_id);
    });
}

// =============================================================================
// PID File Management
// =============================================================================

fn pid_dir() -> PathBuf {
    let home = home::home_dir().expect("Failed to get home directory");
    home.join(".local/share/clawblox/run")
}

fn pid_path(port: u16) -> PathBuf {
    pid_dir().join(format!("port-{}.pid", port))
}

fn read_pid_file(path: &Path) -> Option<u32> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

fn delete_pid_file(path: &Path) {
    let _ = std::fs::remove_file(path);
}

#[cfg(unix)]
fn is_process_alive(pid: u32) -> bool {
    // signal 0 checks if process exists without sending a signal
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

#[cfg(windows)]
fn is_process_alive(pid: u32) -> bool {
    std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {}", pid), "/NH"])
        .output()
        .map(|o| {
            let out = String::from_utf8_lossy(&o.stdout);
            out.contains(&pid.to_string())
        })
        .unwrap_or(false)
}

#[cfg(unix)]
fn kill_process(pid: u32) {
    unsafe {
        libc::kill(pid as libc::pid_t, libc::SIGTERM);
    }
}

#[cfg(windows)]
fn kill_process(pid: u32) {
    let _ = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/F"])
        .output();
}

/// If a previous clawblox process is running on this port, kill it and wait for it to exit.
fn kill_previous_instance(port: u16) {
    let path = pid_path(port);
    if let Some(old_pid) = read_pid_file(&path) {
        if is_process_alive(old_pid) {
            eprintln!(
                "Stopping previous clawblox instance (PID {}) on port {}...",
                old_pid, port
            );
            kill_process(old_pid);
            // Wait up to 2 seconds for the process to exit
            for _ in 0..20 {
                if !is_process_alive(old_pid) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            if is_process_alive(old_pid) {
                eprintln!(
                    "Warning: Previous process (PID {}) did not exit in time",
                    old_pid
                );
            }
        }
        // Remove stale PID file
        delete_pid_file(&path);
    }
}

/// Wait for SIGINT (Ctrl+C) or SIGTERM (Unix only).
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}

// =============================================================================
// Run Command
// =============================================================================

fn write_pid_file_for(path: &Path, pid: u32) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, pid.to_string());
}

#[derive(Debug, Clone, Serialize)]
struct LocalRendererManifest {
    schema_version: u32,
    api_version: u32,
    name: String,
    mode: String,
    entry_url: Option<String>,
    capabilities: Vec<String>,
}

fn default_renderer_manifest() -> LocalRendererManifest {
    LocalRendererManifest {
        schema_version: 1,
        api_version: 1,
        name: String::new(),
        mode: "none".to_string(),
        entry_url: None,
        capabilities: Vec::new(),
    }
}

#[derive(Debug, Deserialize)]
struct RendererPackageJson {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    scripts: HashMap<String, String>,
    #[serde(rename = "clawbloxRenderer", default)]
    clawblox_renderer: Option<RendererPackageMetadata>,
}

#[derive(Debug, Deserialize)]
struct RendererPackageMetadata {
    #[serde(rename = "apiVersion", default)]
    api_version: Option<u32>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    entry: Option<String>,
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(rename = "outFile", default)]
    out_file: Option<String>,
}

fn sanitize_renderer_entry(raw: &str) -> Option<String> {
    let normalized = raw.replace('\\', "/");
    if normalized.is_empty() || normalized.starts_with('/') {
        return None;
    }
    if normalized
        .split('/')
        .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return None;
    }
    Some(normalized)
}

fn normalize_renderer_rel_path(raw: &str) -> Option<String> {
    let mut normalized = raw.replace('\\', "/");
    while normalized.starts_with("./") {
        normalized = normalized[2..].to_string();
    }
    sanitize_renderer_entry(&normalized)
}

fn read_renderer_package(game_dir: &Path) -> Result<Option<RendererPackageJson>, String> {
    let package_json_path = game_dir.join("renderer").join("package.json");
    if !package_json_path.is_file() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&package_json_path)
        .map_err(|e| format!("Failed to read {}: {}", package_json_path.display(), e))?;
    let parsed: RendererPackageJson = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse {}: {}", package_json_path.display(), e))?;
    Ok(Some(parsed))
}

fn detect_renderer_source_entry(game_dir: &Path) -> Option<String> {
    let candidates = [
        "src/index.ts",
        "src/index.tsx",
        "src/index.js",
        "src/index.jsx",
        "index.ts",
        "index.tsx",
        "index.js",
        "index.jsx",
    ];
    for c in candidates {
        let p = game_dir.join("renderer").join(c);
        if p.is_file() {
            return Some(c.to_string());
        }
    }
    None
}

fn command_exists(bin: &str) -> bool {
    Command::new(bin).arg("--version").output().is_ok()
}

fn bundle_renderer_with_esbuild(
    game_dir: &Path,
    source_rel: &str,
    out_rel: &str,
) -> Result<(), String> {
    let renderer_dir = game_dir.join("renderer");
    let source = renderer_dir.join(source_rel);
    let out = renderer_dir.join(out_rel);

    let out_parent = out
        .parent()
        .ok_or_else(|| format!("Invalid renderer bundle output path: {}", out.display()))?;
    std::fs::create_dir_all(out_parent).map_err(|e| {
        format!(
            "Failed to create renderer build dir {}: {}",
            out_parent.display(),
            e
        )
    })?;

    let mut cmd = if command_exists("esbuild") {
        let mut c = Command::new("esbuild");
        c.arg(source.as_os_str());
        c
    } else if command_exists("npx") {
        let mut c = Command::new("npx");
        c.args(["--yes", "esbuild"]);
        c.arg(source.as_os_str());
        c
    } else {
        return Err(
            "Could not find esbuild or npx in PATH. Install esbuild (npm i -D esbuild) or Node.js."
                .to_string(),
        );
    };

    let output = cmd
        .current_dir(game_dir)
        .args([
            "--bundle",
            "--format=esm",
            "--platform=browser",
            "--target=es2020",
            "--sourcemap",
            "--log-level=warning",
        ])
        .arg(format!("--outfile={}", out.display()))
        .output()
        .map_err(|e| format!("Failed to spawn esbuild: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(format!(
            "esbuild failed bundling {} -> {}.\nstdout:\n{}\nstderr:\n{}",
            source.display(),
            out.display(),
            stdout,
            stderr
        ));
    }

    Ok(())
}

fn bundle_renderer_package_with_esbuild(
    game_dir: &Path,
    source_rel: &str,
    out_rel: &str,
) -> Result<(), String> {
    let renderer_dir = game_dir.join("renderer");
    let source = renderer_dir.join(source_rel);
    if !source.is_file() {
        return Err(format!(
            "Renderer package entry '{}' not found at {}",
            source_rel,
            source.display()
        ));
    }
    bundle_renderer_with_esbuild(game_dir, source_rel, out_rel)
}

fn maybe_build_renderer_package(
    game_dir: &Path,
    config: &clawblox::config::RendererConfig,
) -> Result<LocalRendererManifest, String> {
    let Some(package) = read_renderer_package(game_dir)? else {
        return Err(
            "Renderer package required: missing renderer/package.json. Run `clawblox init` for package scaffold or add a renderer package.".to_string(),
        );
    };

    let metadata = package.clawblox_renderer.unwrap_or(RendererPackageMetadata {
        api_version: None,
        name: None,
        entry: None,
        capabilities: vec![],
        out_file: None,
    });

    let entry_rel = metadata
        .entry
        .as_deref()
        .and_then(normalize_renderer_rel_path)
        .or_else(|| detect_renderer_source_entry(game_dir))
        .ok_or_else(|| {
            "Renderer package has no detectable entry. Set renderer/package.json -> clawbloxRenderer.entry (e.g. ./src/index.ts).".to_string()
        })?;

    let out_rel = metadata
        .out_file
        .as_deref()
        .and_then(normalize_renderer_rel_path)
        .unwrap_or_else(|| "dist/renderer.bundle.js".to_string());

    let renderer_dir = game_dir.join("renderer");
    if package.scripts.contains_key("build") {
        if !command_exists("npm") {
            return Err(
                "Renderer package declares a build script, but npm is not available in PATH."
                    .to_string(),
            );
        }
        let output = Command::new("npm")
            .args(["run", "build"])
            .current_dir(&renderer_dir)
            .output()
            .map_err(|e| format!("Failed to run renderer build script: {}", e))?;
        if !output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "Renderer package build failed (npm run build)\nstdout:\n{}\nstderr:\n{}",
                stdout, stderr
            ));
        }
    } else {
        bundle_renderer_package_with_esbuild(game_dir, &entry_rel, &out_rel)?;
    }

    let output_path = renderer_dir.join(&out_rel);
    if !output_path.is_file() {
        return Err(format!(
            "Renderer build did not produce expected output: {}",
            output_path.display()
        ));
    }

    let built_source = std::fs::read_to_string(&output_path)
        .map_err(|e| format!("Failed reading renderer output {}: {}", output_path.display(), e))?;
    if !built_source.contains("createRenderer") {
        return Err(format!(
            "Renderer output {} does not appear to export createRenderer.",
            output_path.display()
        ));
    }

    println!(
        "Renderer package bundle: renderer/{} -> renderer/{}",
        entry_rel, out_rel
    );

    let name = metadata
        .name
        .or(package.name)
        .unwrap_or_else(|| config.name.clone());
    let api_version = metadata.api_version.unwrap_or(config.api_version);
    let capabilities = if metadata.capabilities.is_empty() {
        config.capabilities.clone()
    } else {
        metadata.capabilities
    };

    Ok(LocalRendererManifest {
        schema_version: 1,
        api_version,
        name,
        mode: "module".to_string(),
        entry_url: Some(format!("/renderer-files/{}", out_rel)),
        capabilities,
    })
}

fn run_game(path: PathBuf, port: u16, daemon: bool, datastore: LocalDatastore, record: bool) {
    let path = std::fs::canonicalize(&path).unwrap_or_else(|_| {
        eprintln!("Error: Path '{}' does not exist", path.display());
        std::process::exit(1);
    });

    let world_cfg = clawblox::config::WorldConfig::from_game_dir(&path).unwrap_or_else(|e| {
        eprintln!("{}", e);
        std::process::exit(1);
    });

    let skip_run_command = std::env::var("CLAWBLOX_SKIP_RUN_COMMAND")
        .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false);

    if !skip_run_command {
        if let Some(command) = world_cfg.run.command.as_ref() {
            if daemon {
                eprintln!(
                    "Error: world.toml [run].command is not compatible with daemon mode (`clawblox start`). Use `clawblox run` instead."
                );
                std::process::exit(1);
            }
            run_external_world_command(&path, command, port);
            return;
        }
    }

    if skip_run_command && world_cfg.run.command.is_some() {
        eprintln!("Skipping world.toml [run].command due to CLAWBLOX_SKIP_RUN_COMMAND");
    }

    if world_cfg.runtime.mode == "external" {
        run_external_runtime(path, port, daemon, &world_cfg);
        return;
    }

    if daemon {
        let pid_file = pid_path(port);
        write_pid_file_for(&pid_file, std::process::id());
    }

    // Load world config + bundled runtime script (main.lua + optional scripts/ tree)
    let (config, script) = load_world_and_entry_script(&path).unwrap_or_else(|e| {
        eprintln!("{}", e);
        std::process::exit(1);
    });

    // Load skill.md if present
    let skill_md = config
        .scripts
        .skill
        .as_ref()
        .and_then(|skill_file| std::fs::read_to_string(path.join(skill_file)).ok());
    let renderer_manifest = maybe_build_renderer_package(&path, &config.renderer).unwrap_or_else(|e| {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    });

    let pid_file = pid_path(port);

    println!(
        "Starting {} (max {} players)",
        config.name, config.max_players
    );
    println!("Script: {}", config.scripts.main);
    if let Some(tree) = &config.scripts.tree {
        println!("Script tree: {}", tree);
    } else if path.join("scripts").is_dir() {
        println!("Script tree: scripts (auto-detected)");
    }

    // Create game manager with selected datastore backend.
    // Local run keeps GC disabled regardless of datastore mode.
    let (game_manager, game_handle) = match datastore {
        LocalDatastore::NoDb => {
            println!("Datastore: none (DataStoreService disabled)");
            GameManager::new_without_db(60, ErrorMode::Halt)
        }
        LocalDatastore::Postgres => {
            println!("Datastore: postgres (via DATABASE_URL)");
            let pool = tokio::runtime::Runtime::new()
                .expect("Failed to create tokio runtime")
                .block_on(async { db::create_pool().await })
                .unwrap_or_else(|e| {
                    eprintln!("Error: failed to connect to postgres: {}", e);
                    eprintln!("Hint: set DATABASE_URL or use --datastore sqlite");
                    std::process::exit(1);
                });
            let bridge = Arc::new(AsyncBridge::from_postgres(Arc::new(pool)));
            GameManager::new_with_bridge(60, Some(bridge), ErrorMode::Halt, true)
        }
        LocalDatastore::Sqlite => {
            let db_path = path.join(".clawblox").join("local.db");
            if let Some(parent) = db_path.parent() {
                std::fs::create_dir_all(parent).unwrap_or_else(|e| {
                    eprintln!("Error: failed to create local datastore directory: {}", e);
                    std::process::exit(1);
                });
            }
            println!("Datastore: sqlite ({})", db_path.display());
            let pool = tokio::runtime::Runtime::new()
                .expect("Failed to create tokio runtime")
                .block_on(async {
                    let pool = db::create_sqlite_pool(&db_path).await?;
                    db::ensure_local_datastore_schema(&pool).await?;
                    Ok::<_, sqlx::Error>(pool)
                })
                .unwrap_or_else(|e| {
                    eprintln!("Error: failed to initialize sqlite datastore: {}", e);
                    std::process::exit(1);
                });
            let bridge = Arc::new(AsyncBridge::from_sqlite(Arc::new(pool)));
            GameManager::new_with_bridge(60, Some(bridge), ErrorMode::Halt, true)
        }
    };

    // Use a random game_id for this session
    let game_id = Uuid::new_v4();

    // Create the instance
    let result = find_or_create_instance(
        &game_handle,
        game_id,
        config.max_players,
        config.observation.radius,
        Some(&script),
    );
    let instance_id = result.instance_id;
    println!("Instance: {}", instance_id);

    // Run game loop in background thread
    thread::spawn(move || {
        game_manager.run();
    });

    // Start HTTP server
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let recording_writer = if record {
            let rec_dir = path.join("recordings");
            let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();
            let rec_path = rec_dir.join(format!("{}.clawrec", timestamp));
            match recording::RecordingWriter::create(
                &rec_path,
                &config.name,
                &path.to_string_lossy(),
                60,
            )
            .await
            {
                Ok(writer) => {
                    println!("Recording to: {}", rec_path.display());
                    Some(Arc::new(tokio::sync::Mutex::new(writer)))
                }
                Err(e) => {
                    eprintln!("Warning: failed to start recording: {}", e);
                    None
                }
            }
        } else {
            None
        };

        let (audio_tx, _) = broadcast::channel::<AudioChunk>(64);
        let (speech_tx, _) = broadcast::channel::<SpeechEvent>(64);
        let (speech_progress_tx, _) = broadcast::channel::<SpeechProgressEvent>(128);
        let (playback_done_tx, _) = broadcast::channel::<PlaybackDoneEvent>(64);
        let (chat_tx, _) = broadcast::channel::<LocalChatMessage>(128);
        let state = LocalState {
            game_id,
            instance_id,
            game_handle,
            started_at: Instant::now(),
            skill_md,
            renderer_manifest,
            sessions: Arc::new(DashMap::new()),
            log_file: path.join(".clawblox.log"),
            audio_tx,
            speech_tx,
            speech_progress_tx,
            playback_done_tx,
            chat_tx,
            playback_done_seq: Arc::new(Mutex::new(0)),
            speech_seq: Arc::new(Mutex::new(0)),
            speech_progress_seq: Arc::new(Mutex::new(0)),
            authoritative_speech_event_cursor: Arc::new(Mutex::new(0)),
            audio_streams: Arc::new(Mutex::new(HashMap::new())),
            audio_speech_text: Arc::new(Mutex::new(HashMap::new())),
            chat_state: Arc::new(Mutex::new(LocalChatState::default())),
            recording_writer: recording_writer.clone(),
        };

        // Spawn recording task if recording is enabled
        let mut recording_shutdown_tx = None;
        let mut recording_task: Option<JoinHandle<()>> = None;
        if let Some(ref writer) = recording_writer {
            let writer = writer.clone();
            let game_handle = state.game_handle.clone();
            let instance_id = state.instance_id;
            let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
            recording_shutdown_tx = Some(shutdown_tx);
            recording_task = Some(tokio::spawn(async move {
                let mut tick_interval = interval(Duration::from_millis(16));
                let mut last_tick: u64 = 0;
                let start = Instant::now();
                loop {
                    tokio::select! {
                        _ = shutdown_rx.changed() => {
                            if *shutdown_rx.borrow() {
                                break;
                            }
                        }
                        _ = tick_interval.tick() => {
                            let obs = game::get_spectator_observation_for_instance(&game_handle, instance_id);
                            match obs {
                                Ok(mut obs) => {
                                    if obs.tick != last_tick {
                                        resolve_local_spectator_assets(&mut obs);
                                        let time_ms = start.elapsed().as_millis() as u64;
                                        let mut w = writer.lock().await;
                                        w.record_tick(obs.tick, time_ms, &obs).await;
                                        last_tick = obs.tick;
                                    }
                                }
                                Err(_) => break,
                            }
                        }
                    }
                }
            }));
        }

        let cors = CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any);

        let app = Router::new()
            .route("/", get(local_ui_index))
            .route("/ui", get(local_ui_index))
            .route("/ui/{*path}", get(local_ui_file))
            .route("/spectate", get(local_spectate))
            .route("/spectate/ws", get(local_spectate_ws))
            .route("/renderer/manifest", get(local_renderer_manifest))
            .route("/join", post(local_join))
            .route("/input", post(local_input))
            .route("/observe", get(local_observe))
            .route("/skill.md", get(local_skill))
            .route("/audio", post(local_audio))
            .route("/chat", post(local_chat))
            .route("/chat/messages", get(local_chat_messages))
            .nest_service("/renderer-files", ServeDir::new(path.join("renderer")))
            .nest_service("/assets", ServeDir::new(path.join("assets")))
            .nest_service("/static", ServeDir::new(path.join("static")))
            .with_state(state)
            .layer(cors);

        let addr = SocketAddr::from(([0, 0, 0, 0], port));
        println!();
        println!("Server running on http://localhost:{}", port);
        println!("Local frontend: http://localhost:{}/", port);
        if !daemon {
            println!("Mode: foreground (press Ctrl+C to stop)");
        }
        println!();
        println!("Endpoints:");
        println!("  GET  /              - Embedded local frontend");
        println!("  POST /join?name=X  - Join game, returns session token");
        println!("  POST /input        - Send input (requires X-Session header)");
        println!("  GET  /observe      - Player observation (requires X-Session header)");
        println!("  POST /chat         - Send chat message (requires X-Session header)");
        println!("  GET  /chat/messages - List local chat messages");
        println!("  GET  /skill.md     - Game skill definition");

        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .unwrap_or_else(|e| {
                delete_pid_file(&pid_file);
                eprintln!("Error: Port {} is already in use. Try --port <PORT>", port);
                eprintln!("Details: {}", e);
                std::process::exit(1);
            });

        let pid_file_clone = pid_file.clone();
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                shutdown_signal().await;
                eprintln!("\nShutting down...");
                delete_pid_file(&pid_file_clone);
            })
            .await
            .unwrap_or_else(|e| {
                eprintln!("Server error: {}", e);
            });

        // Finalize recording on shutdown
        if let Some(shutdown_tx) = recording_shutdown_tx {
            let _ = shutdown_tx.send(true);
        }
        if let Some(task) = recording_task {
            if let Err(err) = task.await {
                eprintln!("Warning: recording task join failed: {}", err);
            }
        }
        if let Some(writer) = recording_writer {
            let mut w = writer.lock().await;
            w.close(0, 0).await;
            eprintln!("Recording saved.");
        }

        // Also clean up here in case shutdown_signal didn't fire
        delete_pid_file(&pid_file);
    });
}

fn run_replay(file: PathBuf, speed: f32, port: u16, world_dir_override: Option<PathBuf>) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let reader = recording::RecordingReader::open(&file).await.unwrap_or_else(|e| {
            eprintln!("Error: failed to open recording: {}", e);
            std::process::exit(1);
        });
        let meta = reader.meta().await.unwrap_or_else(|e| {
            eprintln!("Error: failed to read recording metadata: {}", e);
            std::process::exit(1);
        });

        let world_dir = world_dir_override
            .unwrap_or_else(|| PathBuf::from(&meta.world_dir));
        if !world_dir.is_dir() {
            eprintln!(
                "Warning: world directory '{}' not found. Asset serving may fail. Use --world-dir to override.",
                world_dir.display()
            );
        }

        println!("Replaying: {}", file.display());
        println!("World: {} ({})", meta.world_name, world_dir.display());
        println!("Duration: {:.1}s, {} ticks", meta.duration_ms as f64 / 1000.0, meta.total_ticks);

        let first_tick = reader.first_tick().unwrap_or_else(|| {
            eprintln!("Error: recording has no checkpoints to replay");
            std::process::exit(1);
        });
        let playback = replay::ReplayPlayback::new(
            first_tick,
            meta.total_ticks,
            meta.tick_rate,
            meta.duration_ms,
            speed,
        )
        .unwrap_or_else(|e| {
            eprintln!("Error: invalid replay playback state: {}", e);
            std::process::exit(1);
        });

        let reader = Arc::new(reader);
        let playback = Arc::new(tokio::sync::Mutex::new(playback));

        // Load renderer manifest from world_dir
        let renderer_manifest = {
            let world_toml_path = world_dir.join("world.toml");
            if world_toml_path.exists() {
                let cfg = clawblox::config::WorldConfig::from_game_dir(&world_dir).ok();
                cfg.map(|c| maybe_build_renderer_package(&world_dir, &c.renderer).ok())
                    .flatten()
                    .unwrap_or_else(default_renderer_manifest)
            } else {
                default_renderer_manifest()
            }
        };

        let replay_state = ReplayState {
            reader: reader.clone(),
            playback: playback.clone(),
            renderer_manifest,
        };

        let cors = CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any);

        let app = Router::new()
            .route("/", get(local_ui_index))
            .route("/ui", get(local_ui_index))
            .route("/ui/{*path}", get(local_ui_file))
            .route("/spectate/ws", get(replay_spectate_ws))
            .route("/renderer/manifest", get(replay_renderer_manifest))
            .route("/replay/info", get(replay_info))
            .route("/replay/seek", post(replay_seek))
            .route("/replay/speed", post(replay_speed))
            .route("/replay/pause", post(replay_pause))
            .route("/replay/play", post(replay_play))
            .nest_service("/renderer-files", ServeDir::new(world_dir.join("renderer")))
            .nest_service("/assets", ServeDir::new(world_dir.join("assets")))
            .nest_service("/static", ServeDir::new(world_dir.join("static")))
            .with_state(replay_state)
            .layer(cors);

        let addr = SocketAddr::from(([0, 0, 0, 0], port));
        println!("Replay server: http://localhost:{}", port);

        let listener = tokio::net::TcpListener::bind(addr).await.unwrap_or_else(|e| {
            eprintln!("Error: Port {} is already in use. Try --port <PORT>", port);
            eprintln!("Details: {}", e);
            std::process::exit(1);
        });

        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                shutdown_signal().await;
                eprintln!("\nReplay server stopped.");
            })
            .await
            .unwrap_or_else(|e| {
                eprintln!("Server error: {}", e);
            });
    });
}

#[derive(Clone)]
struct ReplayState {
    reader: Arc<recording::RecordingReader>,
    playback: Arc<tokio::sync::Mutex<replay::ReplayPlayback>>,
    renderer_manifest: LocalRendererManifest,
}

async fn replay_spectate_ws(
    State(state): State<ReplayState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_replay_ws(socket, state))
}

async fn handle_replay_ws(socket: WebSocket, state: ReplayState) {
    let (mut sender, mut receiver) = socket.split();
    let mut tick_interval = interval(Duration::from_millis(33));

    loop {
        tokio::select! {
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(data))) => {
                        if sender.send(Message::Pong(data)).await.is_err() { break; }
                    }
                    _ => {}
                }
            }
            _ = tick_interval.tick() => {
                let tick = {
                    let mut pb = state.playback.lock().await;
                    let tick = pb.current_tick();
                    pb.advance_frame(Duration::from_millis(33));
                    tick
                };

                match state.reader.get_state_at(tick).await {
                    Ok(obs) => {
                        if let Ok(json) = serde_json::to_vec(&obs) {
                            let msg = Message::Text(String::from_utf8_lossy(&json).into_owned().into());
                            if sender.send(msg).await.is_err() { break; }
                        }
                    }
                    Err(err) => {
                        let message = format!("Failed to load replay state at tick {}: {}", tick, err);
                        eprintln!("{}", message);
                        let payload = serde_json::json!({
                            "type": "replay_error",
                            "tick": tick,
                            "message": message,
                        });
                        if sender.send(Message::Text(payload.to_string().into())).await.is_err() {
                            break;
                        }
                    }
                }
            }
        }
    }
}

async fn replay_renderer_manifest(State(state): State<ReplayState>) -> Json<LocalRendererManifest> {
    Json(state.renderer_manifest)
}

async fn replay_info(State(state): State<ReplayState>) -> Json<replay::ReplayInfo> {
    let pb = state.playback.lock().await;
    Json(pb.info())
}

#[derive(Deserialize)]
struct SeekQuery {
    tick: u64,
}

async fn replay_seek(
    State(state): State<ReplayState>,
    Query(q): Query<SeekQuery>,
) -> axum::http::StatusCode {
    let mut pb = state.playback.lock().await;
    pb.seek(q.tick);
    axum::http::StatusCode::OK
}

#[derive(Deserialize)]
struct SpeedQuery {
    value: f32,
}

async fn replay_speed(
    State(state): State<ReplayState>,
    Query(q): Query<SpeedQuery>,
) -> axum::http::StatusCode {
    let mut pb = state.playback.lock().await;
    pb.set_speed(q.value);
    axum::http::StatusCode::OK
}

async fn replay_pause(State(state): State<ReplayState>) -> axum::http::StatusCode {
    let mut pb = state.playback.lock().await;
    pb.set_paused(true);
    axum::http::StatusCode::OK
}

async fn replay_play(State(state): State<ReplayState>) -> axum::http::StatusCode {
    let mut pb = state.playback.lock().await;
    pb.set_paused(false);
    axum::http::StatusCode::OK
}

fn run_external_runtime(path: PathBuf, port: u16, daemon: bool, config: &clawblox::config::WorldConfig) {
    if daemon {
        let pid_file = pid_path(port);
        write_pid_file_for(&pid_file, std::process::id());
    }

    let skill_md = config
        .scripts
        .skill
        .as_ref()
        .and_then(|skill_file| std::fs::read_to_string(path.join(skill_file)).ok());
    let renderer_manifest = maybe_build_renderer_package(&path, &config.renderer).unwrap_or_else(|e| {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    });
    let pid_file = pid_path(port);

    println!("Starting {} (external runtime mode)", config.name);

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let state = ExternalLocalState {
            renderer_manifest,
            skill_md,
            started_at: Instant::now(),
        };

        let cors = CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any);

        let app = Router::new()
            .route("/", get(local_ui_index))
            .route("/ui", get(local_ui_index))
            .route("/ui/{*path}", get(local_ui_file))
            .route("/renderer/manifest", get(external_renderer_manifest))
            .route("/spectate", get(external_spectate))
            .route("/spectate/ws", get(external_spectate_ws))
            .route("/skill.md", get(external_skill))
            .nest_service("/renderer-files", ServeDir::new(path.join("renderer")))
            .nest_service("/assets", ServeDir::new(path.join("assets")))
            .nest_service("/static", ServeDir::new(path.join("static")))
            .with_state(state)
            .layer(cors);

        let addr = SocketAddr::from(([0, 0, 0, 0], port));
        println!();
        println!("Server running on http://localhost:{}", port);
        println!("Local frontend: http://localhost:{}/", port);
        if !daemon {
            println!("Mode: foreground (press Ctrl+C to stop)");
        }
        println!();
        println!("Runtime: external (no Lua world, no /join /input /observe)");

        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .unwrap_or_else(|e| {
                delete_pid_file(&pid_file);
                eprintln!("Error: Port {} is already in use. Try --port <PORT>", port);
                eprintln!("Details: {}", e);
                std::process::exit(1);
            });

        let pid_file_clone = pid_file.clone();
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                shutdown_signal().await;
                eprintln!("\nShutting down...");
                delete_pid_file(&pid_file_clone);
            })
            .await
            .unwrap_or_else(|e| {
                eprintln!("Server error: {}", e);
            });

        delete_pid_file(&pid_file);
    });
}

fn run_external_world_command(path: &Path, command: &[String], port: u16) {
    let executable = command.first().cloned().unwrap_or_default();
    if executable.trim().is_empty() {
        eprintln!("Error: world.toml [run].command[0] must be a non-empty executable");
        std::process::exit(1);
    }
    let args: Vec<String> = command.iter().skip(1).cloned().collect();

    println!("Delegating run to world.toml [run].command");
    println!("Command: {} {}", executable, args.join(" "));
    println!(
        "Environment: CLAWBLOX_PORT={} (current dir: {})",
        port,
        path.display()
    );

    let current_exe = std::env::current_exe()
        .ok()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "clawblox".to_string());

    let status = Command::new(&executable)
        .args(&args)
        .current_dir(path)
        .env("CLAWBLOX_PORT", port.to_string())
        .env("CLAWBLOX_BIN", current_exe)
        .status()
        .unwrap_or_else(|e| {
            eprintln!("Error: failed to start run.command executable '{}': {}", executable, e);
            std::process::exit(1);
        });

    if !status.success() {
        if let Some(code) = status.code() {
            std::process::exit(code);
        }
        std::process::exit(1);
    }
}

fn start_game(path: PathBuf, port: u16, datastore: LocalDatastore) {
    let path = std::fs::canonicalize(&path).unwrap_or_else(|_| {
        eprintln!("Error: Path '{}' does not exist", path.display());
        std::process::exit(1);
    });

    let world_cfg = clawblox::config::WorldConfig::from_game_dir(&path).unwrap_or_else(|e| {
        eprintln!("{}", e);
        std::process::exit(1);
    });
    if world_cfg.run.command.is_some() {
        eprintln!(
            "Error: world.toml [run].command is not supported by `clawblox start` (daemon mode). Use `clawblox run`."
        );
        std::process::exit(1);
    }

    kill_previous_instance(port);

    let log_file_path = path.join(".clawblox.log");
    let log_file = std::fs::File::create(&log_file_path).unwrap_or_else(|e| {
        eprintln!("Error creating log file {}: {}", log_file_path.display(), e);
        std::process::exit(1);
    });
    let log_file_stderr = log_file.try_clone().unwrap_or_else(|e| {
        eprintln!("Error cloning log file handle: {}", e);
        std::process::exit(1);
    });

    let exe = std::env::current_exe().expect("Failed to get current executable path");
    let child = std::process::Command::new(exe)
        .args([
            "run",
            &path.to_string_lossy(),
            "--port",
            &port.to_string(),
            "--datastore",
            datastore.as_arg(),
            "--daemon",
        ])
        .stdout(log_file)
        .stderr(log_file_stderr)
        .stdin(std::process::Stdio::null())
        .spawn()
        .unwrap_or_else(|e| {
            eprintln!("Error spawning daemon: {}", e);
            std::process::exit(1);
        });

    // Write the child PID to the PID file
    let pid_file = pid_path(port);
    write_pid_file_for(&pid_file, child.id());

    println!("Server starting in background on http://localhost:{}", port);
    println!("Logs: {}", log_file_path.display());
}

fn stop_game(port: u16) {
    let path = pid_path(port);

    let Some(pid) = read_pid_file(&path) else {
        println!("No running clawblox instance found on port {}", port);
        return;
    };

    if !is_process_alive(pid) {
        delete_pid_file(&path);
        println!(
            "No running clawblox instance found on port {} (cleaned stale PID file)",
            port
        );
        return;
    }

    println!(
        "Stopping clawblox instance (PID {}) on port {}...",
        pid, port
    );
    kill_process(pid);

    for _ in 0..20 {
        if !is_process_alive(pid) {
            delete_pid_file(&path);
            println!("Stopped.");
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    if is_process_alive(pid) {
        eprintln!("Warning: process {} did not exit in time", pid);
    } else {
        delete_pid_file(&path);
        println!("Stopped.");
    }
}

// =============================================================================
// Local Server State & Handlers
// =============================================================================

#[derive(Clone, Debug, Serialize)]
struct AudioChunk {
    speaker: String,
    stream_id: String,
    seq: u32,
    data: String, // base64 PCM
    done: bool,
}

#[derive(Clone, Debug, Serialize)]
struct SpeechEvent {
    speaker: String,
    text: String,
    seq: u64,
}

#[derive(Clone, Debug, Serialize)]
struct SpeechProgressEvent {
    speaker: String,
    stream_id: String,
    text: String,
    seq: u64,
}

#[derive(Clone, Debug, Serialize)]
struct PlaybackDoneEvent {
    speaker: String,
    stream_id: String,
    seq: u64,
}

#[derive(Deserialize)]
struct ChatPostRequest {
    content: String,
}

#[derive(Serialize)]
struct ChatPostResponse {
    id: String,
    created_at: String,
}

#[derive(Deserialize)]
struct ChatMessagesQuery {
    instance_id: Option<Uuid>,
    after: Option<String>,
    limit: Option<usize>,
}

#[derive(Serialize)]
struct ChatMessagesResponse {
    messages: Vec<LocalChatMessage>,
}

#[derive(Clone, Debug)]
struct AudioStreamState {
    agent_id: Uuid,
    speaker: String,
    first_seen: Instant,
    last_seen: Instant,
    total_audio_bytes: u64,
    done_received: bool,
    done_at: Option<Instant>,
    expected_end_at: Option<Instant>,
    completion_scheduled: bool,
    sample_rate_hz: u32,
    channels: u16,
    bytes_per_sample: u16,
    playback_speed: f32,
}

const LOCAL_CHAT_MAX_MESSAGES: usize = 1000;
const LOCAL_CHAT_DEFAULT_LIMIT: usize = 50;
const LOCAL_CHAT_MAX_LIMIT: usize = 100;

#[derive(Clone, Debug, Serialize)]
struct LocalChatMessage {
    id: String,
    instance_id: Uuid,
    agent_id: Uuid,
    agent_name: String,
    message_type: String,
    content: String,
    created_at: String,
}

#[derive(Debug, Default)]
struct LocalChatState {
    next_seq: u64,
    messages: VecDeque<LocalChatMessage>,
}

impl LocalChatState {
    fn push(&mut self, instance_id: Uuid, agent_id: Uuid, agent_name: String, content: String) -> LocalChatMessage {
        self.next_seq = self.next_seq.saturating_add(1);
        let msg = LocalChatMessage {
            id: format!("local_{}", self.next_seq),
            instance_id,
            agent_id,
            agent_name,
            message_type: "text".to_string(),
            content,
            created_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        };
        self.messages.push_back(msg.clone());
        while self.messages.len() > LOCAL_CHAT_MAX_MESSAGES {
            self.messages.pop_front();
        }
        msg
    }

    fn list(&self, instance_id: Uuid, after: Option<&str>, limit: usize) -> Vec<LocalChatMessage> {
        self.messages
            .iter()
            .filter(|m| m.instance_id == instance_id)
            .filter(|m| after.map(|a| m.created_at.as_str() > a).unwrap_or(true))
            .rev()
            .take(limit)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }
}

#[derive(Clone)]
struct LocalState {
    game_id: Uuid,
    instance_id: Uuid,
    game_handle: GameManagerHandle,
    started_at: Instant,
    skill_md: Option<String>,
    renderer_manifest: LocalRendererManifest,
    sessions: Arc<DashMap<String, (Uuid, String)>>, // token -> (agent_id, name)
    log_file: PathBuf,
    audio_tx: broadcast::Sender<AudioChunk>,
    speech_tx: broadcast::Sender<SpeechEvent>,
    speech_progress_tx: broadcast::Sender<SpeechProgressEvent>,
    playback_done_tx: broadcast::Sender<PlaybackDoneEvent>,
    chat_tx: broadcast::Sender<LocalChatMessage>,
    playback_done_seq: Arc<Mutex<u64>>,
    speech_seq: Arc<Mutex<u64>>,
    speech_progress_seq: Arc<Mutex<u64>>,
    authoritative_speech_event_cursor: Arc<Mutex<u64>>,
    /// Maps stream_id -> stream state for routing and completion semantics.
    audio_streams: Arc<Mutex<HashMap<String, AudioStreamState>>>,
    /// Maps stream_id -> speech text for broadcasting on AudioEnded
    audio_speech_text: Arc<Mutex<HashMap<String, String>>>,
    chat_state: Arc<Mutex<LocalChatState>>,
    /// Session recording writer (None if not recording). Kept alive via Arc.
    #[allow(dead_code)]
    recording_writer: Option<Arc<tokio::sync::Mutex<recording::RecordingWriter>>>,
}

#[derive(Clone)]
struct ExternalLocalState {
    renderer_manifest: LocalRendererManifest,
    skill_md: Option<String>,
    started_at: Instant,
}

fn base64_decoded_len(data: &str) -> Option<usize> {
    let len = data.len();
    if len == 0 {
        return Some(0);
    }
    if len % 4 != 0 {
        return None;
    }

    let pad = data.as_bytes().iter().rev().take_while(|b| **b == b'=').count();
    if pad > 2 {
        return None;
    }
    Some((len / 4) * 3 - pad)
}

fn compute_stream_duration_secs(stream: &AudioStreamState) -> f64 {
    let bytes_per_second = (stream.sample_rate_hz as f64)
        * (stream.channels as f64)
        * (stream.bytes_per_sample as f64);
    if bytes_per_second <= 0.0 {
        return 0.0;
    }
    let speed = (stream.playback_speed as f64).max(MIN_PLAYBACK_SPEED as f64);
    (stream.total_audio_bytes as f64) / bytes_per_second / speed
}

fn append_server_log(log_file: &Path, message: &str) {
    let ts_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log_file) {
        let _ = writeln!(file, "[{}] {}", ts_ms, message);
    }
}

fn complete_audio_stream(state: &LocalState, stream_id: &str, reason: &str) {
    let completed_at = Instant::now();
    let stream_state = {
        let mut streams = state.audio_streams.lock().unwrap();
        streams.remove(stream_id)
    };

    let Some(stream_state) = stream_state else {
        return;
    };

    let expected_lag_ms = stream_state
        .expected_end_at
        .map(|eta| {
            if completed_at >= eta {
                completed_at.duration_since(eta).as_millis() as i128
            } else {
                -(eta.duration_since(completed_at).as_millis() as i128)
            }
        })
        .unwrap_or(0);
    append_server_log(
        &state.log_file,
        &format!(
            "[audio] complete stream={} speaker={} reason={} lag_vs_estimate_ms={} total_bytes={}",
            stream_id, stream_state.speaker, reason, expected_lag_ms, stream_state.total_audio_bytes
        ),
    );

    push_playback_done_event(
        state,
        stream_state.speaker.clone(),
        stream_id.to_string(),
    );

    let speech_text = state.audio_speech_text.lock().unwrap().remove(stream_id);

    let speech_finished_data = serde_json::json!({
        "stream_id": stream_id,
        "reason": reason,
        "speaker": stream_state.speaker,
        "speech_text": speech_text,
    });

    if let Err(e) = game::queue_input(
        &state.game_handle,
        state.game_id,
        stream_state.agent_id,
        "SpeechFinished".to_string(),
        speech_finished_data,
    ) {
        eprintln!(
            "[audio] Failed to queue SpeechFinished for stream {}: {}",
            stream_id, e
        );
    }

    if let Err(e) = game::queue_input(
        &state.game_handle,
        state.game_id,
        stream_state.agent_id,
        "AudioEnded".to_string(),
        serde_json::json!({
            "stream_id": stream_id,
            "reason": reason
        }),
    ) {
        eprintln!(
            "[audio] Failed to queue AudioEnded for stream {}: {}",
            stream_id, e
        );
    }
}

fn schedule_audio_stream_completion(state: LocalState, stream_id: String, expected_end_at: Instant) {
    tokio::spawn(async move {
        tokio::time::sleep_until(tokio::time::Instant::from_std(expected_end_at)).await;
        complete_audio_stream(&state, &stream_id, SERVER_PREDICTED_END_REASON);
    });
}

fn push_speech_event(state: &LocalState, speaker: String, text: String) {
    let seq = {
        let mut seq = state.speech_seq.lock().unwrap();
        let next = *seq;
        *seq += 1;
        next
    };
    let event = SpeechEvent { speaker, text, seq };
    let _ = state.speech_tx.send(event);
}

fn maybe_publish_authoritative_speech_events(
    state: &LocalState,
    recent_events: &[clawblox::game::instance::SpectatorReplicatedEvent],
) {
    if recent_events.is_empty() {
        return;
    }

    let mut cursor = state.authoritative_speech_event_cursor.lock().unwrap();
    for event in recent_events {
        if event.id <= *cursor {
            continue;
        }
        if event.name == "SpeechBus" {
            let payload = &event.payload;
            let op = payload.get("op").and_then(|v| v.as_str()).unwrap_or("");
            if op == "SpeechText" {
                let speaker = payload
                    .get("speaker")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let text = payload
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if !speaker.is_empty() && !text.is_empty() {
                    push_speech_event(state, speaker, text);
                }
            }
        }
        *cursor = event.id;
    }
}

fn push_speech_progress_event(state: &LocalState, speaker: String, stream_id: String, text: String) {
    let seq = {
        let mut seq = state.speech_progress_seq.lock().unwrap();
        let next = *seq;
        *seq += 1;
        next
    };
    let event = SpeechProgressEvent {
        speaker,
        stream_id,
        text,
        seq,
    };
    let _ = state.speech_progress_tx.send(event);
}

fn push_playback_done_event(state: &LocalState, speaker: String, stream_id: String) {
    let seq = {
        let mut seq = state.playback_done_seq.lock().unwrap();
        let next = *seq;
        *seq += 1;
        next
    };
    let _ = state.playback_done_tx.send(PlaybackDoneEvent {
        speaker,
        stream_id,
        seq,
    });
}

fn prune_stale_audio_streams(state: &LocalState, now: Instant) {
    let mut stale_stream_ids = Vec::new();
    {
        let mut streams = state.audio_streams.lock().unwrap();
        streams.retain(|stream_id, stream| {
            let keep = now.duration_since(stream.last_seen) < Duration::from_secs(STREAM_TTL_SECS);
            if !keep {
                stale_stream_ids.push(stream_id.clone());
            }
            keep
        });
    }

    if !stale_stream_ids.is_empty() {
        let mut speech_texts = state.audio_speech_text.lock().unwrap();
        for stream_id in stale_stream_ids {
            speech_texts.remove(&stream_id);
        }
    }
}

/// Check if the game instance is halted due to a Lua error.
/// Returns the error message with log path appended, or None if running normally.
fn check_halted(state: &LocalState) -> Option<String> {
    let handle = state.game_handle.instances.get(&state.instance_id)?;
    let instance = handle.read();
    instance.halted_error.as_ref().map(|err| {
        format!(
            "Game halted: {}. See logs for full stack trace: {}",
            err,
            state.log_file.display()
        )
    })
}

fn content_type_for_path(path: &str) -> &'static str {
    if path.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if path.ends_with(".js") {
        "application/javascript; charset=utf-8"
    } else if path.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if path.ends_with(".json") {
        "application/json; charset=utf-8"
    } else if path.ends_with(".svg") {
        "image/svg+xml"
    } else if path.ends_with(".png") {
        "image/png"
    } else if path.ends_with(".jpg") || path.ends_with(".jpeg") {
        "image/jpeg"
    } else {
        "application/octet-stream"
    }
}

fn embedded_ui_file_response(path: &str) -> impl IntoResponse {
    if let Some(file) = LOCAL_RUNTIME_UI_DIR.get_file(path) {
        let content_type = content_type_for_path(path);
        return (
            axum::http::StatusCode::OK,
            [("content-type", content_type)],
            file.contents().to_vec(),
        )
            .into_response();
    }
    (
        axum::http::StatusCode::NOT_FOUND,
        [("content-type", "text/plain; charset=utf-8")],
        b"Not found".to_vec(),
    )
        .into_response()
}

async fn local_ui_index() -> impl IntoResponse {
    embedded_ui_file_response("index.html")
}

fn external_snapshot(state: &ExternalLocalState) -> serde_json::Value {
    let tick = state.started_at.elapsed().as_millis() as u64;
    serde_json::json!({
        "instance_id": "external",
        "tick": tick,
        "server_time_ms": tick,
        "game_status": "playing",
        "players": [],
        "entities": [],
        "recent_events": []
    })
}

async fn external_renderer_manifest(
    State(state): State<ExternalLocalState>,
) -> Json<LocalRendererManifest> {
    Json(state.renderer_manifest)
}

async fn external_spectate(State(state): State<ExternalLocalState>) -> Json<serde_json::Value> {
    Json(external_snapshot(&state))
}

async fn external_skill(State(state): State<ExternalLocalState>) -> impl IntoResponse {
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

async fn external_spectate_ws(
    State(state): State<ExternalLocalState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_external_spectate_ws(socket, state))
}

async fn handle_external_spectate_ws(socket: WebSocket, state: ExternalLocalState) {
    let (mut sender, mut receiver) = socket.split();
    let mut tick_interval = interval(Duration::from_millis(250));

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
                let payload = external_snapshot(&state);
                if sender.send(Message::Text(payload.to_string().into())).await.is_err() {
                    break;
                }
            }
        }
    }
}

async fn local_ui_file(AxumPath(path): AxumPath<String>) -> impl IntoResponse {
    let req_path = path.trim_start_matches('/');
    if req_path.is_empty() {
        return embedded_ui_file_response("index.html");
    }
    embedded_ui_file_response(req_path)
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
    if let Some(err) = check_halted(&state) {
        return Err((axum::http::StatusCode::SERVICE_UNAVAILABLE, err));
    }

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
    state
        .sessions
        .insert(session_token.clone(), (agent_id, query.name));

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

    state.sessions.get(token).map(|r| r.clone()).ok_or((
        axum::http::StatusCode::UNAUTHORIZED,
        "Invalid session".to_string(),
    ))
}

#[derive(Deserialize)]
struct AudioChunkRequest {
    stream_id: String,
    seq: u32,
    data: String,
    done: bool,
    #[serde(default)]
    speech_text: Option<String>,
    #[serde(default)]
    sample_rate_hz: Option<u32>,
    #[serde(default)]
    channels: Option<u16>,
    #[serde(default)]
    bytes_per_sample: Option<u16>,
    #[serde(default)]
    playback_speed: Option<f32>,
}

async fn local_audio(
    State(state): State<LocalState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<AudioChunkRequest>,
) -> Result<axum::http::StatusCode, (axum::http::StatusCode, String)> {
    let (agent_id, name) = get_session(&state, &headers)?;
    let stream_id = body.stream_id.clone();
    let chunk_bytes = base64_decoded_len(&body.data).ok_or((
        axum::http::StatusCode::BAD_REQUEST,
        "Invalid base64 PCM chunk".to_string(),
    ))? as u64;

    let sample_rate_hz = body
        .sample_rate_hz
        .unwrap_or(DEFAULT_SAMPLE_RATE_HZ)
        .max(MIN_POSITIVE_U32);
    let channels = body.channels.unwrap_or(DEFAULT_CHANNELS).max(MIN_POSITIVE_U16);
    let bytes_per_sample = body
        .bytes_per_sample
        .unwrap_or(DEFAULT_BYTES_PER_SAMPLE)
        .max(MIN_POSITIVE_U16);
    let playback_speed = body
        .playback_speed
        .unwrap_or(DEFAULT_PLAYBACK_SPEED)
        .max(MIN_PLAYBACK_SPEED);
    let mut schedule: Option<Instant> = None;

    // Track stream lifecycle metadata and prune stale entries.
    {
        let now = Instant::now();
        prune_stale_audio_streams(&state, now);

        let mut streams = state.audio_streams.lock().unwrap();
        let stream = streams
            .entry(stream_id.clone())
            .or_insert_with(|| AudioStreamState {
                agent_id,
                speaker: name.clone(),
                first_seen: now,
                last_seen: now,
                total_audio_bytes: 0,
                done_received: false,
                done_at: None,
                expected_end_at: None,
                completion_scheduled: false,
                sample_rate_hz,
                channels,
                bytes_per_sample,
                playback_speed,
            });
        stream.last_seen = now;
        stream.total_audio_bytes = stream.total_audio_bytes.saturating_add(chunk_bytes);
        stream.sample_rate_hz = sample_rate_hz;
        stream.channels = channels;
        stream.bytes_per_sample = bytes_per_sample;
        stream.playback_speed = playback_speed;

        if body.done && !stream.done_received {
            stream.done_received = true;
            stream.done_at = Some(now);
            let duration_secs = compute_stream_duration_secs(stream);
            let duration = Duration::from_secs_f64(duration_secs.max(0.0));
            let expected_end_at = stream.first_seen + duration;
            stream.expected_end_at = Some(expected_end_at);

            if !stream.completion_scheduled {
                stream.completion_scheduled = true;
                schedule = Some(expected_end_at);
            }
        }
    }

    if let Some(expected_end_at) = schedule {
        let now = Instant::now();
        let eta_ms = if expected_end_at > now {
            expected_end_at.duration_since(now).as_millis() as u64
        } else {
            0
        };
        let expected_unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
            .saturating_add(eta_ms);
        append_server_log(
            &state.log_file,
            &format!(
                "[audio] estimate stream={} speaker={} done_chunk_seq={} eta_in_ms={} eta_unix_ms={} sample_rate={} channels={} bytes_per_sample={} speed={}",
                stream_id,
                name,
                body.seq,
                eta_ms,
                expected_unix_ms,
                sample_rate_hz,
                channels,
                bytes_per_sample,
                playback_speed
            ),
        );
        schedule_audio_stream_completion(state.clone(), stream_id.clone(), expected_end_at);
    }

    // Store speech text if provided (typically on the done=true chunk)
    if let Some(text) = &body.speech_text {
        state
            .audio_speech_text
            .lock()
            .unwrap()
            .insert(stream_id.clone(), text.clone());
    }

    let _ = state.audio_tx.send(AudioChunk {
        speaker: name,
        stream_id: stream_id.clone(),
        seq: body.seq,
        data: body.data,
        done: body.done,
    });

    Ok(axum::http::StatusCode::OK)
}

async fn local_chat(
    State(state): State<LocalState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<ChatPostRequest>,
) -> Result<Json<ChatPostResponse>, (axum::http::StatusCode, String)> {
    if let Some(err) = check_halted(&state) {
        return Err((axum::http::StatusCode::SERVICE_UNAVAILABLE, err));
    }

    let (agent_id, agent_name) = get_session(&state, &headers)?;
    let content = body.content.trim();
    if content.is_empty() {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
            "content must be 1-500 non-whitespace characters".to_string(),
        ));
    }
    if content.chars().count() > 500 {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
            "content exceeds 500 characters".to_string(),
        ));
    }

    let msg = {
        let mut chat = state.chat_state.lock().unwrap();
        chat.push(state.instance_id, agent_id, agent_name, content.to_string())
    };

    let _ = state.chat_tx.send(msg.clone());

    Ok(Json(ChatPostResponse {
        id: msg.id,
        created_at: msg.created_at,
    }))
}

async fn local_chat_messages(
    State(state): State<LocalState>,
    Query(query): Query<ChatMessagesQuery>,
) -> Result<Json<ChatMessagesResponse>, (axum::http::StatusCode, String)> {
    if let Some(err) = check_halted(&state) {
        return Err((axum::http::StatusCode::SERVICE_UNAVAILABLE, err));
    }

    let instance_id = query.instance_id.unwrap_or(state.instance_id);
    let limit = query
        .limit
        .unwrap_or(LOCAL_CHAT_DEFAULT_LIMIT)
        .clamp(1, LOCAL_CHAT_MAX_LIMIT);
    let after = query.after.as_deref().map(str::trim).filter(|s| !s.is_empty());

    let messages = {
        let chat = state.chat_state.lock().unwrap();
        chat.list(instance_id, after, limit)
    };

    Ok(Json(ChatMessagesResponse { messages }))
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
    if let Some(err) = check_halted(&state) {
        return Err((axum::http::StatusCode::SERVICE_UNAVAILABLE, err));
    }

    let (agent_id, agent_name) = get_session(&state, &headers)?;

    let speech_progress = extract_speech_progress_event(&input.input_type, &input.data);
    let input_type = input.input_type.clone();
    let input_data = input.data.clone();

    game::queue_input(
        &state.game_handle,
        state.game_id,
        agent_id,
        input.input_type,
        input.data,
    )
    .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e))?;

    // Return observation
    let mut observation = game::get_observation(&state.game_handle, state.game_id, agent_id)
        .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e))?;
    resolve_local_player_assets(&mut observation);
    maybe_publish_authoritative_speech_events(&state, &observation.recent_events);

    if let Some(writer) = &state.recording_writer {
        let time_ms = state.started_at.elapsed().as_millis() as u64;
        let tick = observation.tick;
        let writer = writer.clone();
        tokio::spawn(async move {
            let w = writer.lock().await;
            w.record_input(tick, time_ms, &agent_id, &input_type, &input_data)
                .await;
        });
    }

    if let Some(progress) = speech_progress {
        maybe_publish_speech_progress(&state, &observation, &agent_name, progress);
    }

    Ok(Json(observation))
}

#[derive(Clone, Debug)]
struct SpeechProgressInput {
    stream_id: String,
    text: String,
}

fn extract_speech_progress_event(
    input_type: &str,
    data: &serde_json::Value,
) -> Option<SpeechProgressInput> {
    if input_type != "RemoteEvent" {
        return None;
    }
    let name = data.get("name")?.as_str()?;
    if name != "SpeechBus" {
        return None;
    }
    let args = data.get("args")?.as_array()?;
    let payload = args.first()?.as_object()?;
    let op = payload.get("op")?.as_str()?;
    if op != "Progress" {
        return None;
    }

    let stream_id = payload.get("stream_id")?.as_str()?.trim().to_string();
    if stream_id.is_empty() {
        return None;
    }
    let text = payload
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Some(SpeechProgressInput { stream_id, text })
}

fn maybe_publish_speech_progress(
    state: &LocalState,
    observation: &PlayerObservation,
    speaker_name: &str,
    progress: SpeechProgressInput,
) {
    if progress.text.is_empty() {
        return;
    }

    let attrs = &observation.player.attributes;
    let is_speaking = attrs
        .get("IsSpeaking")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if !is_speaking {
        return;
    }

    let observed_stream_id = attrs
        .get("SpeechStreamId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if observed_stream_id != progress.stream_id {
        return;
    }

    let has_lease = attrs
        .get("SpeechLeaseId")
        .and_then(|v| v.as_str())
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    if !has_lease {
        return;
    }

    push_speech_progress_event(
        state,
        speaker_name.to_string(),
        progress.stream_id,
        progress.text,
    );
}

async fn local_observe(
    State(state): State<LocalState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<PlayerObservation>, (axum::http::StatusCode, String)> {
    if let Some(err) = check_halted(&state) {
        return Err((axum::http::StatusCode::SERVICE_UNAVAILABLE, err));
    }

    let (agent_id, _) = get_session(&state, &headers)?;

    let mut observation = game::get_observation(&state.game_handle, state.game_id, agent_id)
        .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e))?;
    resolve_local_player_assets(&mut observation);
    maybe_publish_authoritative_speech_events(&state, &observation.recent_events);

    Ok(Json(observation))
}

fn resolve_local_asset_url(url: &mut String) {
    if let Some(path) = url.strip_prefix("asset://") {
        *url = format!("/assets/{}", path);
    }
}

fn resolve_local_model_url_attr(attrs: &mut std::collections::HashMap<String, serde_json::Value>) {
    if let Some(value) = attrs.get_mut("ModelUrl") {
        if let Some(path) = value.as_str().and_then(|url| url.strip_prefix("asset://")) {
            *value = serde_json::Value::String(format!("/assets/{}", path));
        }
    }
}

/// Resolve asset:// URLs to local /assets/ paths for local spectator observations.
fn resolve_local_spectator_assets(obs: &mut SpectatorObservation) {
    for entity in &mut obs.entities {
        if let Some(ref mut url) = entity.model_url {
            resolve_local_asset_url(url);
        }
    }
}

/// Resolve asset:// URLs to local /assets/ paths for local player observations.
fn resolve_local_player_assets(obs: &mut PlayerObservation) {
    for entity in &mut obs.world.entities {
        if let Some(attrs) = entity.attributes.as_mut() {
            resolve_local_model_url_attr(attrs);
        }
    }
}

async fn local_spectate(
    State(state): State<LocalState>,
) -> Result<Json<SpectatorObservation>, (axum::http::StatusCode, String)> {
    if let Some(err) = check_halted(&state) {
        return Err((axum::http::StatusCode::SERVICE_UNAVAILABLE, err));
    }

    let mut observation =
        game::get_spectator_observation_for_instance(&state.game_handle, state.instance_id)
            .map_err(|e| (axum::http::StatusCode::NOT_FOUND, e))?;

    resolve_local_spectator_assets(&mut observation);

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

async fn local_renderer_manifest(State(state): State<LocalState>) -> Json<LocalRendererManifest> {
    Json(state.renderer_manifest)
}

async fn local_spectate_ws(
    State(state): State<LocalState>,
    Query(params): Query<SpectateWsQuery>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let role = SpectateRole::from_param(params.role.as_deref());
    ws.on_upgrade(move |socket| handle_spectate_ws(socket, state, role))
}

#[derive(Debug, Deserialize)]
struct SpectateWsQuery {
    role: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SpectateRole {
    Actor,
    Spectator,
}

impl SpectateRole {
    fn from_param(role: Option<&str>) -> Self {
        match role.map(|s| s.trim().to_ascii_lowercase()) {
            Some(s) if s == "actor" => Self::Actor,
            _ => Self::Spectator,
        }
    }
}

async fn handle_spectate_ws(socket: WebSocket, state: LocalState, role: SpectateRole) {
    let (mut sender, mut receiver) = socket.split();
    let mut audio_rx = state.audio_tx.subscribe();
    let mut speech_rx = state.speech_tx.subscribe();
    let mut speech_progress_rx = state.speech_progress_tx.subscribe();
    let mut playback_done_rx = state.playback_done_tx.subscribe();
    let mut chat_rx = state.chat_tx.subscribe();

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
                    Some(Ok(Message::Text(_))) => {}
                    _ => {}
                }
            }

            chunk = audio_rx.recv() => {
                if let Ok(chunk) = chunk {
                    let json = serde_json::to_string(&serde_json::json!({
                        "type": "audio_chunk",
                        "speaker": chunk.speaker,
                        "stream_id": chunk.stream_id,
                        "seq": chunk.seq,
                        "data": chunk.data,
                        "done": chunk.done,
                    })).unwrap_or_default();
                    if sender.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
            }

            speech = speech_rx.recv() => {
                if let Ok(speech) = speech {
                    let json = serde_json::to_string(&serde_json::json!({
                        "type": "speech",
                        "speaker": speech.speaker,
                        "text": speech.text,
                        "seq": speech.seq,
                    })).unwrap_or_default();
                    if sender.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
            }

            speech_progress = speech_progress_rx.recv() => {
                if let Ok(progress) = speech_progress {
                    if role == SpectateRole::Spectator {
                        let json = serde_json::to_string(&serde_json::json!({
                            "type": "speech_progress",
                            "speaker": progress.speaker,
                            "stream_id": progress.stream_id,
                            "text": progress.text,
                            "seq": progress.seq,
                        })).unwrap_or_default();
                        if sender.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                }
            }

            playback_done = playback_done_rx.recv() => {
                if let Ok(event) = playback_done {
                    let json = serde_json::to_string(&serde_json::json!({
                        "type": "playback_done",
                        "speaker": event.speaker,
                        "stream_id": event.stream_id,
                        "seq": event.seq,
                    })).unwrap_or_default();
                    if sender.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
            }

            chat = chat_rx.recv() => {
                if let Ok(chat) = chat {
                    let json = serde_json::to_string(&serde_json::json!({
                        "type": "chat_message",
                        "id": chat.id,
                        "instance_id": chat.instance_id,
                        "agent_id": chat.agent_id,
                        "agent_name": chat.agent_name,
                        "message_type": chat.message_type,
                        "content": chat.content,
                        "created_at": chat.created_at,
                    })).unwrap_or_default();
                    if sender.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
            }

            _ = tick_interval.tick() => {
                let observation =
                    game::get_spectator_observation_for_instance(&state.game_handle, state.instance_id);

                match observation {
                    Ok(obs) => {
                        let mut obs = obs;
                        resolve_local_spectator_assets(&mut obs);
                        maybe_publish_authoritative_speech_events(&state, &obs.recent_events);

                        if obs.tick != last_tick {
                            last_tick = obs.tick;
                            same_tick_count = 0;
                            if let Ok(json) = serde_json::to_vec(&obs) {
                                let msg =
                                    Message::Text(String::from_utf8_lossy(&json).into_owned().into());
                                if sender.send(msg).await.is_err() {
                                    break;
                                }
                            }
                        } else {
                            same_tick_count += 1;
                            if same_tick_count >= 5 {
                                same_tick_count = 0;
                                if let Ok(json) = serde_json::to_vec(&obs) {
                                    let msg = Message::Text(
                                        String::from_utf8_lossy(&json).into_owned().into(),
                                    );
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
