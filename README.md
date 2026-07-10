# Vapor Activator

Selective Steam DLC manager using [SmokeAPI](https://github.com/acidicoala/SmokeAPI).  
Pick exactly which DLCs to unlock per game — no all-or-nothing.

## Features

- Detects installed Steam games from Flatpak, native Linux, and Windows paths
- Auto-detects Proton (32/64-bit) vs native Linux games
- Fetches DLC lists from Steam Store API + hidden DLCs from SmokeAPI community database
- **Proxy mode** (default): replaces `steam_api*.dll` / `libsteam_api.so` — no launch args needed
- **Hook mode**: injects via `version.dll` / `winhttp.dll` / `winmm.dll` / `dinput8.dll` / `d3d11.dll` / `dxgi.dll` for games that integrity-check the Steam API DLL
- Reads existing SmokeAPI configs from prior manual or tool-based installs
- Clean removal with original file restoration and hook cleanup
- Auto-downloads the latest SmokeAPI release from GitHub
- Configurable Steam library path, persisted across sessions

## Requirements

- [Rust toolchain](https://rustup.rs)
- Linux or Windows with Steam installed

## Build

```bash
cargo build --release
./target/release/vapor-activator      # Linux
target\release\vapor-activator.exe    # Windows
```

## Usage

1. **Set Steam library folder** — the folder containing `steamapps/` (auto-detected on first run, change with *Change...* if needed)
2. **Download SmokeAPI** — click *Download* in the left panel, cached automatically at:

   | Platform | Cache path |
   |----------|-----------|
   | Linux    | `~/.cache/vapor-activator/smokeapi/` |
   | Windows  | `%LOCALAPPDATA%\vapor-activator\smokeapi\` |

3. **Select a game** from the list
4. **Check the DLCs** you want unlocked — unchecked DLCs stay at their real ownership status
5. **Apply & Install** — by default uses proxy mode (renames original, copies SmokeAPI in its place)
6. Launch the game normally through Steam — no launch options required

### Hook mode

For games that verify their `steam_api*.dll` integrity and reject proxy mode, check *Hook mode* and select the DLL to hijack. The original Steam API file is left untouched; SmokeAPI is injected through a different DLL the game loads. Start with `version.dll` — it's the safest and most reliable.

### Removing

Select the game and click *Remove SmokeAPI*. Restores the original file and cleans up hook DLLs.

## Config

Steam library path is saved to:

| Platform | Config path |
|----------|------------|
| Linux    | `~/.config/vapor-activator/config.json` |
| Windows  | `%APPDATA%\vapor-activator\config.json` |

## How it works

SmokeAPI intercepts Steamworks SDK calls (`BIsSubscribedApp`, `GetDLCCount`, etc.) and reports DLCs as owned/unowned based on your `SmokeAPI.config.json`. Vapor Activator generates that config and handles the DLL installation. No changes to Steam launch options, no modifying game files beyond the Steam API DLL.

## License

MIT

---

*Built with the help of [OpenCode](https://opencode.ai)*
