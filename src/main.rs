#![windows_subsystem = "windows"]

mod dlc;
mod setup;
mod smokeapi;
mod steam;

use eframe::egui;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const CONFIG_DIR: &str = "vapor-activator";
const CONFIG_FILE: &str = "config.json";

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(CONFIG_DIR)
        .join(CONFIG_FILE)
}

fn load_steam_root() -> Option<PathBuf> {
    let path = config_path();
    let content = std::fs::read_to_string(&path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&content).ok()?;
    v.get("steam_root")
        .and_then(|s| s.as_str())
        .map(PathBuf::from)
}

fn save_steam_root(path: &std::path::Path) {
    let cfg_path = config_path();
    let _ = std::fs::create_dir_all(cfg_path.parent().unwrap());
    let v = serde_json::json!({"steam_root": path.to_string_lossy()});
    let _ = std::fs::write(&cfg_path, serde_json::to_string_pretty(&v).unwrap_or_default());
}

#[derive(Clone)]
struct AsyncResult<T> {
    inner: Arc<Mutex<Option<Result<T, String>>>>,
}

impl<T> AsyncResult<T> {
    fn new() -> Self {
        Self { inner: Arc::new(Mutex::new(None)) }
    }
    fn take(&self) -> Option<Result<T, String>> {
        self.inner.lock().unwrap().take()
    }
    fn set(&self, val: Result<T, String>) {
        *self.inner.lock().unwrap() = Some(val);
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Method {
    Proxy,
    Hook,
    Koaloader,
}

impl Method {
    fn label(&self) -> &str {
        match self {
            Method::Proxy => "Proxy",
            Method::Hook => "Hook",
            Method::Koaloader => "Koaloader",
        }
    }
}

const HOOK_DLLS: &[&str] = &[
    "version.dll", "winhttp.dll", "winmm.dll",
    "dinput8.dll", "d3d11.dll", "dxgi.dll",
];

struct App {
    games: Vec<steam::InstalledGame>,
    steam_root: PathBuf,
    selected_idx: Option<usize>,
    selected_game: Option<steam::InstalledGame>,
    detection: smokeapi::GameDetection,
    dlc_list: Vec<dlc::DlcInfo>,
    dlc_result: AsyncResult<(u64, Vec<dlc::DlcInfo>)>,
    dlc_loading: bool,
    unlocked_dlcs: BTreeSet<u64>,
    status_message: String,
    filter: String,
    smokeapi_ready: bool,
    koaloader_ready: bool,
    downloading: Option<&'static str>,
    setup_result: AsyncResult<()>,
    dlc_loading_appid: Option<u64>,
    method: Method,
    hook_dll_index: usize,
}

impl App {
    fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_default();
        let steam_root = load_steam_root().unwrap_or_else(|| {
            let candidates: Vec<PathBuf> = if cfg!(windows) {
                let pf = std::env::var("ProgramFiles(x86)").map(PathBuf::from).unwrap_or_default();
                vec![pf.join("Steam")]
            } else {
                vec![
                    home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
                    home.join(".local/share/Steam"),
                ]
            };
            candidates.iter().find(|p| p.join("steamapps").exists()).cloned().unwrap_or_default()
        });

        let games = steam::discover_games(&steam_root);
        let msg = if games.is_empty() {
            format!("No games found in {}. Use 'Change...' to set the folder containing steamapps/.", steam_root.display())
        } else {
            format!("Found {} games", games.len())
        };

        App {
            games,
            steam_root,
            selected_idx: None,
            selected_game: None,
            detection: smokeapi::GameDetection::default(),
            dlc_list: vec![],
            dlc_result: AsyncResult::new(),
            dlc_loading: false,
            unlocked_dlcs: BTreeSet::new(),
            status_message: msg,
            filter: String::new(),
            smokeapi_ready: setup::is_smokeapi_installed(),
            koaloader_ready: setup::is_koaloader_installed(),
            downloading: None,
            setup_result: AsyncResult::new(),
            dlc_loading_appid: None,
            method: Method::Proxy,
            hook_dll_index: 0,
        }
    }

    fn rescan_games(&mut self) {
        self.games = steam::discover_games(&self.steam_root);
        self.selected_idx = None;
        self.selected_game = None;
        self.dlc_list.clear();
        self.unlocked_dlcs.clear();
        self.detection = smokeapi::GameDetection::default();
        self.status_message = if self.games.is_empty() {
            format!("No games found in {}.", self.steam_root.display())
        } else {
            format!("Found {} games", self.games.len())
        };
    }

    fn select_game(&mut self, idx: usize) {
        self.selected_idx = Some(idx);
        self.selected_game = self.games.get(idx).cloned();
        self.dlc_list.clear();
        self.unlocked_dlcs.clear();
        self.method = Method::Proxy;
        self.detection = smokeapi::detect_game_type(
            &self.selected_game.as_ref().unwrap().game_dir(),
        );
        self.load_existing_config();

        let appid = self.games[idx].appid;
        let result = self.dlc_result.clone();
        self.dlc_loading = true;
        self.dlc_loading_appid = Some(appid);
        self.status_message = "Loading DLCs...".to_string();
        tokio::task::spawn(async move {
            match dlc::fetch_dlc_list(appid).await {
                Ok(dlcs) => result.set(Ok((appid, dlcs))),
                Err(e) => result.set(Err(e)),
            }
        });
    }

    fn load_existing_config(&mut self) {
        if !self.detection.config_exists {
            return;
        }
        if let Some(ref api_path) = self.detection.api_path {
            let dir = api_path.parent().unwrap_or(std::path::Path::new("."));
            if let Some(cfg) = smokeapi::read_existing_config(dir) {
                if let Some(ref overrides) = cfg.override_dlc_status {
                    for (id_str, status) in overrides {
                        if status == "unlocked" {
                            if let Ok(id) = id_str.parse::<u64>() {
                                self.unlocked_dlcs.insert(id);
                            }
                        }
                    }
                }
            }
        }
    }

    fn reanalyze(&mut self) {
        if let Some(ref game) = self.selected_game {
            self.detection = smokeapi::detect_game_type(&game.game_dir());
        }
    }

    fn game_type_label(&self) -> &str {
        match self.detection.game_type {
            smokeapi::GameType::Proton64 => "Proton (64-bit)",
            smokeapi::GameType::Proton32 => "Proton (32-bit)",
            smokeapi::GameType::Native => "Native Linux",
            smokeapi::GameType::Unknown => "Not detected",
        }
    }

    fn arch_label(&self) -> &str {
        match self.detection.arch {
            smokeapi::Arch::X64 => "64-bit",
            smokeapi::Arch::X86 => "32-bit",
            smokeapi::Arch::Unknown => "unknown",
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(result) = self.setup_result.take() {
            self.downloading = None;
            match result {
                Ok(()) => {
                    self.smokeapi_ready = setup::is_smokeapi_installed();
                    self.koaloader_ready = setup::is_koaloader_installed();
                    self.status_message = "Download complete".to_string();
                }
                Err(e) => {
                    self.status_message = format!("Download failed: {}", e);
                }
            }
        }

        if let Some(result) = self.dlc_result.take() {
            self.dlc_loading = false;
            self.dlc_loading_appid = None;
            match result {
                Ok((appid, dlcs)) => {
                    if self.dlc_loading_appid == Some(appid) {
                        self.dlc_list = dlcs;
                        self.status_message = format!("Loaded {} DLCs", self.dlc_list.len());
                    }
                }
                Err(e) => {
                    self.status_message = format!("Error: {}", e);
                }
            }
        }

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Vapor Activator");
                ui.separator();
                ui.label("Selective DLC unlock via SmokeAPI");
            });
        });

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status_message);
                if self.dlc_loading || self.downloading.is_some() {
                    ui.add(egui::Spinner::new());
                }
            });
        });

        egui::SidePanel::left("games_panel")
            .min_width(280.0)
            .show(ctx, |ui| {
                ui.heading("Installed Games");
                ui.horizontal(|ui| {
                    ui.label("Filter:");
                    ui.text_edit_singleline(&mut self.filter);
                });

                ui.separator();

                ui.horizontal(|ui| {
                    ui.label("Steam:");
                    ui.label(egui::RichText::new(self.steam_root.display().to_string()).small());
                    if ui.small_button("Change...").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                            self.steam_root = path;
                            save_steam_root(&self.steam_root);
                            self.rescan_games();
                        }
                    }
                });

                ui.separator();
                ui.label("Downloads:");

                ui.horizontal(|ui| {
                    ui.label("SmokeAPI:");
                    if self.smokeapi_ready {
                        ui.colored_label(egui::Color32::GREEN, "Ready");
                        if ui.small_button("Delete").clicked() {
                            self.delete_smokeapi();
                        }
                    } else if self.downloading == Some("SmokeAPI") {
                        ui.add(egui::Spinner::new());
                    } else {
                        ui.colored_label(egui::Color32::YELLOW, "Missing");
                        if ui.small_button("Get").clicked() {
                            self.download_smokeapi();
                        }
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Koaloader:");
                    if self.koaloader_ready {
                        ui.colored_label(egui::Color32::GREEN, "Ready");
                        if ui.small_button("Delete").clicked() {
                            self.delete_koaloader();
                        }
                    } else if self.downloading == Some("Koaloader") {
                        ui.add(egui::Spinner::new());
                    } else {
                        ui.colored_label(egui::Color32::YELLOW, "Missing");
                        if ui.small_button("Get").clicked() {
                            self.download_koaloader();
                        }
                    }
                });

                ui.separator();

                let filter_lower = self.filter.to_lowercase();
                let mut clicked_idx = None;
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (idx, game) in self.games.iter().enumerate() {
                        if !filter_lower.is_empty()
                            && !game.name.to_lowercase().contains(&filter_lower)
                        {
                            continue;
                        }
                        let selected = self.selected_idx == Some(idx);
                        if ui.selectable_label(selected, &game.name).clicked() {
                            clicked_idx = Some(idx);
                        }
                    }
                });
                if let Some(idx) = clicked_idx {
                    self.select_game(idx);
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(ref gi) = self.selected_game {
                let game_dir = gi.game_dir();
                ui.heading(&gi.name);
                ui.label(format!("AppID: {}  |  Type: {}  |  Arch: {}", gi.appid, self.game_type_label(), self.arch_label()));
                ui.label(format!("Path: {}", game_dir.display()));

                ui.separator();

                let backup_exists = self.detection.backup_path.as_ref()
                    .map(|p| p.exists()).unwrap_or(false);
                let config_exists = self.detection.config_exists;
                let proxy_stale = self.detection.is_smokeapi_proxy && !backup_exists && !config_exists;

                ui.horizontal(|ui| {
                    ui.label("SmokeAPI:");
                    if backup_exists {
                        ui.colored_label(egui::Color32::GREEN, "Installed (proxy)");
                    } else if config_exists {
                        ui.colored_label(egui::Color32::GREEN, "Installed (config)");
                    } else if proxy_stale {
                        ui.colored_label(egui::Color32::RED, "Stale — needs cleanup");
                    } else {
                        ui.colored_label(egui::Color32::YELLOW, "Not installed");
                    }
                });

                if proxy_stale {
                    ui.colored_label(egui::Color32::RED,
                        "SmokeAPI DLL active but config missing. Click Remove to clean up.");
                }

                if let Some(ref api_path) = self.detection.api_path {
                    ui.label(format!("Steam API: {}", api_path.display()));
                } else if self.detection.game_type == smokeapi::GameType::Unknown {
                    ui.colored_label(egui::Color32::RED, "No steam_api file found");
                }

                ui.separator();
                ui.heading("DLC List");

                if self.dlc_loading {
                    ui.add(egui::Spinner::new());
                } else {
                    if ui.button("Refresh").clicked() {
                        if let Some(idx) = self.selected_idx {
                            self.select_game(idx);
                        }
                    }

                    if self.dlc_list.is_empty() {
                        ui.label("No DLCs found.");
                    } else {
                        ui.horizontal(|ui| {
                            if ui.button("Select All").clicked() {
                                for d in &self.dlc_list {
                                    self.unlocked_dlcs.insert(d.appid);
                                }
                            }
                            if ui.button("Deselect All").clicked() {
                                self.unlocked_dlcs.clear();
                            }
                        });

                        let available = ui.available_height();
                        egui::ScrollArea::vertical()
                            .id_salt("dlc_scroll")
                            .max_height((available - 130.0).max(100.0))
                            .show(ui, |ui| {
                                for dlc in &self.dlc_list {
                                    let mut checked = self.unlocked_dlcs.contains(&dlc.appid);
                                    ui.horizontal(|ui| {
                                        if ui.checkbox(&mut checked, "").changed() {
                                            if checked {
                                                self.unlocked_dlcs.insert(dlc.appid);
                                            } else {
                                                self.unlocked_dlcs.remove(&dlc.appid);
                                            }
                                        }
                                        ui.label(format!("{} ({})", dlc.name, dlc.appid));
                                    });
                                }
                            });

                        ui.separator();

                        let can_apply = self.smokeapi_ready
                            && self.detection.api_path.is_some()
                            && self.detection.game_type != smokeapi::GameType::Unknown;
                        let can_remove = backup_exists || config_exists || proxy_stale;
                        let can_method = self.detection.game_type == smokeapi::GameType::Proton64
                            || self.detection.game_type == smokeapi::GameType::Proton32;

                        if can_method {
                            ui.horizontal(|ui| {
                                ui.label("Method:");
                                egui::ComboBox::from_id_salt("method")
                                    .selected_text(self.method.label())
                                    .show_ui(ui, |ui| {
                                        for &m in &[Method::Proxy, Method::Hook, Method::Koaloader] {
                                            if m == Method::Koaloader && !self.koaloader_ready {
                                                continue;
                                            }
                                            ui.selectable_value(&mut self.method, m, m.label());
                                        }
                                    });

                                if self.method != Method::Proxy {
                                    ui.label("as");
                                    egui::ComboBox::from_id_salt("hook_dll")
                                        .selected_text(HOOK_DLLS[self.hook_dll_index])
                                        .show_ui(ui, |ui| {
                                            for (i, name) in HOOK_DLLS.iter().enumerate() {
                                                ui.selectable_value(&mut self.hook_dll_index, i, *name);
                                            }
                                        });
                                }
                            });
                        }

                        ui.horizontal(|ui| {
                            if ui.add_enabled(can_apply, egui::Button::new("Apply & Install")).clicked() {
                                self.apply_smokeapi();
                            }
                            if can_remove && ui.button("Remove SmokeAPI").clicked() {
                                self.remove_smokeapi();
                            }
                        });

                        if !self.smokeapi_ready {
                            ui.colored_label(egui::Color32::YELLOW,
                                "SmokeAPI not downloaded — use 'Get' in left panel");
                        }
                        if self.method == Method::Koaloader && !self.koaloader_ready {
                            ui.colored_label(egui::Color32::YELLOW,
                                "Koaloader not downloaded — use 'Get' in left panel");
                        }
                    }
                }
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label("Select a game from the left panel");
                });
            }
        });
    }
}

