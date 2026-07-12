use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub enum GameType {
    Proton64,
    Proton32,
    Native,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Arch {
    X64,
    X86,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct GameDetection {
    pub game_type: GameType,
    pub api_path: Option<PathBuf>,
    pub backup_path: Option<PathBuf>,
    pub config_exists: bool,
    pub is_smokeapi_proxy: bool,
    pub arch: Arch,
}

impl Default for GameDetection {
    fn default() -> Self {
        Self {
            game_type: GameType::Unknown,
            api_path: None,
            backup_path: None,
            config_exists: false,
            is_smokeapi_proxy: false,
            arch: Arch::Unknown,
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
        .max_depth(10)
        .into_iter()
        .filter_map(|e| e.ok())
        .find(|e| e.file_name().to_string_lossy() == name)
        .map(|e| e.path().to_path_buf())
}

fn file_md5(path: &Path) -> String {
    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return String::new(),
    };
    let mut buf = Vec::new();
    if file.read_to_end(&mut buf).is_err() {
        return String::new();
    }
    let digest = md5::compute(&buf);
    format!("{:x}", digest)
}

fn files_match(a: &Path, b: &Path) -> bool {
    let ha = file_md5(a);
    let hb = file_md5(b);
    !ha.is_empty() && ha == hb
}

fn detect_exe_arch(game_dir: &Path) -> Arch {
    walkdir::WalkDir::new(game_dir)
        .max_depth(5)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .to_lowercase()
                .ends_with(".exe")
        })
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_lowercase();
            !name.contains("setup")
                && !name.contains("install")
                && !name.contains("redist")
                && !name.contains("crash")
                && !name.starts_with("unins")
        })
        .find_map(|e| {
            let mut f = std::fs::File::open(e.path()).ok()?;
            let mut header = [0u8; 64];
            f.read_exact(&mut header).ok()?;
            let pe_offset = u32::from_le_bytes([header[0x3C], header[0x3D], header[0x3E], header[0x3F]]) as u64;
            f.seek(std::io::SeekFrom::Start(pe_offset + 4)).ok()?;
            let mut machine = [0u8; 2];
            f.read_exact(&mut machine).ok()?;
            match u16::from_le_bytes(machine) {
                0x014C => Some(Arch::X86),
                0x8664 => Some(Arch::X64),
                _ => None,
            }
        })
        .unwrap_or(Arch::Unknown)
}

pub fn detect_game_type(game_dir: &Path) -> GameDetection {
    let cache = crate::setup::cache_dir();
    let dll64 = scan_for_file(game_dir, "steam_api64.dll");
    let dll32 = scan_for_file(game_dir, "steam_api.dll");
    let so = scan_for_file(game_dir, "libsteam_api.so");
    let arch = detect_exe_arch(game_dir);

    let check = |api_path: &Path, backup_name: &str, smokeapi_file: &str| -> GameDetection {
        let parent = api_path.parent().unwrap_or(Path::new("."));
        let backup = Some(parent.join(backup_name));
        let config_exists = parent.join("SmokeAPI.config.json").exists();
        let is_proxy = files_match(api_path, &cache.join(smokeapi_file));
        GameDetection {
            api_path: Some(api_path.to_path_buf()),
            backup_path: backup,
            config_exists,
            is_smokeapi_proxy: is_proxy,
            arch,
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
    GameDetection { arch, ..Default::default() }
}

fn smokeapi_cache_name(arch: Arch, game_type: &GameType) -> Result<&'static str, String> {
    match game_type {
        GameType::Native => Ok("libsmoke_api64.so"),
        GameType::Proton64 | GameType::Proton32 => match arch {
            Arch::X86 => Ok("smoke_api32.dll"),
            Arch::X64 | Arch::Unknown => Ok("smoke_api64.dll"),
        },
        GameType::Unknown => Err("Unknown game type".to_string()),
    }
}

fn file_names(game_type: &GameType) -> Result<(&str, &str, &str), String> {
    match game_type {
        GameType::Proton64 => Ok(("steam_api64.dll", "steam_api64_o.dll", "smoke_api64.dll")),
        GameType::Proton32 => Ok(("steam_api.dll", "steam_api_o.dll", "smoke_api32.dll")),
        GameType::Native => Ok(("libsteam_api.so", "libsteam_api_o.so", "libsmoke_api64.so")),
        GameType::Unknown => Err("Unknown game type".to_string()),
    }
}

fn koaloader_config_content(smokeapi_path: &str) -> String {
    format!(
        r#"{{"logging":false,"auto_load":true,"modules":[{{"path":"{}","required":true}}]}}"#,
        smokeapi_path
    )
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
        return Err(format!("{} not found in cache", smokeapi_name));
    }
    std::fs::copy(&source, config_dir.join(original_name))
        .map_err(|e| format!("Failed to copy {}: {}", smokeapi_name, e))?;

    Ok(())
}

