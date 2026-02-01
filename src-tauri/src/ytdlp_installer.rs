use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncWriteExt;

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

    pub async fn install() -> Result<(), String> {
        let ytdlp_dir = Self::get_ytdlp_dir();
        let ytdlp_path = Self::get_ytdlp_path();

        // Create directory if it doesn't exist
        fs::create_dir_all(&ytdlp_dir)
            .await
            .map_err(|e| format!("Failed to create directory: {}", e))?;

        // Download URL based on platform
        #[cfg(target_os = "windows")]
        let download_url = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe";

        #[cfg(target_os = "linux")]
        let download_url = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp";

        println!("Downloading yt-dlp from: {}", download_url);

        // Download the binary
        let response = reqwest::get(download_url)
            .await
            .map_err(|e| format!("Failed to download yt-dlp: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("Failed to download yt-dlp: HTTP {}", response.status()));
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| format!("Failed to read download: {}", e))?;

        // Write to file
        let mut file = fs::File::create(&ytdlp_path)
            .await
            .map_err(|e| format!("Failed to create file: {}", e))?;

        file.write_all(&bytes)
            .await
            .map_err(|e| format!("Failed to write file: {}", e))?;

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

        println!("yt-dlp installed successfully at: {}", ytdlp_path.display());

        Ok(())
    }

    pub async fn get_version() -> Result<String, String> {
        let ytdlp_path = Self::get_ytdlp_path();

        if !ytdlp_path.exists() {
            return Err("yt-dlp not installed".to_string());
        }

        let output = tokio::process::Command::new(&ytdlp_path)
            .arg("--version")
            .output()
            .await
            .map_err(|e| format!("Failed to get version: {}", e))?;

        if !output.status.success() {
            return Err("Failed to get yt-dlp version".to_string());
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}