impl App {
    fn download_smokeapi(&mut self) {
        self.downloading = Some("SmokeAPI");
        self.status_message = "Downloading SmokeAPI...".to_string();
        let result = self.setup_result.clone();
        tokio::task::spawn(async move {
            result.set(setup::download_smokeapi().await);
        });
    }

    fn download_koaloader(&mut self) {
        self.downloading = Some("Koaloader");
        self.status_message = "Downloading Koaloader (75MB)...".to_string();
        let result = self.setup_result.clone();
        tokio::task::spawn(async move {
            result.set(setup::download_koaloader().await);
        });
    }

    fn delete_smokeapi(&mut self) {
        let dir = setup::cache_dir();
        let _ = std::fs::remove_dir_all(&dir);
        self.smokeapi_ready = false;
        self.status_message = "SmokeAPI deleted".to_string();
    }

    fn delete_koaloader(&mut self) {
        let dir = setup::koaloader_dir();
        let _ = std::fs::remove_dir_all(&dir);
        self.koaloader_ready = false;
        self.status_message = "Koaloader deleted".to_string();
    }

    fn apply_smokeapi(&mut self) {
        let api_path = match self.detection.api_path.clone() {
            Some(p) => p,
            None => {
                self.status_message = "No steam_api file found".to_string();
                return;
            }
        };

        let dlcs: Vec<u64> = self.unlocked_dlcs.iter().copied().collect();
        let cache = setup::cache_dir();

        let result = match self.method {
            Method::Proxy => smokeapi::install_proxy(&api_path, &self.detection.game_type, &dlcs, &cache),
            Method::Hook => smokeapi::install_hook(
                &api_path, self.detection.arch, &self.detection.game_type,
                HOOK_DLLS[self.hook_dll_index], &dlcs, &cache,
            ),
            Method::Koaloader => {
                let koala = setup::koaloader_dir();
                smokeapi::install_koaloader(
                    &api_path, self.detection.arch, &self.detection.game_type,
                    HOOK_DLLS[self.hook_dll_index], &dlcs, &cache, &koala,
                )
            }
        };

        match result {
            Ok(()) => {
                self.reanalyze();
                self.status_message = format!("Installed with {} DLC(s) unlocked", dlcs.len());
            }
            Err(e) => self.status_message = e,
        }
    }

    fn remove_smokeapi(&mut self) {
        let api_path = match self.detection.api_path.clone() {
            Some(p) => p,
            None => {
                self.status_message = "No steam_api file found".to_string();
                return;
            }
        };

        let had_backup = self.detection.backup_path.as_ref()
            .map(|p| p.exists()).unwrap_or(false);

        match smokeapi::remove_proxy(&api_path, &self.detection.game_type) {
            Ok(()) => {
                self.unlocked_dlcs.clear();
                self.reanalyze();
                self.status_message = if had_backup {
                    "Removed, original restored".to_string()
                } else {
                    "Removed. Verify game files in Steam if needed.".to_string()
                };
            }
            Err(e) => self.status_message = e,
        }
    }
}

#[tokio::main]
async fn main() {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 680.0])
            .with_min_inner_size([800.0, 500.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Vapor Activator",
        native_options,
        Box::new(|_cc| Ok(Box::new(App::new()))),
    )
    .unwrap();
}
