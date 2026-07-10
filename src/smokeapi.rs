use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub enum GameType {
    Proton64,
    Proton32,
    Native,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct GameDetection {
    pub game_type: GameType,
    pub api_path: Option<PathBuf>,
    pub backup_path: Option<PathBuf>,
    pub config_exists: bool,
    pub is_smokeapi_proxy: bool,
}

impl Default for GameDetection {
    fn default() -> Self {
        Self {
            game_type: GameType::Unknown,
            api_path: None,
            backup_path: None,
            config_exists: false,
            is_smokeapi_proxy: false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SmokeApiConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logging: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_app_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub override_dlc_status: Option<BTreeMap<String, String>>,
}

pub fn generate_config(unlocked_dlcs: &[u64]) -> SmokeApiConfig {
    let override_dlc: Option<BTreeMap<String, String>> = if unlocked_dlcs.is_empty() {
        None
    } else {
        Some(
            unlocked_dlcs
                .iter()
                .map(|&id| (id.to_string(), "unlocked".to_string()))
                .collect(),
        )
    };

    SmokeApiConfig {
        logging: Some(false),
        default_app_status: Some("original".to_string()),
        override_dlc_status: override_dlc,
    }
}

pub fn read_existing_config(config_dir: &Path) -> Option<SmokeApiConfig> {
    let config_path = config_dir.join("SmokeAPI.config.json");
    let content = std::fs::read_to_string(&config_path).ok()?;
    serde_json::from_str(&content).ok()
}

fn scan_for_file(game_dir: &Path, name: &str) -> Option<PathBuf> {
    walkdir::WalkDir::new(game_dir)
        .max_depth(5)
        .into_iter()
        .filter_map(|e| e.ok())
        .find(|e| e.file_name().to_string_lossy() == name)
        .map(|e| e.path().to_path_buf())
}

fn file_size(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

fn files_match_size(a: &Path, b: &Path) -> bool {
    let sa = file_size(a);
    let sb = file_size(b);
    sa > 0 && sa == sb
}

pub fn detect_game_type(game_dir: &Path) -> GameDetection {
    let cache = crate::setup::cache_dir();
    let dll64 = scan_for_file(game_dir, "steam_api64.dll");
    let dll32 = scan_for_file(game_dir, "steam_api.dll");
    let so = scan_for_file(game_dir, "libsteam_api.so");

    let check = |api_path: &Path, backup_name: &str, smokeapi_file: &str| -> GameDetection {
        let parent = api_path.parent().unwrap_or(Path::new("."));
        let backup = Some(parent.join(backup_name));
        let config_exists = parent.join("SmokeAPI.config.json").exists();
        let is_proxy = files_match_size(api_path, &cache.join(smokeapi_file));
        GameDetection {
            api_path: Some(api_path.to_path_buf()),
            backup_path: backup,
            config_exists,
            is_smokeapi_proxy: is_proxy,
            ..Default::default()
        }
    };

    if let Some(ref path) = dll64 {
        return GameDetection { game_type: GameType::Proton64, ..check(path, "steam_api64_o.dll", "smoke_api64.dll") };
    }
    if let Some(ref path) = dll32 {
        return GameDetection { game_type: GameType::Proton32, ..check(path, "steam_api_o.dll", "smoke_api32.dll") };
    }
    if let Some(ref path) = so {
        return GameDetection { game_type: GameType::Native, ..check(path, "libsteam_api_o.so", "libsmoke_api64.so") };
    }
    GameDetection::default()
}

fn file_names(game_type: &GameType) -> Result<(&str, &str, &str), String> {
    match game_type {
        GameType::Proton64 => Ok(("steam_api64.dll", "steam_api64_o.dll", "smoke_api64.dll")),
        GameType::Proton32 => Ok(("steam_api.dll", "steam_api_o.dll", "smoke_api32.dll")),
        GameType::Native => Ok(("libsteam_api.so", "libsteam_api_o.so", "libsmoke_api64.so")),
        GameType::Unknown => Err("Unknown game type".to_string()),
    }
}

fn hook_dll_name(game_type: &GameType) -> Result<&str, String> {
    match game_type {
        GameType::Proton64 | GameType::Proton32 => Ok("version.dll"),
        GameType::Native => Err("Hook mode not supported for native Linux games".to_string()),
        GameType::Unknown => Err("Unknown game type".to_string()),
    }
}

fn smokeapi_cache_name(game_type: &GameType) -> Result<&str, String> {
    match game_type {
        GameType::Proton64 => Ok("smoke_api64.dll"),
        GameType::Proton32 => Ok("smoke_api32.dll"),
        GameType::Native => Ok("libsmoke_api64.so"),
        GameType::Unknown => Err("Unknown game type".to_string()),
    }
}

pub fn install_hook(
    steam_api_path: &Path,
    game_type: &GameType,
    unlocked_dlcs: &[u64],
    cache_dir: &Path,
) -> Result<(), String> {
    let hook_name = hook_dll_name(game_type)?;
    let smokeapi_name = smokeapi_cache_name(game_type)?;
    let config_dir = steam_api_path.parent().unwrap_or(Path::new("."));

    let config = generate_config(unlocked_dlcs);
    let json = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    std::fs::write(config_dir.join("SmokeAPI.config.json"), json).map_err(|e| e.to_string())?;

    let source = cache_dir.join(smokeapi_name);
    if !source.exists() {
        return Err(format!("{} not found in cache. Re-download SmokeAPI.", smokeapi_name));
    }
    std::fs::copy(&source, config_dir.join(hook_name))
        .map_err(|e| format!("Failed to copy {}: {}", smokeapi_name, e))?;

    Ok(())
}

pub fn install_proxy(
    steam_api_path: &Path,
    game_type: &GameType,
    unlocked_dlcs: &[u64],
    cache_dir: &Path,
) -> Result<(), String> {
    let (original_name, backup_name, smokeapi_name) = file_names(game_type)?;
    let config_dir = steam_api_path.parent().unwrap_or(Path::new("."));

    let config = generate_config(unlocked_dlcs);
    let json = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    std::fs::write(config_dir.join("SmokeAPI.config.json"), json).map_err(|e| e.to_string())?;

    let backup_path = config_dir.join(backup_name);
    if !backup_path.exists() {
        std::fs::rename(steam_api_path, &backup_path)
            .map_err(|e| format!("Failed to backup {}: {}", original_name, e))?;
    }

    let source = cache_dir.join(smokeapi_name);
    if !source.exists() {
        return Err(format!("{} not found in cache. Re-download SmokeAPI.", smokeapi_name));
    }
    std::fs::copy(&source, config_dir.join(original_name))
        .map_err(|e| format!("Failed to copy {}: {}", smokeapi_name, e))?;

    Ok(())
}

pub fn remove_proxy(
    steam_api_path: &Path,
    game_type: &GameType,
) -> Result<(), String> {
    let (target_name, backup_name, smokeapi_name) = file_names(game_type)?;
    let dir = steam_api_path.parent().unwrap_or(Path::new("."));
    let target = dir.join(target_name);
    let backup = dir.join(backup_name);
    let cache = crate::setup::cache_dir();

    if backup.exists() {
        std::fs::remove_file(&target)
            .map_err(|e| format!("Failed to remove proxy: {e}"))?;
        std::fs::rename(&backup, &target)
            .map_err(|e| format!("Failed to restore original: {e}"))?;
    } else {
        // No backup — remove proxy DLL if it matches SmokeAPI binary
        let smokeapi_src = cache.join(smokeapi_name);
        if target.exists() && files_match_size(&target, &smokeapi_src) {
            std::fs::remove_file(&target)
                .map_err(|e| format!("Failed to remove SmokeAPI DLL: {e}"))?;
        }

        // Clean hook-mode files (version.dll, etc.)
        for hook in &["version.dll", "winhttp.dll", "winmm.dll"] {
            let hook_path = dir.join(hook);
            if hook_path.exists() && files_match_size(&hook_path, &smokeapi_src) {
                let _ = std::fs::remove_file(&hook_path);
            }
        }
    }

    let config_path = dir.join("SmokeAPI.config.json");
    if config_path.exists() {
        let _ = std::fs::remove_file(&config_path);
    }

    Ok(())
}
