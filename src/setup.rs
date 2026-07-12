// Download and cache SmokeAPI + Koaloader releases from GitHub.
// Koaloader zips have x64/x86 subdirs — we suffix as name64.dll / name32.dll.

use std::path::PathBuf;

const SMOKEAPI_RELEASE: &str =
    "https://api.github.com/repos/acidicoala/SmokeAPI/releases/latest";
const KOALOADER_RELEASE: &str =
    "https://api.github.com/repos/acidicoala/Koaloader/releases/latest";

fn base_cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("vapor-activator")
}

pub fn cache_dir() -> PathBuf {
    base_cache_dir().join("smokeapi")
}

pub fn koaloader_dir() -> PathBuf {
    base_cache_dir().join("koaloader")
}

pub fn is_smokeapi_installed() -> bool {
    let dir = cache_dir();
    dir.join("smoke_api64.dll").exists() || dir.join("libsmoke_api64.so").exists()
}

pub fn is_koaloader_installed() -> bool {
    let dir = koaloader_dir();
    dir.join("version.dll").exists() || dir.join("version64.dll").exists() || dir.join("version32.dll").exists()
}

async fn fetch_release_zip(client: &reqwest::Client, api_url: &str) -> Result<Vec<u8>, String> {
    #[derive(serde::Deserialize)]
    struct Release {
        assets: Vec<Asset>,
    }
    #[derive(serde::Deserialize)]
    struct Asset {
        name: String,
        browser_download_url: String,
    }

    let release: Release = client
        .get(api_url)
        .send().await.map_err(|e| format!("API error: {e}"))?
        .json().await.map_err(|e| format!("JSON error: {e}"))?;

    let zip = release.assets.iter()
        .find(|a| a.name.ends_with(".zip"))
        .ok_or("No zip found")?;

    client.get(&zip.browser_download_url)
        .send().await.map_err(|e| format!("Download error: {e}"))?
        .bytes().await.map_err(|e| format!("Download error: {e}"))
        .map(|b| b.to_vec())
}

fn extract_dlls(zip_bytes: &[u8], out_dir: &PathBuf, prefixes: &[&str]) -> Result<(), String> {
    let cursor = std::io::Cursor::new(zip_bytes);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = file.name().to_string();
        let path = std::path::Path::new(&name);

        let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !fname.ends_with(".dll") && !fname.ends_with(".so") {
            continue;
        }

        if !prefixes.is_empty() && !prefixes.iter().any(|p| fname.starts_with(p)) {
            continue;
        }

        // Koaloader zip has x64/ and x86/ subdirs with same filenames.
        // Distinguish by extracting as name64.dll / name32.dll based on parent.
        let parent_dir = path.parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str());
        let out_name = if parent_dir == Some("x86") {
            let stem = fname.strip_suffix(".dll").unwrap_or(fname);
            format!("{}32.dll", stem)
        } else if parent_dir == Some("x64") {
            let stem = fname.strip_suffix(".dll").unwrap_or(fname);
            format!("{}64.dll", stem)
        } else {
            fname.to_string()
        };

        let out_path = out_dir.join(&out_name);
        let mut out = std::fs::File::create(&out_path).map_err(|e| e.to_string())?;
        std::io::copy(&mut file, &mut out).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub async fn download_smokeapi() -> Result<(), String> {
    let client = reqwest::Client::builder()
        .user_agent("vapor-activator/0.1")
        .timeout(std::time::Duration::from_secs(120))
        .build().map_err(|e| e.to_string())?;

    let zip_bytes = fetch_release_zip(&client, SMOKEAPI_RELEASE).await?;
    extract_dlls(&zip_bytes, &cache_dir(), &[])
}

pub async fn download_koaloader() -> Result<(), String> {
    let client = reqwest::Client::builder()
        .user_agent("vapor-activator/0.1")
        .timeout(std::time::Duration::from_secs(300))
        .build().map_err(|e| e.to_string())?;

    let zip_bytes = fetch_release_zip(&client, KOALOADER_RELEASE).await?;
    // Koaloader zip has DLLs in nested dirs; extract only the ones we support
    extract_dlls(&zip_bytes, &koaloader_dir(), &["version", "winhttp", "winmm", "dinput8", "d3d11", "dxgi"])
}
