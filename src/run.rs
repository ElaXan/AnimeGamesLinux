use std::{path::PathBuf, process::Stdio};
use tokio::process::Command as TokioCommand;

use crate::{
    config::Config,
    game::{genshin::GenshinPatcher, starrail},
    get_wine::{RunnerType, find_proton_dirs, find_wine_binaries},
    umu_run::UmuRun,
    utils::{GameType, select_with_arrows},
};

pub async fn execute_command(
    modified_args: Vec<String>,
    proxy: String,
    game_info: &crate::utils::GameInfo,
    wrapper: bool,
) -> Result<i32, Box<dyn std::error::Error>> {
    let umu_run = UmuRun::default();
    let mut final_args = modified_args;
    let mut selected_runner_type: Option<RunnerType> = None;

    let mut config = Config::load().unwrap_or_default();

    if !wrapper {
        // Collect runner candidates
        let proton_runners = find_proton_dirs();
        let wine_runners = find_wine_binaries();

        let candidates: Vec<(PathBuf, RunnerType)> = if game_info.game_type == GameType::StarRail {
            wine_runners
                .iter()
                .map(|r| (r.path.clone(), r.runner_type.clone()))
                .collect()
        } else {
            let mut all_candidates = proton_runners
                .iter()
                .map(|r| (r.path.clone(), r.runner_type.clone()))
                .collect::<Vec<_>>();
            all_candidates.extend(
                wine_runners
                    .iter()
                    .map(|r| (r.path.clone(), r.runner_type.clone())),
            );
            all_candidates
        };

        let mut selected: Option<PathBuf> = config
            .saved_runner_for_exe(&game_info.game_exe)
            .and_then(|s| {
                let p = PathBuf::from(&s);
                if candidates.iter().any(|(c, _)| c == &p) {
                    Some(p)
                } else {
                    None
                }
            });

        if selected.is_none() && !candidates.is_empty() {
            let options: Vec<String> = candidates
                .iter()
                .map(|(p, _)| p.to_string_lossy().to_string())
                .collect();
            let options_refs: Vec<&str> = options.iter().map(|s| s.as_str()).collect();
            let choice = select_with_arrows("Select a wine/proton runner to use", &options_refs);
            let idx = choice.unwrap_or(0);
            if let Some((path, runner_type)) = candidates.get(idx) {
                selected = Some(path.clone());
                selected_runner_type = Some(runner_type.clone());
                config.set_runner_for_exe(&game_info.game_exe, path.to_string_lossy().to_string());
                if let Err(e) = config.save() {
                    tracing::warn!("Failed to save config: {}", e);
                }
            }
        } else if let Some(ref sel) = selected {
            // Find the runner type for the selected runner
            if let Some((_, runner_type)) = candidates.iter().find(|(p, _)| p == sel) {
                selected_runner_type = Some(runner_type.clone());
            }
        }
        if let Some(runner_dir) = &selected {
            // Only use umu-run if we have a Proton runner
            if wrapper && selected_runner_type == Some(RunnerType::Proton) {
                if !umu_run.is_installed() {
                    tracing::info!("umu-run not found. Installing...");
                    umu_run
                        .install()
                        .await
                        .expect("Failed to download umu-run binary");
                    tracing::info!("umu-run installed successfully.");
                }
                final_args.insert(0, umu_run.binary.clone());
                tracing::info!(
                    "Using umu-run with Proton to execute command: {}",
                    final_args.join(" ")
                );
                unsafe {
                    std::env::set_var("PROTONPATH", runner_dir.to_string_lossy().to_string());
                }
            } else {
                tracing::info!(
                    "Using Wine/direct execution for command: {}",
                    final_args.join(" ")
                );
            }

            // Determine and set WINEPREFIX: prefer configured per-game wineprefix,
            // otherwise use default at ~/.local/share/anime-games-proxy/prefix/<exe_name_folder>
            if !game_info.game_exe.is_empty() {
                let exe_file_name = std::path::Path::new(&game_info.game_exe)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string());

                if let Some(exe_name) = exe_file_name {
                    // strip .exe extension for folder name if present
                    let exe_folder = if exe_name.to_lowercase().ends_with(".exe") {
                        exe_name[..exe_name.len() - 4].to_string()
                    } else {
                        exe_name.clone()
                    };

                    // Try to find matching game entry: exact name, then name without .exe
                    let mut chosen_prefix: Option<String> = None;
                    if let Some(game) = config.games.iter().find(|g| g.name == exe_name)
                        && !game.wineprefix.trim().is_empty()
                    {
                        chosen_prefix = Some(game.wineprefix.clone());
                    }

                    if chosen_prefix.is_none()
                        && let Some(game) = config.games.iter().find(|g| {
                            let nm = g.name.clone();
                            let nm_no_ext = if nm.to_lowercase().ends_with(".exe") {
                                nm[..nm.len() - 4].to_string()
                            } else {
                                nm
                            };
                            nm_no_ext == exe_folder
                        })
                        && !game.wineprefix.trim().is_empty()
                    {
                        chosen_prefix = Some(game.wineprefix.clone());
                    }

                    if chosen_prefix.is_none()
                        && let Some(home) = dirs::home_dir()
                    {
                        let default_prefix = home
                            .join(".local/share/anime-games-proxy/prefix")
                            .join(&exe_folder)
                            .to_string_lossy()
                            .to_string();
                        chosen_prefix = Some(default_prefix);
                    }
                    if let Some(prefix) = chosen_prefix {
                        unsafe {
                            std::env::set_var("WINEPREFIX", prefix.clone());
                        }
                        tracing::info!("Using WINEPREFIX: {}", prefix);
                    }
                }
            }
        } else {
            tracing::info!("No wine/proton runner selected. Proceeding without runner.");
        }
    }

    let mut genshin_patcher: Option<GenshinPatcher> = None;
    if game_info.game_type == GameType::Genshin && !game_info.game_exe.is_empty() {
        match GenshinPatcher::new(&game_info.game_exe) {
            Ok(mut patcher) => match patcher.check_and_patch().await {
                Ok(patched) => {
                    if patched {
                        tracing::info!("Genshin Impact patched successfully");
                    }
                    genshin_patcher = Some(patcher);
                }
                Err(e) => {
                    tracing::warn!("Failed to patch Genshin Impact: {}", e);
                    tracing::warn!("Continuing without patch...\n");
                }
            },
            Err(e) => {
                tracing::warn!("Failed to initialize Genshin Impact patcher: {}", e);
                tracing::warn!("Continuing without patch...\n");
            }
        }
    }
    if game_info.game_type == GameType::StarRail {
        tracing::info!("StarRail detected. Ensure injector is set up correctly.");
        let patch_file = starrail::get_patch_file_path();
        if !patch_file.exists() {
            tracing::warn!("StarRail injector not found at: {}", patch_file.display());
            tracing::info!("Downloading necessary files...");
            match starrail::download_latest_patch().await {
                Ok(path) => {
                    tracing::info!("StarRail injector downloaded to: {}", path.display());
                }
                Err(e) => {
                    tracing::error!("Failed to download StarRail injector: {}", e);
                }
            }
        }
    }

    tracing::info!("executable: {}", game_info.game_exe);

    tracing::info!("Executing command: {}", final_args.join(" "));

    let mut child = TokioCommand::new(&final_args[0])
        .args(&final_args[1..])
        .envs(vec![
            ("http_proxy", proxy.clone()),
            ("https_proxy", proxy.clone()),
        ])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("Failed to execute command");

    let child_pid = child.id().unwrap_or(0);
    tracing::info!("Started process ({})", child_pid);

    // Wait for either the child to exit or Ctrl-C. On Ctrl-C, forward SIGINT to child.
    //
    // Rationale: previously the wrapper would block waiting for the child and then
    // enter a long polling loop for the real game process (StarRail.exe). That
    // made the parent ignore Ctrl-C. We now listen for `tokio::signal::ctrl_c()`
    // and call `libc::kill(pid, SIGINT)` to forward the interrupt to the child
    // process so it can exit cleanly. If the child doesn't exit within a short
    // timeout we return with exit code 130 (typical for interrupted processes).
    let exit_code: i32 = tokio::select! {
        status = child.wait() => {
            let status = status.expect("Failed to wait for command");
            let code = status.code().unwrap_or(1);
            tracing::info!("Command exited with status: {}", code);
            code
        }
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Received Ctrl-C: forwarding SIGINT to child (pid={})", child_pid);
            if child_pid != 0 {
                unsafe { libc::kill(child_pid as i32, libc::SIGINT); }
            }
            // Wait a short while for the child to exit cleanly, then return.
            let _ = tokio::time::timeout(std::time::Duration::from_secs(5), child.wait()).await;
            130
        }
    };

    if game_info.game_type == GameType::StarRail {
        let game_process_name = "StarRail.exe";

        tracing::info!("Waiting for game process '{}'...", game_process_name);

        let mut game_started = false;
        for _ in 0..60 {
            let output = TokioCommand::new("pgrep")
                .arg(game_process_name)
                .output()
                .await
                .expect("Failed to run pgrep");
            if !output.stdout.is_empty() {
                game_started = true;
                break;
            }
            tokio::select! {
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(5)) => {},
                _ = tokio::signal::ctrl_c() => {
                    tracing::info!("Received Ctrl-C while waiting for game to start");
                    return Ok(exit_code);
                }
            }
        }

        if game_started {
            tracing::info!(
                "Game process '{}' started. Monitoring for exit...",
                game_process_name
            );
            loop {
                let output = TokioCommand::new("pgrep")
                    .arg(game_process_name)
                    .output()
                    .await
                    .expect("Failed to run pgrep");
                if output.stdout.is_empty() {
                    tracing::info!("Game process '{}' exited.", game_process_name);
                    break;
                }
                tokio::select! {
                    _ = tokio::time::sleep(tokio::time::Duration::from_secs(5)) => {},
                    _ = tokio::signal::ctrl_c() => {
                        tracing::info!("Received Ctrl-C while monitoring game process");
                        break;
                    }
                }
            }
        } else {
            tracing::error!(
                "Game process '{}' did not start within timeout.",
                game_process_name
            );
        }
    }

    if let Some(mut patcher) = genshin_patcher
        && patcher.is_patched()
        && let Err(e) = patcher.unpatch()
    {
        tracing::error!("Failed to unpatch Genshin Impact: {}", e);
    }

    Ok(exit_code)
}
