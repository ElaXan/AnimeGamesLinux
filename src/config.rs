use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ConfigGame {
    pub name: String,
    pub wineprefix: String,
    pub proton_wine_path: String,
    pub server: String,
    pub server_port: i32,
    pub use_ssl: bool,
}

impl Default for ConfigGame {
    fn default() -> Self {
        Self {
            name: String::new(),
            wineprefix: String::new(),
            proton_wine_path: String::new(),
            server: "127.0.0.1".to_string(),
            server_port: 80,
            use_ssl: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub games: Vec<ConfigGame>,
    pub proxy_port: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            games: Vec::new(),
            proxy_port: "8080".to_string(),
        }
    }
}

impl Config {
    fn config_paths() -> Result<PathBuf, Box<dyn std::error::Error>> {
        let home_dir = dirs::home_dir().ok_or("Failed to get home directory")?;
        let config_dir = home_dir.join(".config").join("anime-games-proxy");
        if !config_dir.exists() {
            std::fs::create_dir_all(&config_dir)?;
        }
        Ok(config_dir.join("config.json"))
    }

    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let config_file = Self::config_paths()?;

        if config_file.exists() {
            tracing::info!("Loading config from {}", config_file.display());
            let config_data = std::fs::read_to_string(&config_file)?;
            let config: Config = serde_json::from_str(&config_data)?;
            Ok(config)
        } else {
            tracing::info!(
                "Config file not found at {}. Creating default config.",
                config_file.display()
            );
            Self::save(&Config::default())?;
            Ok(Config::default())
        }
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config_file = Self::config_paths()?;
        let config_data = serde_json::to_string_pretty(self)?;
        std::fs::write(&config_file, config_data)?;
        Ok(())
    }

    pub fn saved_runner_for_exe(&self, game_exe: &str) -> Option<String> {
        let exe_game = std::path::Path::new(game_exe)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(game_exe);

        if let Some(game) = self.games.iter().find(|g| g.name == exe_game) {
            if !game.proton_wine_path.trim().is_empty() {
                return Some(game.proton_wine_path.clone());
            }
            if !game.wineprefix.trim().is_empty() {
                return Some(game.wineprefix.clone());
            }
        }

        None
    }

    pub fn set_runner_for_exe(&mut self, game_exe: &str, runner_path: String) {
        let exe_game = std::path::Path::new(game_exe)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(game_exe)
            .to_string();

        if let Some(game) = self.games.iter_mut().find(|game| game.name == exe_game) {
            game.proton_wine_path = runner_path;
        } else {
            let new_game = ConfigGame {
                name: exe_game,
                proton_wine_path: runner_path,
                ..Default::default()
            };
            self.games.push(new_game);
        }
    }

    /// Returns server settings (server, port, use_ssl) for a given game executable.
    /// If a matching game isn't found, returns `None`.
    pub fn server_for_exe(&self, game_exe: &str) -> Option<(String, i32, bool)> {
        let exe_game = std::path::Path::new(game_exe)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(game_exe);

        if let Some(game) = self.games.iter().find(|g| g.name == exe_game) {
            return Some((game.server.clone(), game.server_port, game.use_ssl));
        }

        None
    }
}
