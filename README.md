# Vapor Activator

Selective Steam DLC manager using [SmokeAPI](https://github.com/acidicoala/SmokeAPI) and [Koaloader](https://github.com/acidicoala/Koaloader).  
Pick exactly which DLCs to unlock per game — no all-or-nothing.

## Features

- Detects installed Steam games from Flatpak, native Linux, and Windows paths
- Auto-detects Proton (32/64-bit) vs native Linux games
- **EXE architecture detection** — reads PE headers to determine 32/64-bit, deploys correct binaries
- **MD5 fingerprinting** — identifies already-installed SmokeAPI/Koaloader DLLs reliably
- **Multi-source DLC discovery** — Steam Store API + SteamCMD depot metadata + community hidden DLCs list
- **Proxy mode** (default): replaces `steam_api*.dll` / `libsteam_api.so` — no launch args needed
- **Hook mode**: injects via `version.dll` / `winhttp.dll` / `winmm.dll` / `dinput8.dll` / `d3d11.dll` / `dxgi.dll` for games that integrity-check the Steam API DLL
- **Koaloader mode**: full DLL sideloading — proxy DLL forwards calls to the real system DLL while loading SmokeAPI as a module. Original `steam_api.dll` is never touched
- Reads existing SmokeAPI configs from prior manual or tool-based installs
- Clean removal with original file restoration, hook cleanup, and Koaloader config removal
- Auto-downloads SmokeAPI and Koaloader from GitHub releases
- Configurable Steam library path, persisted across sessions

## Download

| Platform | Link |
|----------|------|
| Linux    | [vapor-activator](https://github.com/Steinzu/vapor-activator/releases/latest/download/vapor-activator) |
| Windows  | [vapor-activator.exe](https://github.com/Steinzu/vapor-activator/releases/latest/download/vapor-activator.exe) |

> No release yet? Trigger the workflow on [Actions](https://github.com/Steinzu/vapor-activator/actions) — *Release* → *Run workflow* → enter version like `v1.0.0`.

## Build from source

Requires [Rust](https://rustup.rs).

```bash
cargo build --release
./target/release/vapor-activator      # Linux
target\release\vapor-activator.exe    # Windows
```

## Usage

1. **Set Steam library folder** — the folder containing `steamapps/` (auto-detected, change with *Change...*)
2. **Download tools** — click *Get* next to SmokeAPI and Koaloader in the left panel. Cached at:

   | Platform | Cache path |
   |----------|-----------|
   | Linux    | `~/.cache/vapor-activator/smokeapi/` and `.../koaloader/` |
   | Windows  | `%LOCALAPPDATA%\vapor-activator\smokeapi\` and `...\koaloader\` |

3. **Select a game** from the list — shows type, architecture, and install status
4. **Choose method**:
   - **Proxy** (default) — renames original `steam_api*.dll` → `_o.dll`, copies SmokeAPI in its place. Works for most games.
   - **Hook** — leaves original untouched, places SmokeAPI as `version.dll` (or another selected DLL). For games that verify DLL integrity.
   - **Koaloader** — full sideload: the proxy DLL forwards all calls to the real system DLL while loading SmokeAPI silently. The original file is never checked.
5. **Check the DLCs** you want unlocked — unchecked DLCs stay at real ownership
6. **Apply & Install**
7. Launch the game normally through Steam

### Removing

Select the game and click *Remove SmokeAPI*. Restores original files, cleans up hook DLLs, Koaloader configs, and SmokeAPI configs.

## Config

Steam library path saved to:

| Platform | Path |
|----------|------|
| Linux    | `~/.config/vapor-activator/config.json` |
| Windows  | `%APPDATA%\vapor-activator\config.json` |

## How it works

SmokeAPI intercepts Steamworks SDK calls (`BIsSubscribedApp`, `GetDLCCount`, etc.) and reports DLCs as owned/unowned based on `SmokeAPI.config.json`. In Koaloader mode, the proxy DLL loads the real system DLL, forwards every call, and also loads SmokeAPI as a module — the game never sees the real `steam_api.dll` being touched.

## License

MIT

---

*Built with the help of [OpenCode](https://opencode.ai)*
