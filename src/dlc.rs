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
) -> Vec<DlcInfo> {
    let mut dlcs = Vec::with_capacity(ids.len());
    let count = ids.len();

    for (i, &id) in ids.iter().enumerate() {
        let url = format!(
            "https://store.steampowered.com/api/appdetails?appids={}",
            id
        );

        let name = match client.get(&url).send().await {
            Ok(r) => match r.json::<HashMap<String, AppDetailsResponse>>().await {
                Ok(resp) => resp
                    .get(&id.to_string())
                    .and_then(|r| r.data.as_ref())
                    .map(|d| d.name.clone())
                    .unwrap_or_else(|| format!("DLC {}", id)),
                Err(_) => format!("DLC {}", id),
            },
            Err(_) => format!("DLC {}", id),
        };

        dlcs.push(DlcInfo { appid: id, name });

        // Rate limit for games with many DLCs, skip last iteration
        if count > 10 && i + 1 < count {
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        }
    }

    dlcs.sort_by_key(|a| a.name.to_lowercase());
    dlcs
}

pub async fn fetch_dlc_list(appid: u64) -> Result<Vec<DlcInfo>, String> {
    let client = build_client()?;

    // 1. Get DLC IDs from Steam Store API
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

    // 2. Merge hidden DLCs from community list
    if let Ok(hidden_resp) = client.get(HIDDEN_DLC_URL).send().await {
        if let Ok(hidden_map) = hidden_resp
            .json::<HashMap<String, HashMap<String, String>>>()
            .await
        {
            if let Some(game_dlcs) = hidden_map.get(&appid.to_string()) {
                for id_str in game_dlcs.keys() {
                    if let Ok(id) = id_str.parse::<u64>() {
                        if !dlc_ids.contains(&id) {
                            dlc_ids.push(id);
                        }
                    }
                }
            }
        }
    }

    if dlc_ids.is_empty() {
        return Ok(vec![]);
    }

    Ok(fetch_dlc_names(&client, &dlc_ids).await)
}
