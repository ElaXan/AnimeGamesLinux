use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};

use dirs::home_dir;

const GENSHIN_IMPACT_REQUIRED_VERSION: &str = "5.0.0";
const GENSHIN_IMPACT_DOWNLOAD_URL: &str =
    "https://github.com/ElaXan/hk4e-patch-universal/releases/download/1/Astrolabe.dll";
const PATCH_FILENAME: &str = "Astrolabe.dll";

#[derive(Debug)]
pub enum GenshinError {
    IoError(std::io::Error),
    DownloadError(String),
    InvalidVersion(String),
    PathNotFound(String),
}

impl std::fmt::Display for GenshinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GenshinError::IoError(e) => write!(f, "IO error: {}", e),
            GenshinError::DownloadError(e) => write!(f, "Download error: {}", e),
            GenshinError::InvalidVersion(e) => write!(f, "Invalid version: {}", e),
            GenshinError::PathNotFound(e) => write!(f, "Path not found: {}", e),
        }
    }
}

impl From<std::io::Error> for GenshinError {
    fn from(error: std::io::Error) -> Self {
        GenshinError::IoError(error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Version {
    major: u32,
    minor: u32,
    patch: u32,
}

impl Version {
    fn parse(version_str: &str) -> Result<Self, GenshinError> {
        let parts: Vec<&str> = version_str.split('.').collect();

        if parts.is_empty() {
            return Err(GenshinError::InvalidVersion(format!(
                "Empty version string: {}",
                version_str
            )));
        }

        let major = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);

        let minor = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);

        let patch = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);

        Ok(Version {
            major,
            minor,
            patch,
        })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.major.cmp(&other.major) {
            Ordering::Equal => match self.minor.cmp(&other.minor) {
                Ordering::Equal => self.patch.cmp(&other.patch),
                other => other,
            },
            other => other,
        }
    }
}

pub struct GenshinPatcher {
    game_dir: PathBuf,
    plugins_dir: PathBuf,
    patched: bool,
}

impl GenshinPatcher {
    pub fn new<P: AsRef<Path>>(game_exe: P) -> Result<Self, GenshinError> {
        let game_exe = game_exe.as_ref();

        if !game_exe.exists() {
            return Err(GenshinError::PathNotFound(format!(
                "Game executable not found: {}",
                game_exe.display()
            )));
        }

        let game_dir = game_exe
            .parent()
            .ok_or_else(|| {
                GenshinError::PathNotFound("Could not determine game directory".to_string())
            })?
            .to_path_buf();

        let plugins_dir = game_dir.join("GenshinImpact_Data").join("Plugins");

        Ok(Self {
            game_dir,
            plugins_dir,
            patched: false,
        })
    }

    pub fn read_game_version(&self) -> Result<String, GenshinError> {
        let config_file = self.game_dir.join("config.ini");

        if !config_file.exists() {
            return Err(GenshinError::PathNotFound(format!(
                "config.ini not found at: {}",
                config_file.display()
            )));
        }

        let content = fs::read_to_string(&config_file)?;

        for line in content.lines() {
            if line.starts_with("game_version=") {
                let version = line
                    .trim_start_matches("game_version=")
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_string();

                if !version.is_empty() {
                    return Ok(version);
                }
            }
        }

        Err(GenshinError::InvalidVersion(
            "Could not find game_version in config.ini".to_string(),
        ))
    }

    async fn download_patch(&self) -> Result<(), GenshinError> {
        let patch_dir = Self::get_patch_dir();

        if patch_dir.join(PATCH_FILENAME).exists() {
            tracing::info!("Patch file already exists at: {}", patch_dir.display());
            return Ok(());
        }

        tracing::info!("Downloading patch from: {}", GENSHIN_IMPACT_DOWNLOAD_URL);

        let response = reqwest::get(GENSHIN_IMPACT_DOWNLOAD_URL)
            .await
            .expect("Failed to send request");
        if !response.status().is_success() {
            return Err(GenshinError::DownloadError(format!(
                "Failed to download patch. HTTP Status: {}",
                response.status()
            )));
        }

        let bytes = response.bytes().await.map_err(|e| {
            GenshinError::DownloadError(format!("Failed to read response bytes: {}", e))
        })?;

        tokio::fs::create_dir_all(&patch_dir).await?;
        let mut file = tokio::fs::File::create(&patch_dir.join(PATCH_FILENAME)).await?;
        tokio::io::copy(&mut bytes.as_ref(), &mut file).await?;

        tracing::info!("Patch downloaded successfully to: {}", patch_dir.display());

        Ok(())
    }

