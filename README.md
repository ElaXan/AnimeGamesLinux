# Anime Games Linux

Run your favorite anime games on a private server with this transparent proxy wrapper. No complicated setup, no messing with hosts files—just point, configure, and play.

## What does it do?

This tool sits between your game and the internet, redirecting traffic from the official server to the private server. It handles patches automatically and works seamlessly with Wine/Proton on Linux.

## Quick Start

### Get the tool

1. Grab the latest release from the [release page](https://github.com/ElaXan/AnimeGamesLinux/releases)
2. Make it executable:
```bash
chmod +x anime-games-linux
```
3. Save it in a safe place like `~/bin/` or `~/.local/bin/`

### Using with Heroic (easiest method)

Heroic Games Launcher explains this very clearly:

**1. Add your game to Heroic**
- If it isn't already there, add it and navigate to the game's executable file.

**2. Configure the wrapper**
- Right-click the game → Settings
- Go to the Advanced tab
- In the Wrapper field, enter the full path to `anime-games-linux`
- Like: `/home/yourname/bin/anime-games-linux`
- **Add `-w` to the wrapper field** (this prevents annoying prompts)

**3. Set your server info**
- Go to the Environment tab in the same settings
- Add the following variables:
```
SERVER=ps.yuuki.me
SERVER_PORT=443
USE_SSL=1
```
- Change these variables to match your private server if you're using a different server.

**4. Press play**

Done. The wrapper will take care of the rest.

## Configuration

### Environment Variables

Set these in the Heroic Environment tab or export them to your shell:

| Variable | Function | Default |
|----------|--------------|---------|
| `SERVER` | Private server address | `127.0.0.1` |
| `SERVER_PORT` | Server port | `80` |
| `USE_SSL` | Use HTTPS (set to 1 to enable) | disabled |
| `PROXY_PORT` | Local proxy port | `8080` |
| `WINEPREFIX` | Custom Wine prefix path | (auto-detected) |

### Command Line

If you run it manually:

```bash
anime-games-linux [OPTIONS] -- <COMMAND>
```

**Options:**
- `-w, --wrapper` - Wrapper mode (skip Wine selection prompt)
- `--wineprefix <PATH>` - Custom Wine prefix
- `--server <ADDRESS>` - Server address
- `--server-port <PORT>` - Server port
- `--use-ssl` - Enable SSL
- `--proxy-port <PORT>` - Local proxy port

**Example:**
```bash
anime-games-linux --server my-server.com --use-ssl game.exe
```

## Configuration file location

The settings are located in `~/.config/anime-games-proxy/config.json`. The tool manages these settings automatically, but you can edit them manually if you want to customize them.

## Building from source

**Requirements:**
- Rust 1.70 or later
- Cargo
- OpenSSL development library (`libssl-dev` on Ubuntu/Debian)

**Build:**
```bash
cargo build --release
```

The binary ends in `target/release/anime-games-linux`

## License

MIT License - see LICENSE.txt