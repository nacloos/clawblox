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
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tokio::time::interval;
use tower_http::cors::{Any, CorsLayer};
use uuid::Uuid;

#[cfg(unix)]
use libc;

use include_dir::{include_dir, Dir};
use tower_http::services::ServeDir;

use clawblox::config::WorldConfig;
use clawblox::game::{
    self, find_or_create_instance,
    instance::{ErrorMode, PlayerObservation, SpectatorObservation},
    GameManager, GameManagerHandle,
};

static DOCS_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/docs");

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
        /// Name for the new project (creates subdirectory). Omit to init in current directory.
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
        /// Run as daemon (internal use only)
        #[arg(long, hide = true)]
        daemon: bool,
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
    Docs,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { name } => init_project(name),
        Commands::Run { path, port, daemon } => run_game(path, port, daemon),
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
        Commands::Docs => fetch_docs(),
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
        let versions_dir = home.join(".local").join("share").join("clawblox").join("versions");
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

fn fetch_docs() {
    let docs_dir = std::env::current_dir()
        .expect("Failed to get current directory")
        .join("docs");

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

const GITHUB_DOCS_API_URL: &str =
    "https://api.github.com/repos/nacloos/clawblox/contents/docs";

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

    // Fetch latest docs from GitHub, fall back to embedded copy
    {
        let docs_dir = project_dir.join("docs");
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
        match rt.block_on(fetch_docs_from_github(&docs_dir)) {
            Ok(()) => {}
            Err(e) => {
                eprintln!("Warning: Could not fetch docs from GitHub ({}), using bundled copy", e);
                write_embedded_dir(&DOCS_DIR, &docs_dir);
            }
        }
    }

    // Create assets/ directory for game assets (GLB models, images, audio)
    std::fs::create_dir_all(project_dir.join("assets"))
        .expect("Failed to create assets directory");

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

    // Load world config
    let config = WorldConfig::from_game_dir(&path).unwrap_or_else(|e| {
        eprintln!("Error loading world.toml: {}", e);
        std::process::exit(1);
    });

    // Read Lua script
    let script_path = path.join(&config.scripts.main);
    let script_code = std::fs::read_to_string(&script_path).unwrap_or_else(|e| {
        eprintln!("Error reading {}: {}", script_path.display(), e);
        std::process::exit(1);
    });

    // Read SKILL.md if configured
    let skill_md = config
        .scripts
        .skill
        .as_ref()
        .and_then(|skill_file| {
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
                    "game_type": config.game_type,
                    "script_code": script_code,
                    "skill_md": skill_md,
                    "max_players": config.max_players as i32,
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
                    "game_type": config.game_type,
                    "script_code": script_code,
                    "skill_md": skill_md,
                    "max_players": config.max_players as i32,
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
                tar_builder.append_dir_all(".", &assets_dir).unwrap_or_else(|e| {
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

fn run_game(path: PathBuf, port: u16, daemon: bool) {
    let path = std::fs::canonicalize(&path).unwrap_or_else(|_| {
        eprintln!("Error: Path '{}' does not exist", path.display());
        std::process::exit(1);
    });

    if !daemon {
        // Non-daemon mode: kill old instance, spawn child daemon, return immediately
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
            .args(["run", &path.to_string_lossy(), "--port", &port.to_string(), "--daemon"])
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

        println!("Server starting on http://localhost:{}", port);
        println!("Logs: {}", log_file_path.display());
        return;
    }

    // Daemon mode: run the server (parent already killed old instance and wrote PID file)

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

    let pid_file = pid_path(port);

    println!("Starting {} (max {} players)", config.name, config.max_players);
    println!("Script: {}", config.scripts.main);

    // Create game manager without database (Halt mode: stop on first Lua error)
    let (game_manager, game_handle) = GameManager::new_without_db(60, ErrorMode::Halt);

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
            log_file: path.join(".clawblox.log"),
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
            .nest_service("/assets", ServeDir::new(path.join("assets")))
            .nest_service("/static", ServeDir::new(path.join("static")))
            .with_state(state)
            .layer(cors);

        let addr = SocketAddr::from(([0, 0, 0, 0], port));
        println!();
        println!("Server running on http://localhost:{}", port);
        println!();
        println!("Endpoints:");
        println!("  POST /join?name=X  - Join game, returns session token");
        println!("  POST /input        - Send input (requires X-Session header)");
        println!("  GET  /observe      - Player observation (requires X-Session header)");
        println!("  GET  /skill.md     - Game skill definition");

        let listener = tokio::net::TcpListener::bind(addr).await.unwrap_or_else(|e| {
            delete_pid_file(&pid_file);
            eprintln!(
                "Error: Port {} is already in use. Try --port <PORT>",
                port
            );
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

        // Also clean up here in case shutdown_signal didn't fire
        delete_pid_file(&pid_file);
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
    log_file: PathBuf,
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
    if let Some(err) = check_halted(&state) {
        return Err((axum::http::StatusCode::SERVICE_UNAVAILABLE, err));
    }

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
    if let Some(err) = check_halted(&state) {
        return Err((axum::http::StatusCode::SERVICE_UNAVAILABLE, err));
    }

    let (agent_id, _) = get_session(&state, &headers)?;

    let observation = game::get_observation(&state.game_handle, state.game_id, agent_id)
        .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e))?;

    Ok(Json(observation))
}

/// Resolve asset:// URLs to local /assets/ paths for local development.
fn resolve_local_assets(obs: &mut SpectatorObservation) {
    for entity in &mut obs.entities {
        if let Some(ref mut url) = entity.model_url {
            if let Some(path) = url.strip_prefix("asset://") {
                *url = format!("/assets/{}", path);
            }
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

    resolve_local_assets(&mut observation);

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
                        let mut obs = obs;
                        resolve_local_assets(&mut obs);

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
