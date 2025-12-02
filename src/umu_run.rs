use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

#[derive(Deserialize, Serialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize, Serialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

pub struct UmuRun {
    pub binary: String,
}

impl Default for UmuRun {
    fn default() -> Self {
        Self {
            binary: "umu-run".to_string(),
        }
    }
}

impl UmuRun {
    pub fn is_installed(&self) -> bool {
        if let Ok(path) = std::env::var("PATH") {
            for dir in std::env::split_paths(&path) {
                if dir.join(&self.binary).exists() {
                    return true;
                }
            }
        }
        let home = dirs::home_dir()
            .expect("Failed to get home directory")
            .to_str()
            .unwrap()
            .to_string();
        Path::new(&home)
            .join(".local/share/anime-games-proxy")
            .join(&self.binary)
            .exists()
    }

    pub async fn install(&self) -> Result<(), Box<dyn std::error::Error>> {
        let client = Client::new();
        let url = "https://api.github.com/repos/Open-Wine-Components/umu-launcher/releases";
        let response = client.get(url).send().await?;
        let release: Release = response.json().await?;

        let asset = release
            .assets
            .iter()
            .find(|a| a.name.contains("zipapp") && a.name.ends_with(".tar"))
            .ok_or("No suitable zipapp tar asset found")?;

        let download_response = client.get(&asset.browser_download_url).send().await?;
        let bytes = download_response.bytes().await?;
        let temp_dir = std::env::temp_dir();
        fs::create_dir_all(&temp_dir)?;
        let tar_path = temp_dir.join(&asset.name);
        let mut file = tokio::fs::File::create(&tar_path).await?;
        file.write_all(&bytes).await?;

        let output = Command::new("tar")
            .args([
                "-xf",
                &tar_path.to_string_lossy(),
                "-C",
                &temp_dir.to_string_lossy(),
            ])
            .output()?;
        if !output.status.success() {
            return Err("Failed to extract archive".into());
        }

        let home = std::env::var("HOME")?;
        let bin_dir = Path::new(&home).join(".local/share/anime-games-proxy");
        fs::create_dir_all(&bin_dir)?;
        let extracted_bin = temp_dir.join("umu-run");
        fs::rename(extracted_bin, bin_dir.join(&self.binary))?;
        let mut perms = fs::metadata(bin_dir.join(&self.binary))?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(bin_dir.join(&self.binary), perms)?;

        fs::remove_dir_all(temp_dir)?;

        Ok(())
    }
}
