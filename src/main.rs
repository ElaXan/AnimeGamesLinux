mod config;
mod game;
mod get_wine;
mod proxy;
mod run;
mod umu_run;
mod utils;

use clap::Parser;
use proxy::{create_proxy, set_proxy_addr};

#[derive(clap::Parser, Debug)]
#[command(name = "anime-games-ps-linux", version)]
struct Cli {
    /// Run as wrapper only
    #[arg(short, long)]
    wrapper: bool,

    /// Set WINEPREFIX or you can set it via environment variable WINEPREFIX
    #[arg(long)]
    wineprefix: Option<String>,

    #[arg(long, help = "Set server address (overrides config)")]
    server: Option<String>,

    #[arg(long, help = "Set server port (overrides config)")]
    server_port: Option<String>,

    #[arg(long, help = "Use SSL for server connection (overrides config)")]
    use_ssl: bool,

    #[arg(long, help = "Set proxy port (overrides config)")]
    proxy_port: Option<String>,

    /// Command to execute. Example: `/path/to/game.exe`
    #[arg(required = true, trailing_var_arg = true)]
    command: Vec<String>,
}

use crate::utils::{detect_game, modify_command_for_game};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_max_level(
            #[cfg(debug_assertions)]
            tracing::Level::DEBUG,
            #[cfg(not(debug_assertions))]
            tracing::Level::INFO,
        )
        .without_time()
        .init();

    let cli = Cli::parse();

    let args: Vec<String> = cli.command;

    println!("Anime Games PS Linux Wrapper");
    println!("========================\n");

    let config = config::Config::load().unwrap_or_default();

    let proxy_port: String = std::env::var("PROXY_PORT").unwrap_or_else(|_| {
        cli.proxy_port
            .clone()
            .or_else(|| Some(config.proxy_port.clone()))
            .unwrap_or_else(|| "8080".to_string())
    });

    let game_info = detect_game(&args);

    let (cfg_server, cfg_server_port, cfg_use_ssl) = config
        .server_for_exe(&game_info.game_exe)
        .or_else(|| {
            config
                .games
                .first()
                .map(|g| (g.server.clone(), g.server_port, g.use_ssl))
        })
        .unwrap_or(("127.0.0.1".to_string(), 80, false));

    let server: String = std::env::var("SERVER")
        .ok()
        .or_else(|| cli.server.clone())
        .unwrap_or(cfg_server);
    let server_port: String = std::env::var("SERVER_PORT")
        .ok()
        .or_else(|| cli.server_port.clone())
        .unwrap_or(cfg_server_port.to_string());
    let use_ssl: bool = std::env::var_os("USE_SSL").is_some() || cli.use_ssl || cfg_use_ssl;

    let server_addr = format!(
        "{}://{}:{}",
        if use_ssl { "https" } else { "http" },
        server,
        server_port
    );

    let modified_args = match modify_command_for_game(&args, &game_info) {
        Ok(args) => args,
        Err(e) => {
            tracing::error!("Error: {}", e);
            std::process::exit(1);
        }
    };

    tracing::info!("Starting proxy on port {}", proxy_port);
    tracing::info!("Server address: {}", server_addr);

    // Set the target server address
    set_proxy_addr(server_addr);

    // Create and start the proxy server
    let proxy_handle = create_proxy(proxy_port.parse().unwrap()).await;

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    tracing::info!("Proxy server is running...");

    let proxy = format!("http://127.0.0.1:{}", proxy_port);
    let exit_code = run::execute_command(modified_args, proxy, &game_info, cli.wrapper)
        .await
        .unwrap_or(1);

    proxy_handle.abort();

    std::process::exit(exit_code);
}
