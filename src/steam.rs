// VDF parser and Steam game discovery from libraryfolders.vdf + appmanifest_*.acf.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct InstalledGame {
    pub appid: u64,
    pub name: String,
    pub installdir: String,
    pub library_path: PathBuf,
}

impl InstalledGame {
    pub fn game_dir(&self) -> PathBuf {
        self.library_path
            .join("steamapps")
            .join("common")
            .join(&self.installdir)
    }
}

pub type VdfObject = BTreeMap<String, VdfValue>;

#[derive(Debug, Clone)]
pub enum VdfValue {
    String(String),
    Object(VdfObject),
}

pub fn parse_vdf(content: &str) -> Option<VdfObject> {
    let tokens = tokenize(content);
    let mut pos = 0;
    parse_object(&tokens, &mut pos)
}

fn tokenize(content: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = content.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        match chars[i] {
            '/' if i + 1 < chars.len() && chars[i + 1] == '/' => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '{' => {
                tokens.push("{".to_string());
                i += 1;
            }
            '}' => {
                tokens.push("}".to_string());
                i += 1;
            }
            '"' => {
                i += 1;
                let mut s = String::new();
                while i < chars.len() && chars[i] != '"' {
                    if chars[i] == '\\' && i + 1 < chars.len() {
                        i += 1;
                        match chars[i] {
                            'n' => s.push('\n'),
                            't' => s.push('\t'),
                            '\\' => s.push('\\'),
                            '"' => s.push('"'),
                            c => {
                                s.push('\\');
                                s.push(c);
                            }
                        }
                    } else {
                        s.push(chars[i]);
                    }
                    i += 1;
                }
                if i < chars.len() {
                    i += 1;
                }
                tokens.push(s);
            }
            c if c.is_whitespace() => {
                i += 1;
            }
            _ => {
                let mut s = String::new();
                while i < chars.len()
                    && !chars[i].is_whitespace()
                    && chars[i] != '"'
                    && chars[i] != '{'
                    && chars[i] != '}'
                {
                    s.push(chars[i]);
                    i += 1;
                }
                if !s.is_empty() {
                    tokens.push(s);
                }
            }
        }
    }
    tokens
}

fn parse_object(tokens: &[String], pos: &mut usize) -> Option<VdfObject> {
    let mut obj = VdfObject::new();

    while *pos < tokens.len() {
        if tokens[*pos] == "}" {
            *pos += 1;
            return Some(obj);
        }

        let key = tokens[*pos].clone();
        *pos += 1;

        if *pos >= tokens.len() {
            break;
        }

        if tokens[*pos] == "{" {
            *pos += 1;
            if let Some(sub) = parse_object(tokens, pos) {
                obj.insert(key, VdfValue::Object(sub));
            }
        } else {
            let val = tokens[*pos].clone();
            *pos += 1;
            obj.insert(key, VdfValue::String(val));
        }
    }

    Some(obj)
}

pub fn discover_games(steam_root: &Path) -> Vec<InstalledGame> {
    const NON_GAME_IDS: &[u64] = &[
        1070560, 1391110, 1493710, 1628350, 2180100, 228980, 4183110,
    ];

    let vdf_path = steam_root.join("steamapps").join("libraryfolders.vdf");
    let content = match std::fs::read_to_string(&vdf_path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    // Collect unique library paths (steam_root + any secondary libraries from VDF)
    let mut libs: Vec<PathBuf> = vec![steam_root.to_path_buf()];
    if let Some(root) = parse_vdf(&content) {
        if let Some(VdfValue::Object(folders)) = root.get("libraryfolders") {
            for val in folders.values() {
                if let VdfValue::Object(obj) = val {
                    if let Some(VdfValue::String(path)) = obj.get("path") {
                        let p = PathBuf::from(path);
                        if !libs.contains(&p) {
                            libs.push(p);
                        }
                    }
                }
            }
        }
    }

    let mut games = Vec::new();
    for lib_path in &libs {
        let manifest_dir = lib_path.join("steamapps");
        let entries = match std::fs::read_dir(&manifest_dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let fname = entry.file_name();
            let fname = fname.to_string_lossy();
            if !fname.starts_with("appmanifest_") || !fname.ends_with(".acf") {
                continue;
            }

            let content = match std::fs::read_to_string(entry.path()) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let root = match parse_vdf(&content) {
                Some(r) => r,
                None => continue,
            };
            let appstate = match root.get("AppState") {
                Some(VdfValue::Object(o)) => o,
                _ => continue,
            };

            let appid: u64 = match appstate.get("appid") {
                Some(VdfValue::String(s)) => {
                    match s.parse() {
                        Ok(id) => id,
                        Err(_) => continue,
                    }
                }
                _ => continue,
            };
            if NON_GAME_IDS.contains(&appid) {
                continue;
            }

            let name = match appstate.get("name") {
                Some(VdfValue::String(s)) => s.clone(),
                _ => continue,
            };
            let installdir = match appstate.get("installdir") {
                Some(VdfValue::String(s)) => s.clone(),
                _ => continue,
            };

            let game = InstalledGame {
                appid,
                name,
                installdir,
                library_path: lib_path.clone(),
            };
            if !game.game_dir().exists() {
                continue;
            }
            games.push(game);
        }
    }

    games.sort_by_key(|a| a.name.to_lowercase());
    games
}
