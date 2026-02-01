use crate::models::YTVideoInfo;
use crate::ytdlp_installer::YTDLPInstaller;
use serde_json::Value;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

pub struct YTDLPManager;

impl YTDLPManager {
    pub fn new() -> Self {
        Self
    }

    pub async fn search(&self, query: String, music_mode: bool) -> Result<Vec<YTVideoInfo>, String> {
        let search_query = if music_mode {
            format!("ytsearch10:{} music song audio", query)
        } else {
            format!("ytsearch10:{}", query)
        };

        let ytdlp_path = Self::get_ytdlp_path();

        let mut child = Command::new(&ytdlp_path)
            .args(&[
                "--dump-json",
                "--flat-playlist",
                "--no-warnings",
                "--ignore-errors",
                &search_query,
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to spawn yt-dlp: {}. Make sure yt-dlp is installed.", e))?;

        let stdout = child
            .stdout
            .take()
            .ok_or("Failed to capture stdout")?;

        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        let mut results = Vec::new();

        while let Ok(Some(line)) = lines.next_line().await {
            if let Ok(json) = serde_json::from_str::<Value>(&line) {
                if let Some(video) = Self::parse_video_info(&json) {
                    results.push(video);
                }
            }
        }

        child.wait().await.map_err(|e| format!("yt-dlp process error: {}", e))?;

        Ok(results)
    }

    pub async fn get_audio_url(&self, video_id: String) -> Result<String, String> {
        let ytdlp_path = Self::get_ytdlp_path();
        let url = format!("https://www.youtube.com/watch?v={}", video_id);

        let output = Command::new(&ytdlp_path)
            .args(&[
                "--dump-json",
                "-f", "bestaudio",
                "--no-warnings",
                &url,
            ])
            .output()
            .await
            .map_err(|e| format!("Failed to get audio URL: {}", e))?;

        if !output.status.success() {
            return Err("Failed to extract audio URL from YouTube".to_string());
        }

        let json: Value = serde_json::from_slice(&output.stdout)
            .map_err(|e| format!("Failed to parse yt-dlp output: {}", e))?;

        json.get("url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "No audio URL found in response".to_string())
    }

    fn parse_video_info(json: &Value) -> Option<YTVideoInfo> {
        Some(YTVideoInfo {
            id: json.get("id")?.as_str()?.to_string(),
            title: json.get("title")?.as_str()?.to_string(),
            uploader: json
                .get("uploader")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown")
                .to_string(),
            duration: json.get("duration").and_then(|v| v.as_i64()).unwrap_or(0),
            thumbnail_url: json
                .get("thumbnail")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            audio_url: None,
            description: json
                .get("description")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        })
    }

    fn get_ytdlp_path() -> String {
        // Use the installer's path
        let installed_path = YTDLPInstaller::get_ytdlp_path();
        installed_path.to_string_lossy().to_string()
    }

    pub async fn check_ytdlp_exists(&self) -> bool {
        let ytdlp_path = Self::get_ytdlp_path();

        Command::new(&ytdlp_path)
            .arg("--version")
            .output()
            .await
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    pub async fn update_ytdlp(&self) -> Result<(), String> {
        let ytdlp_path = Self::get_ytdlp_path();

        let output = Command::new(&ytdlp_path)
            .arg("-U")
            .output()
            .await
            .map_err(|e| format!("Failed to update yt-dlp: {}", e))?;

        if !output.status.success() {
            return Err("Failed to update yt-dlp".to_string());
        }

        Ok(())
    }
}