pub fn install_hook(
    steam_api_path: &Path,
    arch: Arch,
    game_type: &GameType,
    hook_dll: &str,
    unlocked_dlcs: &[u64],
    cache_dir: &Path,
) -> Result<(), String> {
    if *game_type == GameType::Native {
        return Err("Hook mode not supported for native Linux games".to_string());
    }
    let smokeapi_name = smokeapi_cache_name(arch, game_type)?;
    let config_dir = steam_api_path.parent().unwrap_or(Path::new("."));

    let config = generate_config(unlocked_dlcs);
    let json = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    std::fs::write(config_dir.join("SmokeAPI.config.json"), json).map_err(|e| e.to_string())?;

    let source = cache_dir.join(smokeapi_name);
    if !source.exists() {
        return Err(format!("{} not found in cache", smokeapi_name));
    }
    std::fs::copy(&source, config_dir.join(hook_dll))
        .map_err(|e| format!("Failed to copy {}: {}", smokeapi_name, e))?;

    Ok(())
}

pub fn install_koaloader(
    steam_api_path: &Path,
    arch: Arch,
    game_type: &GameType,
    proxy_dll: &str,
    unlocked_dlcs: &[u64],
    cache_dir: &Path,
    koaloader_dir: &Path,
) -> Result<(), String> {
    if *game_type == GameType::Native {
        return Err("Koaloader not supported for native Linux games".to_string());
    }
    let smokeapi_name = smokeapi_cache_name(arch, game_type)?;
    let config_dir = steam_api_path.parent().unwrap_or(Path::new("."));

    // Write SmokeAPI config
    let config = generate_config(unlocked_dlcs);
    let json = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    std::fs::write(config_dir.join("SmokeAPI.config.json"), json).map_err(|e| e.to_string())?;

    // Copy SmokeAPI DLL (unrenamed, loaded by Koaloader)
    let smokeapi_src = cache_dir.join(smokeapi_name);
    if !smokeapi_src.exists() {
        return Err(format!("{} not found in cache", smokeapi_name));
    }
    std::fs::copy(&smokeapi_src, config_dir.join(smokeapi_name))
        .map_err(|e| format!("Failed to copy {}: {}", smokeapi_name, e))?;

    // Copy Koaloader proxy DLL (cache may have name64.dll / name32.dll from zip subdirs)
    let stem = proxy_dll.strip_suffix(".dll").unwrap_or(proxy_dll);
    let cache_proxy = match arch {
        Arch::X64 => {
            let named = koaloader_dir.join(format!("{}64.dll", stem));
            if named.exists() { named } else { koaloader_dir.join(proxy_dll) }
        }
        Arch::X86 => {
            let named = koaloader_dir.join(format!("{}32.dll", stem));
            if named.exists() { named } else { koaloader_dir.join(proxy_dll) }
        }
        Arch::Unknown => koaloader_dir.join(proxy_dll),
    };
    if !cache_proxy.exists() {
        return Err(format!("Koaloader proxy {} not found. Re-download Koaloader.", cache_proxy.display()));
    }
    std::fs::copy(&cache_proxy, config_dir.join(proxy_dll))
        .map_err(|e| format!("Failed to copy Koaloader {}: {}", proxy_dll, e))?;

    // Write Koaloader config
    std::fs::write(
        config_dir.join("Koaloader.config.json"),
        koaloader_config_content(smokeapi_name),
    )
    .map_err(|e| e.to_string())?;

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
        let smokeapi_src = cache.join(smokeapi_name);
        if target.exists() && files_match(&target, &smokeapi_src) {
            std::fs::remove_file(&target)
                .map_err(|e| format!("Failed to remove SmokeAPI DLL: {e}"))?;
        }

        for hook in &["version.dll", "winhttp.dll", "winmm.dll", "dinput8.dll", "d3d11.dll", "dxgi.dll"] {
            let hook_path = dir.join(hook);
            if hook_path.exists() && files_match(&hook_path, &smokeapi_src) {
                let _ = std::fs::remove_file(&hook_path);
            }
        }

        // Clean SmokeAPI DLLs copied by Koaloader install
        for name in &["smoke_api32.dll", "smoke_api64.dll", "libsmoke_api64.so"] {
            let p = dir.join(name);
            if p.exists() && files_match(&p, &cache.join(name)) {
                let _ = std::fs::remove_file(&p);
            }
        }

        // Clean Koaloader config
        let koaloader_config = dir.join("Koaloader.config.json");
        if koaloader_config.exists() {
            let _ = std::fs::remove_file(&koaloader_config);
        }
    }

    let config_path = dir.join("SmokeAPI.config.json");
    if config_path.exists() {
        let _ = std::fs::remove_file(&config_path);
    }

    Ok(())
}
