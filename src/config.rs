//! World configuration parsing from world.toml files

use serde::Deserialize;
use std::path::Path;

/// Scripts configuration section
#[derive(Debug, Clone, Deserialize)]
pub struct ScriptsConfig {
    /// Main Lua script file (relative to game directory)
    pub main: String,
    /// Skill definition file (relative to game directory)
    #[serde(default)]
    pub skill: Option<String>,
}

impl Default for ScriptsConfig {
    fn default() -> Self {
        Self {
            main: "main.lua".to_string(),
            skill: Some("SKILL.md".to_string()),
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
    /// Game type (e.g., "lua")
    #[serde(default = "default_game_type")]
    pub game_type: String,
    /// Scripts configuration
    #[serde(default)]
    pub scripts: ScriptsConfig,
}

fn default_max_players() -> u32 {
    8
}

fn default_game_type() -> String {
    "lua".to_string()
}

impl WorldConfig {
    /// Load world configuration from a TOML file
    pub fn from_file(path: &Path) -> Result<Self, WorldConfigError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| WorldConfigError::IoError(path.to_path_buf(), e))?;

        toml::from_str(&content)
            .map_err(|e| WorldConfigError::ParseError(path.to_path_buf(), e))
    }

    /// Load world configuration from a game directory
    /// Looks for world.toml in the given directory
    pub fn from_game_dir(game_dir: &Path) -> Result<Self, WorldConfigError> {
        let config_path = game_dir.join("world.toml");
        Self::from_file(&config_path)
    }
}

/// Errors that can occur when loading world configuration
#[derive(Debug)]
pub enum WorldConfigError {
    IoError(std::path::PathBuf, std::io::Error),
    ParseError(std::path::PathBuf, toml::de::Error),
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
        assert_eq!(config.game_type, "lua");
    }

    #[test]
    fn test_parse_full_config() {
        let toml = r#"
            name = "Test Game"
            description = "A test game"
            max_players = 16
            game_type = "lua"

            [scripts]
            main = "main.lua"
            skill = "SKILL.md"
        "#;
        let config: WorldConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.name, "Test Game");
        assert_eq!(config.description, Some("A test game".to_string()));
        assert_eq!(config.max_players, 16);
        assert_eq!(config.scripts.main, "main.lua");
    }
}
