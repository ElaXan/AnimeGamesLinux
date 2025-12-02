use std::{fs, path::PathBuf};

use reqwest::Client;
use serde_json::Value;

use crate::utils::get_patch_dir;

pub fn get_patch_file_path() -> PathBuf {
    get_patch_dir().join("star-rail").join("jadeite.exe")
}

const RELEASES_API: &str = "https://codeberg.org/api/v1/repos/mkrsym1/jadeite/releases";

pub async fn download_latest_patch() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let client = Client::new();
    let resp = client.get(RELEASES_API).send().await?.error_for_status()?;
    let releases: Value = resp.json().await?;
    let arr = releases.as_array().ok_or("unexpected releases json")?;

    let release = arr
        .iter()
        .find(|rel| {
            rel.get("draft").and_then(Value::as_bool) != Some(true)
                && rel.get("prerelease").and_then(Value::as_bool) != Some(true)
        })
        .ok_or("no releases found")?;

    if let Some(assets) = release.get("assets").and_then(Value::as_array) {
        for asset in assets {
            if let Some(url) = asset.get("browser_download_url").and_then(Value::as_str) {
                let name = asset
                    .get("name")
                    .and_then(Value::as_str)
                    .map(|s| s.to_string())
                    .or_else(|| url.split('/').next_back().map(|s| s.to_string()))
                    .ok_or("cannot determine asset name")?;
                let out_path = get_patch_dir().join("star-rail").join(&name);
                if let Some(parent) = out_path.parent() {
                    fs::create_dir_all(parent)?;
                }

                let mut resp = client.get(url).send().await?.error_for_status()?;
                let mut file = fs::File::create(&out_path)?;
                while let Some(chunk) = resp.chunk().await? {
                    std::io::copy(&mut chunk.as_ref(), &mut file)?;
                }

                if name.to_lowercase().ends_with(".zip") {
                    let zip_file = fs::File::open(&out_path)?;
                    let mut archive = zip::ZipArchive::new(zip_file)?;

                    let dest_path = get_patch_file_path();
                    if let Some(parent) = dest_path.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    archive.extract(dest_path.parent().unwrap())?;

                    fs::remove_file(&out_path)?; // remove zip after extraction
                    return Ok(dest_path);
                }

                return Ok(out_path);
            }
        }
    }

    Err("no downloadable asset found in release".into())
}
