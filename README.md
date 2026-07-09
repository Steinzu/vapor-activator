# Vapor Activator

Selective Steam DLC manager using [SmokeAPI](https://github.com/acidicoala/SmokeAPI). Pick which DLCs to unlock per game instead of all-or-nothing.

![](screenshot.png)

## Features

- Detects all installed Steam games (Flatpak and native paths)
- Auto-detects Proton vs native Linux games
- Fetches DLC lists from Steam Store + hidden DLCs from SmokeAPI community database
- Installs SmokeAPI in proxy mode — no launch args needed
- Reads existing SmokeAPI configs (handles prior manual installs)
- Clean removal with original file restoration
- Configurable Steam library location

## Requirements

- Rust toolchain (build from source)
- Linux with Steam installed

## Build & Run

```bash
cargo build --release
./target/release/vapor-activator
```

## Usage

1. **Set Steam library folder** — the folder containing `steamapps/` (auto-detected; use *Change...* if needed). Not the game install folder, not the Steam client binary.
2. **Download SmokeAPI** — click *Download* in the left panel. Fetches the latest release from GitHub and caches it at:
   - Linux: `~/.cache/vapor-activator/smokeapi/`
   - Windows: `%LOCALAPPDATA%\vapor-activator\smokeapi\`
3. **Select a game** — lists all installed games from your Steam libraries
4. **Check DLCs you want unlocked** — unchecked DLCs stay at their real ownership status
5. **Apply & Install** — writes `SmokeAPI.config.json` and sets up proxy mode
6. Launch the game normally through Steam

### Removing

Select the game and click *Remove SmokeAPI*. Restores the original Steam API file.

## How it works

**Proxy mode**: renames the original `steam_api64.dll` (or `libsteam_api.so`) to `*_o.*` and places SmokeAPI in its place. SmokeAPI intercepts Steam API calls and reports DLCs as owned/unowned based on your config. No Steam launch options needed.

## License

MIT

---

*Built with the help of [OpenCode](https://opencode.ai)*
