use crate::command_utils::{command_no_window, unix_timestamp};
use crate::models::YTVideoInfo;
use crate::ytdlp_installer::YTDLPInstaller;
use crate::analytics::Analytics;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::{Mutex, Semaphore};

// Caps how many yt-dlp download processes run at once — a "Download All" on a
// large playlist would otherwise spawn one process per track simultaneously,
// hammering the network/CPU and likely tripping YouTube's rate limiting.
const MAX_CONCURRENT_DOWNLOADS: usize = 3;

// Helper function to build YouTube bypass arguments for downloads
// Start with no bypass, escalate if needed
fn build_youtube_bypass_args() -> Vec<String> {
    // Start with no bypass - just use normal yt-dlp
    // The ytdlp_manager already handles the escalation through multiple methods if needed
    // For downloads, we start simple and let yt-dlp work normally
    println!("🎯 Using normal yt-dlp for download (no bypass by default)");
    Vec::new()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub video_id: String,
    pub progress: f64, // 0.0 to 1.0
    pub speed: String,
    pub eta: String,
    pub file_size: String,
    pub is_completed: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadedTrack {
    pub video_info: YTVideoInfo,
    pub file_path: String,
    pub file_size: i64,
    pub download_date: i64,
}

pub struct DownloadManager {
    active_downloads: Arc<Mutex<HashMap<String, DownloadProgress>>>,
    completed_downloads: Arc<Mutex<Vec<String>>>, // video IDs
    downloads_dir: Arc<Mutex<PathBuf>>,
    audio_quality: Arc<Mutex<String>>, // Audio quality preference
    app_handle: Arc<Mutex<Option<AppHandle>>>,
    download_semaphore: Arc<Semaphore>,
    analytics: Arc<Analytics>,
}

impl DownloadManager {
    pub fn new(analytics: Arc<Analytics>) -> Self {
        // Default downloads directory
        let downloads_dir = dirs::download_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("YTAudioBar Downloads");

        // Create directory if it doesn't exist
        std::fs::create_dir_all(&downloads_dir).ok();

        Self {
            active_downloads: Arc::new(Mutex::new(HashMap::new())),
            completed_downloads: Arc::new(Mutex::new(Vec::new())),
            downloads_dir: Arc::new(Mutex::new(downloads_dir)),
            audio_quality: Arc::new(Mutex::new("best".to_string())), // Default to best quality
            app_handle: Arc::new(Mutex::new(None)),
            download_semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_DOWNLOADS)),
            analytics,
        }
    }

    pub async fn set_app_handle(&self, handle: AppHandle) {
        *self.app_handle.lock().await = Some(handle);
    }

    /// Initialize by scanning downloads directory for existing downloads
    pub async fn initialize(&self) {
        let downloads_dir = self.downloads_dir.lock().await.clone();
        let mut completed = self.completed_downloads.lock().await;

        // One directory scan for audio files, then a second pass over metadata
        // files checking against that map -- instead of re-scanning the whole
        // directory per metadata file found.
        let audio_files = scan_audio_files_by_id(&downloads_dir);

        if let Ok(entries) = std::fs::read_dir(&downloads_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(file_name) = path.file_name() {
                    let name = file_name.to_string_lossy();
                    // Look for metadata files
                    if name.ends_with("_metadata.json") {
                        // Extract video ID from filename
                        let video_id = name.trim_end_matches("_metadata.json").to_string();
                        // Check if corresponding audio file exists
                        if audio_files.contains_key(&video_id) {
                            completed.push(video_id);
                        }
                    }
                }
            }
        }

        println!("Initialized download manager with {} existing downloads", completed.len());
    }

    pub async fn set_downloads_dir_silent(&self, path: PathBuf) {
        std::fs::create_dir_all(&path).ok();
        *self.downloads_dir.lock().await = path;
    }

    pub async fn set_downloads_dir(&self, path: PathBuf) -> Result<(), String> {
        // Get old directory
        let old_dir = self.downloads_dir.lock().await.clone();

        // Create new directory if it doesn't exist
        std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;

        // Check if old directory has files to migrate
        let should_migrate = if old_dir != path {
            self.has_downloads_in_directory(&old_dir).await
        } else {
            false
        };

        if should_migrate {
            // Check if new directory is empty
            let is_new_dir_empty = self.is_directory_empty(&path).await;

            if !is_new_dir_empty {
                return Err("Target directory is not empty. Please choose an empty folder or manually move your downloads.".to_string());
            }

            // Migrate downloads
            self.migrate_downloads(&old_dir, &path).await?;
        }

        // Update the directory
        *self.downloads_dir.lock().await = path;

        Ok(())
    }

    async fn has_downloads_in_directory(&self, dir: &PathBuf) -> bool {
        if let Ok(entries) = std::fs::read_dir(dir) {
            let audio_extensions = ["flac", "m4a", "webm", "mp3", "aac", "ogg"];
            for entry in entries.flatten() {
                if let Some(ext) = entry.path().extension() {
                    if audio_extensions.contains(&ext.to_str().unwrap_or("")) {
                        return true;
                    }
                }
            }
        }
        false
    }

    async fn is_directory_empty(&self, dir: &PathBuf) -> bool {
        if let Ok(mut entries) = std::fs::read_dir(dir) {
            entries.next().is_none()
        } else {
            true
        }
    }

    async fn migrate_downloads(&self, from: &PathBuf, to: &PathBuf) -> Result<(), String> {
        println!("🚚 Migrating downloads from {} to {}", from.display(), to.display());

        let mut migrated_count = 0;
        let mut error_count = 0;

        if let Ok(entries) = std::fs::read_dir(from) {
            let audio_extensions = ["flac", "m4a", "webm", "mp3", "aac", "ogg", "json"];

            for entry in entries.flatten() {
                let path = entry.path();
                let file_name = path.file_name().unwrap_or_default();

                // Check if it's an audio file or metadata file
                let should_migrate = if let Some(ext) = path.extension() {
                    audio_extensions.contains(&ext.to_str().unwrap_or(""))
                } else {
                    false
                };

                if should_migrate {
                    let dest_path = to.join(file_name);

                    match std::fs::rename(&path, &dest_path) {
                        Ok(_) => {
                            migrated_count += 1;
                            println!("✅ Migrated: {}", file_name.to_string_lossy());
                        }
                        Err(e) => {
                            error_count += 1;
                            eprintln!("❌ Failed to migrate {}: {}", file_name.to_string_lossy(), e);
                        }
                    }
                }
            }
        }

        println!("🎉 Migration complete: {} files moved, {} errors", migrated_count, error_count);

        if error_count > 0 {
            Err(format!("Migration completed with {} errors", error_count))
        } else {
            Ok(())
        }
    }

    pub async fn download_track(&self, track: YTVideoInfo) -> Result<(), String> {
        let video_id = track.id.clone();

        // Check if already downloading
        {
            let active = self.active_downloads.lock().await;
            if active.contains_key(&video_id) {
                return Err("Download already in progress".to_string());
            }
        }

        // Check if already downloaded
        {
            let completed = self.completed_downloads.lock().await;
            if completed.contains(&video_id) {
                return Err("Track already downloaded".to_string());
            }
        }

        // Initialize progress
        {
            let mut active = self.active_downloads.lock().await;
            active.insert(
                video_id.clone(),
                DownloadProgress {
                    video_id: video_id.clone(),
                    progress: 0.0,
                    speed: "Starting...".to_string(),
                    eta: "Calculating...".to_string(),
                    file_size: "Unknown".to_string(),
                    is_completed: false,
                    error: None,
                },
            );
        }

        self.emit_downloads_update().await;

        // Spawn download task
        let self_clone = Arc::new(self.clone_for_task());
        let track_clone = track.clone();

        tokio::spawn(async move {
            if let Err(e) = self_clone.download_with_ytdlp(track_clone).await {
                println!("❌ Download failed: {}", e);
                self_clone
                    .update_download_error(&video_id, &e.to_string())
                    .await;
            }
        });

        Ok(())
    }

    fn clone_for_task(&self) -> Self {
        Self {
            active_downloads: Arc::clone(&self.active_downloads),
            completed_downloads: Arc::clone(&self.completed_downloads),
            downloads_dir: Arc::clone(&self.downloads_dir),
            audio_quality: Arc::clone(&self.audio_quality),
            app_handle: Arc::clone(&self.app_handle),
            download_semaphore: Arc::clone(&self.download_semaphore),
            analytics: Arc::clone(&self.analytics),
        }
    }

    async fn download_with_ytdlp(&self, track: YTVideoInfo) -> Result<(), String> {
        // Wait for a free slot before actually spawning yt-dlp — the track still shows
        // as "active" (queued) in the UI immediately, it just won't start downloading
        // until fewer than MAX_CONCURRENT_DOWNLOADS others are in flight.
        let _permit = self
            .download_semaphore
            .acquire()
            .await
            .map_err(|e| e.to_string())?;

        let ytdlp_path = YTDLPInstaller::get_ytdlp_path();
        let downloads_dir = self.downloads_dir.lock().await.clone();
        let quality = self.audio_quality.lock().await.clone();

        let safe_title = sanitize_filename(&track.title);
        let safe_uploader = sanitize_filename(&track.uploader);
        // Include video_id in filename to uniquely identify downloads
        let filename = format!("[{}] {} - {}", track.id, safe_title, safe_uploader);

        let output_template = downloads_dir
            .join(format!("{}.%(ext)s", filename))
            .to_string_lossy()
            .to_string();

        let video_url = format!("https://www.youtube.com/watch?v={}", track.id);

        // Build format string based on quality setting
        // Prefer formats that Symphonia fully supports (MP3, M4A/AAC, OGG/Vorbis, FLAC)
        // Avoid WebM/Opus which has incomplete Symphonia support
        let format_string = match quality.as_str() {
            "320" => "bestaudio[abr<=320][ext=mp3]/bestaudio[abr<=320][ext=m4a]/bestaudio[abr<=320][ext=ogg]/bestaudio[abr<=320]",
            "256" => "bestaudio[abr<=256][ext=mp3]/bestaudio[abr<=256][ext=m4a]/bestaudio[abr<=256][ext=ogg]/bestaudio[abr<=256]",
            "192" => "bestaudio[abr<=192][ext=mp3]/bestaudio[abr<=192][ext=m4a]/bestaudio[abr<=192][ext=ogg]/bestaudio[abr<=192]",
            "128" => "bestaudio[abr<=128][ext=mp3]/bestaudio[abr<=128][ext=m4a]/bestaudio[abr<=128][ext=ogg]/bestaudio[abr<=128]",
            _ => "bestaudio[ext=mp3]/bestaudio[ext=m4a]/bestaudio[ext=ogg]/bestaudio",
        };

        // Build bypass arguments
        let bypass_args = build_youtube_bypass_args();

        // Build complete argument list
        let mut ytdlp_args = vec![
            "--format".to_string(),
            format_string.to_string(),
            "--output".to_string(),
            output_template.clone(),
            "--no-playlist".to_string(),
            "--newline".to_string(), // Force yt-dlp to output progress on new lines
            "--progress".to_string(),
        ];
        ytdlp_args.extend(bypass_args);
        ytdlp_args.push(video_url.clone());

        let args_refs: Vec<&str> = ytdlp_args.iter().map(|s| s.as_str()).collect();

        // Use tokio::process::Command for proper async I/O
        let mut child = command_no_window(&ytdlp_path.to_string_lossy())
            .args(&args_refs)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn yt-dlp: {}", e))?;

        let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
        let video_id = track.id.clone();
        let self_for_parse = self.clone_for_task();

        // Spawn task to parse output
        let parse_handle = tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, BufReader};
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();

            while let Ok(Some(line)) = lines.next_line().await {
                self_for_parse.parse_ytdlp_progress(&line, &video_id).await;
            }
        });

        let status = child.wait().await.map_err(|e| format!("Wait failed: {}", e))?;

        // Wait for parsing to complete
        let _ = parse_handle.await;

        if status.success() {
            self.mark_download_completed(&track).await?;
            Ok(())
        } else {
            Err(format!("Download failed with status: {:?}", status))
        }
    }

    async fn parse_ytdlp_progress(&self, line: &str, video_id: &str) {
        if line.contains("[download]") && line.contains("%") {
            let parts: Vec<&str> = line.split_whitespace().collect();

            let mut progress = 0.0;
            let mut speed = String::new();
            let mut eta = String::new();
            let mut file_size = String::new();

            for (i, part) in parts.iter().enumerate() {
                if part.contains("%") {
                    if let Some(p) = part.replace("%", "").parse::<f64>().ok() {
                        progress = p / 100.0;
                    }
                } else if part.contains("MiB") || part.contains("KiB") {
                    if i > 0 && parts[i - 1] == "of" {
                        file_size = part.to_string();
                    } else if part.contains("/s") {
                        speed = part.to_string();
                    }
                } else if *part == "ETA" && i + 1 < parts.len() {
                    eta = parts[i + 1].to_string();
                }
            }

            let mut active = self.active_downloads.lock().await;
            if let Some(dl) = active.get_mut(video_id) {
                dl.progress = progress;
                dl.speed = speed;
                dl.eta = eta;
                dl.file_size = file_size;
            }

            drop(active);
            self.emit_downloads_update().await;
        }
    }

    async fn mark_download_completed(&self, track: &YTVideoInfo) -> Result<(), String> {
        // Remove from active
        {
            let mut active = self.active_downloads.lock().await;
            active.remove(&track.id);
        }

        // Add to completed
        {
            let mut completed = self.completed_downloads.lock().await;
            if !completed.contains(&track.id) {
                completed.push(track.id.clone());
            }
        }

        // Save metadata
        self.save_track_metadata(track).await?;

        self.analytics.track("track_downloaded");
        self.emit_downloads_update().await;
        Ok(())
    }

    async fn update_download_error(&self, video_id: &str, error: &str) {
        let mut active = self.active_downloads.lock().await;
        if let Some(dl) = active.get_mut(video_id) {
            dl.error = Some(error.to_string());
        }
        drop(active);
        self.analytics.track("download_failed");
        self.emit_downloads_update().await;
    }

    async fn save_track_metadata(&self, track: &YTVideoInfo) -> Result<(), String> {
        let downloads_dir = self.downloads_dir.lock().await.clone();
        let metadata_path = downloads_dir.join(format!("{}_metadata.json", track.id));

        let metadata = serde_json::json!({
            "id": track.id,
            "title": track.title,
            "uploader": track.uploader,
            "duration": track.duration,
            "thumbnail_url": track.thumbnail_url,
            "description": track.description,
            "download_date": unix_timestamp(),
        });

        let json = serde_json::to_string_pretty(&metadata).map_err(|e| e.to_string())?;
        std::fs::write(&metadata_path, json).map_err(|e| e.to_string())?;

        Ok(())
    }

    pub async fn get_active_downloads(&self) -> Vec<DownloadProgress> {
        self.active_downloads
            .lock()
            .await
            .values()
            .cloned()
            .collect()
    }

    pub async fn get_downloaded_tracks(&self) -> Vec<DownloadedTrack> {
        let completed = self.completed_downloads.lock().await;
        let downloads_dir = self.downloads_dir.lock().await.clone();

        // Scan the directory once and look up each track's file by id, instead of
        // re-scanning the whole directory per track (O(n) -> O(n^2) for n downloads).
        let audio_files = scan_audio_files_by_id(&downloads_dir);
        let mut tracks = Vec::new();

        for video_id in completed.iter() {
            let metadata_path = downloads_dir.join(format!("{}_metadata.json", video_id));

            if let Ok(json) = std::fs::read_to_string(&metadata_path) {
                if let Ok(metadata) = serde_json::from_str::<serde_json::Value>(&json) {
                    let video_info = YTVideoInfo {
                        id: metadata["id"].as_str().unwrap_or("").to_string(),
                        title: metadata["title"].as_str().unwrap_or("").to_string(),
                        uploader: metadata["uploader"].as_str().unwrap_or("").to_string(),
                        duration: metadata["duration"].as_i64().unwrap_or(0),
                        thumbnail_url: metadata["thumbnail_url"].as_str().map(|s| s.to_string()),
                        audio_url: None,
                        description: metadata["description"].as_str().map(|s| s.to_string()),
                    };

                    if let Some(file_path) = audio_files.get(video_id) {
                        let file_size = std::fs::metadata(file_path)
                            .map(|m| m.len() as i64)
                            .unwrap_or(0);

                        tracks.push(DownloadedTrack {
                            video_info,
                            file_path: file_path.to_string_lossy().to_string(),
                            file_size,
                            download_date: metadata["download_date"].as_i64().unwrap_or(0),
                        });
                    }
                }
            }
        }

        tracks
    }

    pub async fn get_storage_used(&self) -> i64 {
        let downloads_dir = self.downloads_dir.lock().await.clone();
        calculate_directory_size(&downloads_dir)
    }

    pub async fn is_downloaded(&self, video_id: &str) -> bool {
        self.completed_downloads.lock().await.contains(&video_id.to_string())
    }

    pub async fn get_downloaded_file_path(&self, video_id: &str) -> Option<String> {
        if !self.is_downloaded(video_id).await {
            return None;
        }

        let downloads_dir = self.downloads_dir.lock().await.clone();
        find_audio_file(&downloads_dir, video_id).map(|p| p.to_string_lossy().to_string())
    }

    pub async fn get_downloads_directory(&self) -> String {
        self.downloads_dir
            .lock()
            .await
            .to_string_lossy()
            .to_string()
    }

    pub async fn set_audio_quality(&self, quality: String) -> Result<(), String> {
        *self.audio_quality.lock().await = quality;
        Ok(())
    }

    pub async fn get_audio_quality(&self) -> String {
        self.audio_quality.lock().await.clone()
    }

    pub async fn delete_download(&self, video_id: &str) -> Result<(), String> {
        let downloads_dir = self.downloads_dir.lock().await.clone();

        // Delete audio file
        if let Some(file_path) = find_audio_file(&downloads_dir, video_id) {
            std::fs::remove_file(&file_path).map_err(|e| e.to_string())?;
        }

        // Delete metadata
        let metadata_path = downloads_dir.join(format!("{}_metadata.json", video_id));
        if metadata_path.exists() {
            std::fs::remove_file(&metadata_path).map_err(|e| e.to_string())?;
        }

        // Remove from completed list
        {
            let mut completed = self.completed_downloads.lock().await;
            completed.retain(|id| id != video_id);
        }

        self.emit_downloads_update().await;
        Ok(())
    }

    pub async fn cancel_download(&self, video_id: &str) -> Result<(), String> {
        let mut active = self.active_downloads.lock().await;
        active.remove(video_id);
        drop(active);

        self.emit_downloads_update().await;
        Ok(())
    }

    async fn emit_downloads_update(&self) {
        if let Some(handle) = self.app_handle.lock().await.as_ref() {
            let _ = handle.emit("downloads-updated", ());
        }
    }
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '.')
        .collect()
}

