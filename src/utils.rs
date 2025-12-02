use std::path::PathBuf;

use dialoguer::{Select, theme::ColorfulTheme};
use dirs::home_dir;

use crate::game::starrail::get_patch_file_path;

#[derive(Debug, Clone, PartialEq)]
pub enum GameType {
    Genshin,
    StarRail,
    Unknown,
}

pub struct GameInfo {
    pub game_exe: String,
    pub game_type: GameType,
}

pub fn detect_game(args: &[String]) -> GameInfo {
    for arg in args {
        if arg.contains("GenshinImpact.exe") {
            return GameInfo {
                game_exe: arg.clone(),
                game_type: GameType::Genshin,
            };
        } else if arg.contains("StarRail.exe") {
            return GameInfo {
                game_exe: arg.clone(),
                game_type: GameType::StarRail,
            };
        } else if arg.ends_with(".exe") {
            return GameInfo {
                game_exe: arg.clone(),
                game_type: GameType::Unknown,
            };
        }
    }

    GameInfo {
        game_exe: String::new(),
        game_type: GameType::Unknown,
    }
}

pub fn select_with_arrows(prompt: &str, options: &[&str]) -> Option<usize> {
    Select::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .items(options)
        .default(0)
        .interact_opt()
        .expect("Selection failed")
}

pub fn get_patch_dir() -> PathBuf {
    home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".local")
        .join("share")
        .join("anime-games-proxy")
        .join("patch")
}

pub fn modify_command_for_game(
    args: &[String],
    game_info: &GameInfo,
) -> Result<Vec<String>, String> {
    let mut modified_args = args.to_vec();

    if game_info.game_type == GameType::StarRail {
        let injector_path = get_patch_file_path();

        tracing::info!("StarRail detected");
        tracing::info!("Using injector: {}", injector_path.display());

        if let Some(game_exe_index) = modified_args
            .iter()
            .position(|arg| arg.contains("StarRail.exe"))
        {
            let original_exe = modified_args[game_exe_index].clone();
            modified_args[game_exe_index] = injector_path.to_string_lossy().to_string();
            modified_args.insert(game_exe_index + 1, original_exe);
        }
    }

    Ok(modified_args)
}
