use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct DlcInfo {
    pub appid: u64,
    pub name: String,
}

#[derive(Deserialize, Debug)]
struct AppDetailsResponse {
    #[serde(default)]
    data: Option<AppDetailsData>,
}

#[derive(Deserialize, Debug)]
struct AppDetailsData {
    #[serde(default)]
    name: String,
    #[serde(default)]
    dlc: Vec<u64>,
}

#[derive(Deserialize, Debug)]
struct SteamCmdResponse {
    data: Option<HashMap<String, SteamCmdApp>>,
}

#[derive(Deserialize, Debug)]
struct SteamCmdApp {
    #[serde(default)]
    extended: Option<SteamCmdExtended>,
    #[serde(default)]
    depots: Option<HashMap<String, SteamCmdDepot>>,
}

#[derive(Deserialize, Debug)]
struct SteamCmdExtended {
    #[serde(default)]
    listofdlc: String,
}

#[derive(Deserialize, Debug)]
struct SteamCmdDepot {
    #[serde(default)]
    dlcappid: String,
}

const HIDDEN_DLC_URL: &str =
    "https://raw.githubusercontent.com/acidicoala/public-entitlements/refs/heads/steam/v2/dlc.json";

fn build_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("vapor-activator/0.1")
        .build()
        .map_err(|e| e.to_string())
}

async fn fetch_dlc_names(
    client: &reqwest::Client,
    ids: &[u64],
    hidden_names: &HashMap<u64, String>,
) -> Vec<DlcInfo> {
    let mut dlcs = Vec::with_capacity(ids.len());
    let count = ids.len();

    for (i, &id) in ids.iter().enumerate() {
        let url = format!(
            "https://store.steampowered.com/api/appdetails?appids={}",
            id
        );

        let mut name = match client.get(&url).send().await {
            Ok(r) => match r.json::<HashMap<String, AppDetailsResponse>>().await {
                Ok(resp) => resp
                    .get(&id.to_string())
                    .and_then(|r| r.data.as_ref())
                    .map(|d| d.name.clone())
                    .filter(|n| !n.is_empty())
                    .unwrap_or_default(),
                Err(_) => String::new(),
            },
            Err(_) => String::new(),
        };

        // Fall back to hidden DLCs list for names
        if name.is_empty() {
            name = hidden_names.get(&id).cloned().unwrap_or_else(|| format!("DLC {}", id));
        }

        dlcs.push(DlcInfo { appid: id, name });

        if count > 10 && i + 1 < count {
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        }
    }

    dlcs.sort_by_key(|a| a.name.to_lowercase());
    dlcs
}

fn parse_comma_dlc_ids(s: &str) -> Vec<u64> {
    s.split(',').filter_map(|id| id.trim().parse::<u64>().ok()).collect()
}

async fn fetch_dlc_from_steamcmd(
    client: &reqwest::Client,
    appid: u64,
) -> Vec<u64> {
    let url = format!("https://api.steamcmd.net/v1/info/{}", appid);
    let resp = match client.get(&url).send().await {
        Ok(r) => r,
        Err(_) => return vec![],
    };
    let cmd: SteamCmdResponse = match resp.json().await {
        Ok(r) => r,
        Err(_) => return vec![],
    };
    let app = match cmd.data.and_then(|mut d| d.remove(&appid.to_string())) {
        Some(a) => a,
        None => return vec![],
    };

    let mut ids = Vec::new();

    if let Some(ext) = app.extended {
        ids.extend(parse_comma_dlc_ids(&ext.listofdlc));
    }
    if let Some(depots) = app.depots {
        for (_, depot) in depots {
            if !depot.dlcappid.is_empty() {
                if let Ok(id) = depot.dlcappid.parse::<u64>() {
                    if !ids.contains(&id) {
                        ids.push(id);
                    }
                }
            }
        }
    }

    ids
}

async fn fetch_hidden_dlcs(
    client: &reqwest::Client,
    appid: u64,
) -> (Vec<u64>, HashMap<u64, String>) {
    let mut ids = Vec::new();
    let mut names = HashMap::new();

    let resp = match client.get(HIDDEN_DLC_URL).send().await {
        Ok(r) => r,
        Err(_) => return (ids, names),
    };
    let map: HashMap<String, HashMap<String, String>> = match resp.json().await {
        Ok(m) => m,
        Err(_) => return (ids, names),
    };

    if let Some(game_dlcs) = map.get(&appid.to_string()) {
        for (id_str, dlc_name) in game_dlcs {
            if let Ok(id) = id_str.parse::<u64>() {
                if !ids.contains(&id) {
                    ids.push(id);
                }
                names.insert(id, dlc_name.clone());
            }
        }
    }

    (ids, names)
}

pub async fn fetch_dlc_list(appid: u64) -> Result<Vec<DlcInfo>, String> {
    let client = build_client()?;

    let url = format!(
        "https://store.steampowered.com/api/appdetails?appids={}",
        appid
    );

    let resp: HashMap<String, AppDetailsResponse> = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("HTTP error: {e}"))?
        .json()
        .await
        .map_err(|e| format!("JSON error: {e}"))?;

    let data = resp
        .get(&appid.to_string())
        .and_then(|r| r.data.as_ref())
        .ok_or("No app data returned")?;

    let mut dlc_ids: Vec<u64> = data.dlc.clone();

    // Fetch SteamCMD and hidden DLCs in parallel
    let (cmd_ids, (hidden_ids, hidden_names)) = tokio::join!(
        fetch_dlc_from_steamcmd(&client, appid),
        fetch_hidden_dlcs(&client, appid),
    );

    for id in cmd_ids {
        if !dlc_ids.contains(&id) {
            dlc_ids.push(id);
        }
    }
    for id in hidden_ids {
        if !dlc_ids.contains(&id) {
            dlc_ids.push(id);
        }
    }

    if dlc_ids.is_empty() {
        return Ok(vec![]);
    }

    Ok(fetch_dlc_names(&client, &dlc_ids, &hidden_names).await)
}