fn find_audio_file(dir: &PathBuf, video_id: &str) -> Option<PathBuf> {
    let extensions = ["flac", "m4a", "webm", "mp3", "aac", "ogg"];

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if extensions.contains(&ext.to_str().unwrap_or(""))
                    && path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.contains(video_id))
                        .unwrap_or(false)
                {
                    return Some(path);
                }
            }
        }
    }

    None
}

// One pass over the downloads directory building an id -> path map, instead of
// re-scanning the whole directory per track (which turns "list N downloads"
// into an O(N^2) directory walk). Filenames are written as
// "[{video_id}] {title} - {uploader}.ext", so the id is read straight out of
// the leading brackets rather than doing a substring search per candidate.
fn scan_audio_files_by_id(dir: &PathBuf) -> HashMap<String, PathBuf> {
    let extensions = ["flac", "m4a", "webm", "mp3", "aac", "ogg"];
    let mut by_id = HashMap::new();

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
                continue;
            };
            if !extensions.contains(&ext) {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if let Some(id) = name.strip_prefix('[').and_then(|rest| rest.split(']').next()) {
                by_id.insert(id.to_string(), path);
            }
        }
    }

    by_id
}

fn calculate_directory_size(dir: &PathBuf) -> i64 {
    let mut total = 0i64;

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_file() {
                    total += metadata.len() as i64;
                }
            }
        }
    }

    total
}

