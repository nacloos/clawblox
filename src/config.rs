//! World configuration parsing from world.toml files

use serde::Deserialize;
use std::path::Path;

/// Scripts configuration section
#[derive(Debug, Clone, Deserialize)]
pub struct ScriptsConfig {
    /// Main Lua script file (relative to game directory)
    pub main: String,
    /// Optional script tree directory (relative to game directory).
    /// Expected layout uses service roots like:
    /// - scripts/ServerScriptService/**/*.lua
    /// - scripts/Workspace/**/*.lua
    #[serde(default)]
    pub tree: Option<String>,
    /// Skill definition file (relative to game directory)
    #[serde(default)]
    pub skill: Option<String>,
}

impl Default for ScriptsConfig {
    fn default() -> Self {
        Self {
            main: "main.lua".to_string(),
            tree: None,
            skill: Some("SKILL.md".to_string()),
        }
    }
}

/// Runtime mode for local execution.
///
/// - `clawblox` (default): run the built-in Lua simulation.
/// - `external`: serve frontend/renderer only (no Lua runtime).
#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeConfig {
    #[serde(default = "default_runtime_mode")]
    pub mode: String,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            mode: default_runtime_mode(),
        }
    }
}

/// Optional external run command configuration.
///
/// When configured, `clawblox run` can delegate process startup to this command.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct RunConfig {
    /// Command to execute, argv-style.
    /// Example: ["node", "play.js"]
    #[serde(default)]
    pub command: Option<Vec<String>>,
}

/// Renderer configuration section.
#[derive(Debug, Clone, Deserialize)]
pub struct RendererConfig {
    /// Human-readable renderer name shown in local runtime UI.
    #[serde(default = "default_renderer_name")]
    pub name: String,
    /// Renderer host mode. Currently "module" or "default".
    #[serde(default = "default_renderer_mode")]
    pub mode: String,
    /// Renderer API contract version expected by this game.
    #[serde(default = "default_renderer_api_version")]
    pub api_version: u32,
    /// Optional renderer module entrypoint relative to game_dir/renderer/.
    #[serde(default)]
    pub entry: Option<String>,
    /// Optional renderer source entrypoint relative to game_dir/renderer/.
    /// If set, local CLI can bundle this into a runtime-friendly ESM module.
    #[serde(default)]
    pub source: Option<String>,
    /// Optional declared capabilities for docs and tooling.
    #[serde(default)]
    pub capabilities: Vec<String>,
}

impl Default for RendererConfig {
    fn default() -> Self {
        Self {
            name: default_renderer_name(),
            mode: default_renderer_mode(),
            api_version: default_renderer_api_version(),
            entry: None,
            source: None,
            capabilities: vec![],
        }
    }
}

/// Observation configuration section.
#[derive(Debug, Clone, Deserialize)]
pub struct ObservationConfig {
    /// Maximum distance at which other players are included in observations.
    #[serde(default = "default_observation_radius")]
    pub radius: f32,
}

impl Default for ObservationConfig {
    fn default() -> Self {
        Self {
            radius: default_observation_radius(),
        }
    }
}

/// World configuration from world.toml
#[derive(Debug, Clone, Deserialize)]
pub struct WorldConfig {
    /// Display name of the game
    pub name: String,
    /// Description of the game
    #[serde(default)]
    pub description: Option<String>,
    /// Maximum number of players per instance
    #[serde(default = "default_max_players")]
    pub max_players: u32,
    /// Scripts configuration
    #[serde(default)]
    pub scripts: ScriptsConfig,
    /// Runtime mode selection
    #[serde(default)]
    pub runtime: RuntimeConfig,
    /// Frontend renderer configuration
    #[serde(default)]
    pub renderer: RendererConfig,
    /// Observation configuration
    #[serde(default)]
    pub observation: ObservationConfig,
    /// Optional external runtime command
    #[serde(default)]
    pub run: RunConfig,
}

fn default_max_players() -> u32 {
    8
}

fn default_renderer_name() -> String {
    "Game Renderer".to_string()
}

fn default_renderer_mode() -> String {
    "module".to_string()
}

fn default_renderer_api_version() -> u32 {
    1
}

fn default_observation_radius() -> f32 {
    100.0
}

fn default_runtime_mode() -> String {
    "clawblox".to_string()
}

