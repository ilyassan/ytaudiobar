use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use std::sync::LazyLock;
use serde::{Deserialize, Serialize};
use futures_util::StreamExt;
use tauri::{AppHandle, Emitter};
use crate::command_utils::{command_no_window, unix_timestamp};

static INSTALL_LOCK: LazyLock<Arc<Mutex<bool>>> = LazyLock::new(|| Arc::new(Mutex::new(false)));

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
}

#[derive(Serialize, Deserialize)]
struct UpdateCheck {
    last_check: i64,
}

#[derive(Clone, Serialize)]
pub struct DepProgress {
    pub dependency: String,
    pub downloaded: u64,
    pub total: u64,
}

pub struct YTDLPInstaller;

impl YTDLPInstaller {
    pub fn get_ytdlp_dir() -> PathBuf {
        let mut path = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."));
        path.push("ytaudiobar");
        path.push("bin");
        path
    }

    pub fn get_ytdlp_path() -> PathBuf {
        let mut path = Self::get_ytdlp_dir();

        #[cfg(target_os = "windows")]
        path.push("yt-dlp.exe");

        #[cfg(not(target_os = "windows"))]
        path.push("yt-dlp");

        path
    }

    pub async fn is_installed() -> bool {
        Self::get_ytdlp_path().exists()
    }

    async fn download_with_progress(app_handle: &AppHandle) -> Result<(), String> {
        let ytdlp_dir = Self::get_ytdlp_dir();
        let ytdlp_path = Self::get_ytdlp_path();

        fs::create_dir_all(&ytdlp_dir)
            .await
            .map_err(|e| format!("Failed to create directory: {}", e))?;

        #[cfg(target_os = "windows")]
        let download_url = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe";

        #[cfg(target_os = "linux")]
        let download_url = {
            // Use standalone binary (same approach as macOS - no Python needed)
            println!("📥 Downloading yt-dlp standalone binary for Linux");
            "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux"
        };

        println!("📥 Downloading yt-dlp from: {}", download_url);

        let response = reqwest::get(download_url)
            .await
            .map_err(|e| format!("Failed to download yt-dlp: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("Failed to download yt-dlp: HTTP {}", response.status()));
        }

        let total_size = response.content_length().unwrap_or(0);
        let mut downloaded: u64 = 0;
        let mut stream = response.bytes_stream();
        let mut file = fs::File::create(&ytdlp_path)
            .await
            .map_err(|e| format!("Failed to create file: {}", e))?;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| format!("Download error: {}", e))?;
            file.write_all(&chunk)
                .await
                .map_err(|e| format!("Write error: {}", e))?;

            downloaded += chunk.len() as u64;

            // Emit real progress
            let _ = app_handle.emit("dep-progress", DepProgress {
                dependency: "ytdlp".to_string(),
                downloaded,
                total: total_size,
            });
        }

        // Make executable on Linux
        #[cfg(not(target_os = "windows"))]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&ytdlp_path)
                .map_err(|e| format!("Failed to get file metadata: {}", e))?
                .permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&ytdlp_path, perms)
                .map_err(|e| format!("Failed to set permissions: {}", e))?;
        }

        println!("✅ yt-dlp installed at: {}", ytdlp_path.display());
        Ok(())
    }

    pub async fn install(app_handle: &AppHandle) -> Result<(), String> {
        let mut installing = INSTALL_LOCK.lock().await;

        if Self::is_installed().await {
            return Ok(());
        }

        if *installing {
            drop(installing);
            for _ in 0..120 {
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                if Self::is_installed().await {
                    return Ok(());
                }
            }
            return Err("yt-dlp installation timeout".to_string());
        }

        *installing = true;
        let result = Self::download_with_progress(app_handle).await;
        *installing = false;

        result
    }

    pub async fn get_version() -> Result<String, String> {
        let ytdlp_path = Self::get_ytdlp_path();

        if !ytdlp_path.exists() {
            return Err("yt-dlp not installed".to_string());
        }

        let output = command_no_window(ytdlp_path.to_str().unwrap_or("yt-dlp"))
            .arg("--version")
            .output()
            .await
            .map_err(|e| format!("Failed to get version: {}", e))?;

        if !output.status.success() {
            return Err("Failed to get yt-dlp version".to_string());
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn get_update_check_file() -> PathBuf {
        let mut path = Self::get_ytdlp_dir();
        path.push("last_update_check.json");
        path
    }

    async fn get_last_update_check() -> Option<i64> {
        let check_file = Self::get_update_check_file();
        if !check_file.exists() {
            return None;
        }

        let content = fs::read_to_string(&check_file).await.ok()?;
        let check: UpdateCheck = serde_json::from_str(&content).ok()?;
        Some(check.last_check)
    }

    async fn save_update_check() -> Result<(), String> {
        let check_file = Self::get_update_check_file();
        let check = UpdateCheck {
            last_check: unix_timestamp(),
        };
        let content = serde_json::to_string(&check)
            .map_err(|e| format!("Failed to serialize update check: {}", e))?;
        fs::write(&check_file, content)
            .await
            .map_err(|e| format!("Failed to write update check: {}", e))
    }

    pub async fn should_check_for_update() -> bool {
        match Self::get_last_update_check().await {
            Some(last_check) => {
                let now = unix_timestamp();
                let hours_since_check = (now - last_check) / 3600;
                hours_since_check >= 24
            }
            None => true,
        }
    }

    pub async fn fetch_latest_version() -> Result<String, String> {
        let url = "https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest";

        let client = reqwest::Client::new();
        let response = client
            .get(url)
            .header("User-Agent", "YTAudioBar")
            .send()
            .await
            .map_err(|e| format!("Failed to fetch latest version: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("GitHub API error: HTTP {}", response.status()));
        }

        let release: GitHubRelease = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        Ok(release.tag_name)
    }

    pub async fn check_and_update(app_handle: &AppHandle) -> Result<Option<String>, String> {
        if !Self::is_installed().await {
            return Err("yt-dlp not installed".to_string());
        }

        if !Self::should_check_for_update().await {
            return Ok(None);
        }

        println!("🔍 Checking for yt-dlp updates...");

        let current_version = Self::get_version().await?;
        let latest_version = Self::fetch_latest_version().await?;

        let _ = Self::save_update_check().await;

        if current_version == latest_version {
            println!("✅ yt-dlp is up to date ({})", current_version);
            return Ok(None);
        }

        println!("📦 Updating yt-dlp: {} → {}", current_version, latest_version);
        Self::install(app_handle).await?;

        println!("✅ yt-dlp updated to {}", latest_version);
        Ok(Some(latest_version))
    }
}