#[cfg(test)]
mod tests {
    use super::*;

    // Track/uploader titles come straight from YouTube and become filenames on
    // disk (`[{id}] {title} - {uploader}.ext`) -- sanitize_filename is the only
    // thing standing between arbitrary video metadata and the filesystem.

    #[test]
    fn keeps_plain_ascii_titles_unchanged() {
        assert_eq!(sanitize_filename("Hello World"), "Hello World");
    }

    #[test]
    fn strips_path_separators_so_traversal_is_impossible() {
        // Slashes/backslashes are removed entirely (not collapsed into a
        // no-op), so the result can never escape the downloads directory
        // regardless of how many ../ segments are in the input.
        let sanitized = sanitize_filename("../../etc/passwd");
        assert!(!sanitized.contains('/'));
        assert!(!sanitized.contains('\\'));

        let sanitized_win = sanitize_filename("..\\..\\Windows\\System32");
        assert!(!sanitized_win.contains('/'));
        assert!(!sanitized_win.contains('\\'));
    }

    #[test]
    fn strips_punctuation_that_extensions_use() {
        assert_eq!(sanitize_filename("Song: Part 2 (Remix)!"), "Song Part 2 Remix");
    }

    #[test]
    fn keeps_unicode_letters() {
        // char::is_alphanumeric is Unicode-aware, so non-ASCII titles (common
        // for international YouTube content) aren't mangled into nothing.
        assert_eq!(sanitize_filename("日本語 Song"), "日本語 Song");
    }

