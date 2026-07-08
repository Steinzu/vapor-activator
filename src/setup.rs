use std::path::PathBuf;

const RELEASE_API: &str =
    "https://api.github.com/repos/acidicoala/SmokeAPI/releases/latest";

pub fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("vapor-activator")
        .join("smokeapi")
}

pub fn is_installed() -> bool {
    let dir = cache_dir();
    dir.join("smoke_api64.dll").exists() || dir.join("libsmoke_api64.so").exists()
}

pub async fn download_latest() -> Result<(), String> {
    let client = reqwest::Client::builder()
        .user_agent("vapor-activator/0.1")
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;

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
        .get(RELEASE_API)
        .send()
        .await
        .map_err(|e| format!("GitHub API error: {e}"))?
        .json()
        .await
        .map_err(|e| format!("JSON error: {e}"))?;

    let zip_asset = release
        .assets
        .iter()
        .find(|a| a.name.ends_with(".zip"))
        .ok_or("No zip asset found in release")?;

    let zip_bytes = client
        .get(&zip_asset.browser_download_url)
        .send()
        .await
        .map_err(|e| format!("Download error: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("Download error: {e}"))?;

    let cache = cache_dir();
    std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;

    let cursor = std::io::Cursor::new(zip_bytes.as_ref());
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = file.name().to_string();

        // Extract only the filename, skipping directory entries
        let fname = std::path::Path::new(&name)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        if !fname.ends_with(".dll") && !fname.ends_with(".so") {
            continue;
        }

        let out_path = cache.join(fname);
        let mut out = std::fs::File::create(&out_path).map_err(|e| e.to_string())?;
        std::io::copy(&mut file, &mut out).map_err(|e| e.to_string())?;
    }

    Ok(())
}