impl WorldConfig {
    /// Load world configuration from a TOML file
    pub fn from_file(path: &Path) -> Result<Self, WorldConfigError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| WorldConfigError::IoError(path.to_path_buf(), e))?;

        let cfg: WorldConfig = toml::from_str(&content)
            .map_err(|e| WorldConfigError::ParseError(path.to_path_buf(), e))?;
        cfg.validate(path)?;
        Ok(cfg)
    }

    /// Load world configuration from a game directory
    /// Looks for world.toml in the given directory
    pub fn from_game_dir(game_dir: &Path) -> Result<Self, WorldConfigError> {
        let config_path = game_dir.join("world.toml");
        Self::from_file(&config_path)
    }

    fn validate(&self, path: &Path) -> Result<(), WorldConfigError> {
        if !self.observation.radius.is_finite() || self.observation.radius <= 0.0 {
            return Err(WorldConfigError::ValidationError(
                path.to_path_buf(),
                "observation.radius must be a finite number > 0".to_string(),
            ));
        }
        if let Some(cmd) = self.run.command.as_ref() {
            if cmd.is_empty() {
                return Err(WorldConfigError::ValidationError(
                    path.to_path_buf(),
                    "run.command must contain at least one token".to_string(),
                ));
            }
            if cmd[0].trim().is_empty() {
                return Err(WorldConfigError::ValidationError(
                    path.to_path_buf(),
                    "run.command[0] must be a non-empty executable".to_string(),
                ));
            }
        }
        Ok(())
    }
}

/// Errors that can occur when loading world configuration
#[derive(Debug)]
pub enum WorldConfigError {
    IoError(std::path::PathBuf, std::io::Error),
    ParseError(std::path::PathBuf, toml::de::Error),
    ValidationError(std::path::PathBuf, String),
}

impl std::fmt::Display for WorldConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorldConfigError::IoError(path, e) => {
                write!(f, "Failed to read {}: {}", path.display(), e)
            }
            WorldConfigError::ParseError(path, e) => {
                write!(f, "Failed to parse {}: {}", path.display(), e)
            }
            WorldConfigError::ValidationError(path, msg) => {
                write!(f, "Invalid config {}: {}", path.display(), msg)
            }
        }
    }
}

impl std::error::Error for WorldConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_config() {
        let toml = r#"
            name = "Test Game"
        "#;
        let config: WorldConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.name, "Test Game");
        assert_eq!(config.max_players, 8);
        assert_eq!(config.runtime.mode, "clawblox");
        assert_eq!(config.observation.radius, 100.0);
    }

    #[test]
    fn test_parse_full_config() {
        let toml = r#"
            name = "Test Game"
            description = "A test game"
            max_players = 16
            [runtime]
            mode = "external"

            [scripts]
            main = "main.lua"
            tree = "scripts"
            skill = "SKILL.md"

            [renderer]
            name = "Test Renderer"
            mode = "module"
            api_version = 1
            entry = "index.js"
            source = "src/index.ts"
            capabilities = ["camera.follow"]

            [observation]
            radius = 64

            [run]
            command = ["custom-runner", "--flag"]
        "#;
        let config: WorldConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.name, "Test Game");
        assert_eq!(config.description, Some("A test game".to_string()));
        assert_eq!(config.max_players, 16);
        assert_eq!(config.runtime.mode, "external");
        assert_eq!(config.scripts.main, "main.lua");
        assert_eq!(config.scripts.tree, Some("scripts".to_string()));
        assert_eq!(config.renderer.mode, "module");
        assert_eq!(config.renderer.entry.as_deref(), Some("index.js"));
        assert_eq!(config.renderer.source.as_deref(), Some("src/index.ts"));
        assert_eq!(config.renderer.api_version, 1);
        assert_eq!(config.observation.radius, 64.0);
        assert_eq!(
            config.run.command.as_ref().map(|c| c.as_slice()),
            Some(["custom-runner", "--flag"].as_slice())
        );
    }

    #[test]
    fn test_invalid_observation_radius_fails_validation() {
        let toml = r#"
            name = "Bad Config"

            [observation]
            radius = 0
        "#;

        let config: WorldConfig = toml::from_str(toml).unwrap();
        let err = config
            .validate(std::path::Path::new("world.toml"))
            .unwrap_err();
        assert!(format!("{}", err).contains("observation.radius"));
    }
}