    pub fn get_patch_dir() -> PathBuf {
        home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(".local")
            .join("share")
            .join("anime-games-proxy")
            .join("patch")
            .join("genshin")
    }

    fn apply_patch(&mut self) -> Result<(), GenshinError> {
        if !self.plugins_dir.exists() {
            return Err(GenshinError::PathNotFound(format!(
                "Plugins directory not found at: {}",
                self.plugins_dir.display()
            )));
        }

        let source_path = Self::get_patch_dir().join(PATCH_FILENAME);
        let target_patch = self.plugins_dir.join(PATCH_FILENAME);
        let backup_patch = Self::get_patch_dir().join(format!("{}.bak", PATCH_FILENAME));

        if !source_path.exists() {
            return Err(GenshinError::PathNotFound(format!(
                "Source patch file not found at: {}",
                source_path.display()
            )));
        }

        if !target_patch.exists() {
            return Err(GenshinError::PathNotFound(format!(
                "Original {} not found in the game directory",
                PATCH_FILENAME
            )));
        }

        let target_metadata = fs::metadata(&target_patch)?;
        let source_metadata = fs::metadata(&source_path)?;

        if source_metadata.len() >= target_metadata.len() {
            tracing::info!("Game is already patched or patch is not needed.");
            self.patched = true;
            return Ok(());
        }

        if !backup_patch.exists() {
            fs::copy(&target_patch, &backup_patch)?;
            tracing::info!(
                "Backup of original patch created at: {}",
                backup_patch.display()
            );
        }

        tracing::info!("Applying patch...");
        fs::copy(source_path, &target_patch)?;
        tracing::info!("Patch applied successfully");

        self.patched = true;
        Ok(())
    }

    pub fn unpatch(&mut self) -> Result<(), GenshinError> {
        if !self.patched {
            return Ok(());
        }

        let target_patch = self.plugins_dir.join(PATCH_FILENAME);
        let backup_patch = Self::get_patch_dir().join(format!("{}.bak", PATCH_FILENAME));

        if backup_patch.exists() {
            tracing::info!("Unpatching the game...");
            fs::copy(backup_patch, target_patch)?;
            tracing::info!("Game unpatched successfully");
        }
        self.patched = false;
        Ok(())
    }

    pub async fn check_and_patch(&mut self) -> Result<bool, GenshinError> {
        tracing::info!("Genshin Impact detected. Checking if patch is needed...");

        if !self.plugins_dir.exists() {
            tracing::warn!(
                "Plugins directory not found at {}",
                self.plugins_dir.display()
            );
            return Ok(false);
        }

        let version_str = match self.read_game_version() {
            Ok(v) => {
                tracing::info!("Game version: {}", v);
                v
            }
            Err(e) => {
                tracing::warn!("{}", e);
                return Ok(false);
            }
        };

        let game_version = Version::parse(&version_str)?;
        let required_version = Version::parse(GENSHIN_IMPACT_REQUIRED_VERSION)?;

        if game_version >= required_version {
            tracing::info!("Patching game...");

            self.download_patch().await?;
            self.apply_patch()?;
            Ok(true)
        } else {
            tracing::warn!(
                "Game version {} is older than {}. Skipping patch.",
                version_str,
                GENSHIN_IMPACT_REQUIRED_VERSION
            );
            Ok(false)
        }
    }

    pub fn is_patched(&self) -> bool {
        self.patched
    }
}