    #[test]
    fn never_produces_a_path_separator_from_arbitrary_input() {
        for input in [
            "/etc/passwd",
            "C:\\Windows\\System32\\config",
            "title/with/slashes",
            "title\\with\\backslashes",
        ] {
            let sanitized = sanitize_filename(input);
            assert!(
                !sanitized.contains('/') && !sanitized.contains('\\'),
                "sanitize_filename({:?}) produced a path separator: {:?}",
                input,
                sanitized
            );
        }
    }

    // Real downloaded filenames look like "[{video_id}] {title} - {uploader}.ext".
    fn make_download_file(dir: &std::path::Path, video_id: &str, ext: &str) {
        std::fs::write(
            dir.join(format!("[{}] Some Title - Some Uploader.{}", video_id, ext)),
            b"fake audio bytes",
        )
        .unwrap();
    }

    #[test]
    fn find_audio_file_matches_on_video_id_and_extension() {
        let dir = tempfile::tempdir().unwrap();
        make_download_file(dir.path(), "abc123", "mp3");

        let found = find_audio_file(&dir.path().to_path_buf(), "abc123");
        assert!(found.is_some());
        assert!(found.unwrap().to_string_lossy().contains("abc123"));
    }

    #[test]
    fn find_audio_file_ignores_non_audio_extensions() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("[abc123] notes.txt"), b"not audio").unwrap();

        assert!(find_audio_file(&dir.path().to_path_buf(), "abc123").is_none());
    }

    #[test]
    fn find_audio_file_returns_none_for_unknown_id() {
        let dir = tempfile::tempdir().unwrap();
        make_download_file(dir.path(), "abc123", "mp3");

        assert!(find_audio_file(&dir.path().to_path_buf(), "not-there").is_none());
    }

    #[test]
    fn scan_audio_files_by_id_maps_every_download_by_its_bracketed_id() {
        let dir = tempfile::tempdir().unwrap();
        make_download_file(dir.path(), "id1", "mp3");
        make_download_file(dir.path(), "id2", "webm");
        make_download_file(dir.path(), "id3", "flac");

        let map = scan_audio_files_by_id(&dir.path().to_path_buf());

        assert_eq!(map.len(), 3);
        assert!(map.contains_key("id1"));
        assert!(map.contains_key("id2"));
        assert!(map.contains_key("id3"));
    }

    #[test]
    fn scan_audio_files_by_id_skips_metadata_json_and_unknown_extensions() {
        let dir = tempfile::tempdir().unwrap();
        make_download_file(dir.path(), "id1", "mp3");
        std::fs::write(dir.path().join("id1_metadata.json"), b"{}").unwrap();
        std::fs::write(dir.path().join("id2.txt"), b"not audio").unwrap();

        let map = scan_audio_files_by_id(&dir.path().to_path_buf());

        assert_eq!(map.len(), 1);
        assert!(map.contains_key("id1"));
    }

    #[test]
    fn scan_audio_files_by_id_on_empty_dir_returns_empty_map() {
        let dir = tempfile::tempdir().unwrap();
        assert!(scan_audio_files_by_id(&dir.path().to_path_buf()).is_empty());
    }

    #[test]
    fn calculate_directory_size_sums_file_sizes_only() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.mp3"), vec![0u8; 100]).unwrap();
        std::fs::write(dir.path().join("b.mp3"), vec![0u8; 250]).unwrap();
        std::fs::create_dir(dir.path().join("subdir")).unwrap();

        let total = calculate_directory_size(&dir.path().to_path_buf());
        assert_eq!(total, 350);
    }

    #[test]
    fn calculate_directory_size_of_empty_dir_is_zero() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(calculate_directory_size(&dir.path().to_path_buf()), 0);
    }
}
