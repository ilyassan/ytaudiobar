use crate::models::{YTVideoInfo, YTPlaylistInfo, YTPlaylistPreview};
use crate::ytdlp_installer::YTDLPInstaller;
use crate::command_utils::command_no_window;
use serde_json::Value;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Child;
use rand::seq::SliceRandom;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::sync::LazyLock;

// Global search process manager
static SEARCH_PROCESS: LazyLock<Arc<Mutex<Option<Child>>>> = LazyLock::new(|| Arc::new(Mutex::new(None)));

// Caps how many tracks we fetch from a single playlist — some auto-generated
// "Uploads from X" playlists run into the thousands, which would be slow to
// fetch and unwieldy to render as one unvirtualized list.
const MAX_PLAYLIST_TRACKS: usize = 300;

// YouTube bot bypass methods (in order of escalation)
#[derive(Debug, Clone, Copy)]
pub enum YouTubeBotBypassMethod {
    None,                // No bypass - normal yt-dlp behavior
    RateLimit,           // Rate limiting to appear human
    UserAgentRotation,   // Rotate user agents with headers
    GeoBypass,           // Geo-bypass with player skip
    CookiesFromBrowser,  // Last resort: Use browser cookies
}

pub struct YTDLPManager;

impl YTDLPManager {
    pub fn new() -> Self {
        Self
    }

    // Detect default browser for cookie extraction
    fn detect_default_browser() -> &'static str {
        #[cfg(target_os = "windows")]
        {
            // On Windows, try common browsers in order
            if std::path::Path::new(&format!("{}\\Google\\Chrome\\User Data",
                std::env::var("LOCALAPPDATA").unwrap_or_default())).exists() {
                return "chrome";
            }
            if std::path::Path::new(&format!("{}\\Microsoft\\Edge\\User Data",
                std::env::var("LOCALAPPDATA").unwrap_or_default())).exists() {
                return "edge";
            }
            "chrome" // Default fallback
        }
        #[cfg(target_os = "linux")]
        {
            "chrome" // Most common on Linux
        }
        #[cfg(target_os = "macos")]
        {
            "safari" // Default on macOS
        }
    }

    // Build bypass arguments based on method
    pub(crate) fn build_bypass_args(method: YouTubeBotBypassMethod) -> Vec<String> {
        let mut args = Vec::new();

        match method {
            YouTubeBotBypassMethod::None => {
                // Method 0: No bypass - normal yt-dlp behavior
                println!("🎯 Using normal yt-dlp (no bypass)");
                // Return empty args - just use default yt-dlp behavior
                return args;
            }
            YouTubeBotBypassMethod::RateLimit => {
                // Method 1: Rate limiting with delays to appear human-like
                args.push("--sleep-interval".to_string());
                args.push("2".to_string());
                args.push("--max-sleep-interval".to_string());
                args.push("8".to_string());
                args.push("--sleep-subtitles".to_string());
                args.push("1".to_string());
                args.push("--extractor-args".to_string());
                args.push("youtube:player_skip=configs,webpage".to_string());
                println!("⏱️ Using rate limiting bypass method");
            }
            YouTubeBotBypassMethod::UserAgentRotation => {
                // Method 2: User-Agent rotation with realistic headers
                let user_agents = vec![
                    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
                    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/119.0.0.0 Safari/537.36",
                    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
                ];
                let selected_ua = user_agents.choose(&mut rand::thread_rng()).unwrap_or(&user_agents[0]);
                args.push("--user-agent".to_string());
                args.push(selected_ua.to_string());
                args.push("--referer".to_string());
                args.push("https://www.youtube.com/".to_string());
                args.push("--add-header".to_string());
                args.push("Accept-Language:en-US,en;q=0.9".to_string());
                println!("🕸️ Using user-agent rotation bypass method");
            }
            YouTubeBotBypassMethod::GeoBypass => {
                // Method 3: Advanced geo-bypass with proxy rotation
                args.push("--geo-bypass-country".to_string());
                args.push("US".to_string());
                args.push("--extractor-args".to_string());
                args.push("youtube:player_skip=configs,js".to_string());
                args.push("--sleep-requests".to_string());
                args.push("1".to_string());
                args.push("--user-agent".to_string());
                args.push("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".to_string());
                println!("🌍 Using geo-bypass method");
            }
            YouTubeBotBypassMethod::CookiesFromBrowser => {
                // Method 4 (Last resort): Extract cookies from signed-in browser
                let browser = Self::detect_default_browser();
                args.push("--cookies-from-browser".to_string());
                args.push(browser.to_string());
                args.push("--extractor-args".to_string());
                args.push("youtube:skip=dash,hls".to_string());
                println!("🍪 Using browser cookies bypass method (browser: {}) - LAST RESORT", browser);
            }
        }

        // Common anti-detection arguments for all methods (except None)
        args.push("--no-check-certificate".to_string());
        args.push("--geo-bypass".to_string());

        args
    }

    // Try bypass methods in sequence until one works
    // Order: None (normal) -> RateLimit -> UserAgentRotation -> GeoBypass -> CookiesFromBrowser (last resort)
    async fn try_with_bypass<F, T>(operation: F) -> Result<T, String>
    where
        F: Fn(YouTubeBotBypassMethod) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, String>> + Send>>,
    {
        let methods = vec![
            YouTubeBotBypassMethod::None,
            YouTubeBotBypassMethod::RateLimit,
            YouTubeBotBypassMethod::UserAgentRotation,
            YouTubeBotBypassMethod::GeoBypass,
            YouTubeBotBypassMethod::CookiesFromBrowser,
        ];

        for (i, method) in methods.iter().enumerate() {
            println!("🔄 Attempt {}/{}: {:?}", i + 1, methods.len(), method);
            match operation(*method).await {
                Ok(result) => {
                    println!("✅ Success with method: {:?}", method);
                    return Ok(result);
                }
                Err(e) => {
                    println!("⚠️ Method {:?} failed: {}", method, e);
                    if i == methods.len() - 1 {
                        return Err(format!("All methods failed. Last error: {}", e));
                    }
                    println!("⏭️ Trying next method...");
                }
            }
        }

        Err("All bypass methods exhausted".to_string())
    }

    pub async fn search(&self, query: String) -> Result<Vec<YTVideoInfo>, String> {
        let search_query = format!("ytsearch10:{}", query);

        // Try with bypass methods
        Self::try_with_bypass(|bypass_method| {
            let search_query = search_query.clone();
            Box::pin(async move {
                Self::search_with_method(search_query, bypass_method).await
            })
        }).await
    }

    pub async fn search_playlists(&self, query: String) -> Result<Vec<YTPlaylistInfo>, String> {
        Self::try_with_bypass(|bypass_method| {
            let query = query.clone();
            Box::pin(async move {
                Self::search_playlists_with_method(query, bypass_method).await
            })
        }).await
    }

    async fn search_playlists_with_method(query: String, bypass_method: YouTubeBotBypassMethod) -> Result<Vec<YTPlaylistInfo>, String> {
        let ytdlp_path = Self::get_ytdlp_path();
        let encoded_query = Self::percent_encode_query(&query);
        // sp=EgIQAw%3D%3D is YouTube's own (undocumented) search-filter token for "Type: Playlist"
        let search_url = format!("https://www.youtube.com/results?search_query={}&sp=EgIQAw%3D%3D", encoded_query);
        let bypass_args = Self::build_bypass_args(bypass_method);

        let mut args = vec![
            "--flat-playlist".to_string(),
            "--dump-single-json".to_string(),
            "--playlist-end".to_string(),
            "10".to_string(),
            "--no-warnings".to_string(),
            "--ignore-errors".to_string(),
        ];
        args.extend(bypass_args);
        args.push(search_url);

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

        let output = command_no_window(&ytdlp_path)
            .args(&args_refs)
            .env("PYTHONIOENCODING", "utf-8")
            .env("LC_ALL", "C.UTF-8")
            .output()
            .await
            .map_err(|e| format!("Failed to search playlists: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Playlist search failed: {}", stderr.trim()));
        }

        let json: Value = serde_json::from_slice(&output.stdout)
            .map_err(|e| format!("Failed to parse playlist search results: {}", e))?;

        let playlists: Vec<YTPlaylistInfo> = json
            .get("entries")
            .and_then(|v| v.as_array())
            .map(|entries| entries.iter().filter_map(Self::parse_playlist_info).collect())
            .unwrap_or_default();

        if playlists.is_empty() {
            return Err("No playlists found".to_string());
        }

        Ok(playlists)
    }

    // Minimal percent-encoder for a URL query parameter — avoids pulling in an
    // extra crate for the one search-query string we need to escape here.
    fn percent_encode_query(input: &str) -> String {
        let mut encoded = String::with_capacity(input.len());
        for byte in input.as_bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    encoded.push(*byte as char);
                }
                _ => {
                    encoded.push_str(&format!("%{:02X}", byte));
                }
            }
        }
        encoded
    }

    fn parse_playlist_info(json: &Value) -> Option<YTPlaylistInfo> {
        // Only accept playlist-shaped entries — the sp= filter is unofficial and can
        // occasionally leak videos/channels through, so we double-check the entry type.
        let ie_key = json.get("ie_key").and_then(|v| v.as_str()).unwrap_or("");
        if ie_key != "YoutubeTab" {
            return None;
        }

        let id = json.get("id")?.as_str()?.to_string();

        Some(YTPlaylistInfo {
            id: id.clone(),
            title: json.get("title")?.as_str()?.to_string(),
            thumbnail_url: json
                .get("thumbnails")
                .and_then(|v| v.as_array())
                .and_then(|arr| arr.last()) // last entry is the highest-resolution thumbnail
                .and_then(|thumb| thumb.get("url"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            url: json
                .get("url")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("https://www.youtube.com/playlist?list={}", id)),
        })
    }

    pub async fn get_playlist_preview(&self, playlist_url: String) -> Result<YTPlaylistPreview, String> {
        Self::try_with_bypass(|bypass_method| {
            let playlist_url = playlist_url.clone();
            Box::pin(async move {
                Self::get_playlist_preview_with_method(playlist_url, bypass_method).await
            })
        }).await
    }

    async fn get_playlist_preview_with_method(playlist_url: String, bypass_method: YouTubeBotBypassMethod) -> Result<YTPlaylistPreview, String> {
        Self::cancel_search().await;

        let ytdlp_path = Self::get_ytdlp_path();
        let bypass_args = Self::build_bypass_args(bypass_method);

        let mut args = vec![
            "--flat-playlist".to_string(),
            "-j".to_string(),
            "--no-warnings".to_string(),
            "--ignore-errors".to_string(),
            "--playlist-end".to_string(),
            MAX_PLAYLIST_TRACKS.to_string(),
        ];
        args.extend(bypass_args);
        args.push(playlist_url);

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

        let mut child = command_no_window(&ytdlp_path)
            .args(&args_refs)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("PYTHONIOENCODING", "utf-8")
            .env("LC_ALL", "C.UTF-8")
            .spawn()
            .map_err(|e| format!("Failed to spawn yt-dlp: {}. Make sure yt-dlp is installed.", e))?;

        let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
        let mut stderr = child.stderr.take().ok_or("Failed to capture stderr")?;

        {
            let mut search_process = SEARCH_PROCESS.lock().await;
            *search_process = Some(child);
        }

        let stderr_handle = tokio::spawn(async move {
            let mut buffer = Vec::new();
            use tokio::io::AsyncReadExt;
            let _ = stderr.read_to_end(&mut buffer).await;
            String::from_utf8_lossy(&buffer).to_string()
        });

        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        let mut tracks = Vec::new();
        let mut playlist_meta: Option<(String, String, String, i64)> = None;

        while let Ok(Some(line)) = lines.next_line().await {
            if let Ok(json) = serde_json::from_str::<Value>(&line) {
                if playlist_meta.is_none() {
                    let pid = json.get("playlist_id").and_then(|v| v.as_str()).unwrap_or_default().to_string();
                    let ptitle = json.get("playlist_title").and_then(|v| v.as_str()).unwrap_or("Untitled Playlist").to_string();
                    let puploader = json.get("playlist_uploader").and_then(|v| v.as_str()).unwrap_or("Unknown").to_string();
                    let pcount = json.get("playlist_count").and_then(|v| v.as_i64()).unwrap_or(0);
                    playlist_meta = Some((pid, ptitle, puploader, pcount));
                }

                if let Some(mut track) = Self::parse_video_info(&json) {
                    // Flat-playlist entries don't carry a per-video uploader — fall back
                    // to the playlist owner so the track list isn't full of "Unknown".
                    if track.uploader == "Unknown" {
                        if let Some((_, _, ref uploader, _)) = playlist_meta {
                            track.uploader = uploader.clone();
                        }
                    }
                    tracks.push(track);
                }
            }
        }

        let exit_status = {
            let mut search_process = SEARCH_PROCESS.lock().await;
            if let Some(mut child) = search_process.take() {
                child.wait().await
            } else {
                return Err("Playlist fetch was cancelled".to_string());
            }
        };
        exit_status.map_err(|e| format!("yt-dlp process error: {}", e))?;

        if tracks.is_empty() {
            let stderr_output = stderr_handle.await.unwrap_or_default();
            let error_msg = if !stderr_output.is_empty() {
                format!("Playlist is empty or unavailable. yt-dlp stderr: {}", stderr_output.trim())
            } else {
                "Playlist is empty or unavailable".to_string()
            };
            return Err(error_msg);
        }

        let (id, title, uploader, count) = playlist_meta.unwrap_or_else(|| {
            (String::new(), "Untitled Playlist".to_string(), "Unknown".to_string(), 0)
        });

        Ok(YTPlaylistPreview {
            id,
            title,
            uploader,
            track_count: if count > 0 { count } else { tracks.len() as i64 },
            tracks,
        })
    }

    async fn search_with_method(search_query: String, bypass_method: YouTubeBotBypassMethod) -> Result<Vec<YTVideoInfo>, String> {
        // Cancel any existing search process first
        Self::cancel_search().await;

        let ytdlp_path = Self::get_ytdlp_path();
        let bypass_args = Self::build_bypass_args(bypass_method);

        let mut args = vec![
            "--flat-playlist".to_string(),  // Fast search - skip detailed metadata
            "-j".to_string(),                // JSON output
            "--no-warnings".to_string(),
            "--ignore-errors".to_string(),
        ];
        args.extend(bypass_args);
        args.push(search_query);

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

        // Execute yt-dlp binary directly
        let mut child = command_no_window(&ytdlp_path)
            .args(&args_refs)
            .stdin(Stdio::null())  // Close stdin - don't wait for input
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())  // Capture stderr for error messages
            .env("PYTHONIOENCODING", "utf-8")  // Help Python initialize encoding
            .env("LC_ALL", "C.UTF-8")  // Set locale for Python
            .spawn()
            .map_err(|e| format!("Failed to spawn yt-dlp: {}. Make sure yt-dlp is installed.", e))?;

        let stdout = child
            .stdout
            .take()
            .ok_or("Failed to capture stdout")?;

        let mut stderr = child
            .stderr
            .take()
            .ok_or("Failed to capture stderr")?;

        // Store the child process so it can be cancelled
        {
            let mut search_process = SEARCH_PROCESS.lock().await;
            *search_process = Some(child);
        }

        // Spawn task to read stderr in background
        let stderr_handle = tokio::spawn(async move {
            let mut buffer = Vec::new();
            use tokio::io::AsyncReadExt;
            let _ = stderr.read_to_end(&mut buffer).await;
            String::from_utf8_lossy(&buffer).to_string()
        });

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

        // Wait for process to complete and clean up
        let exit_status = {
            let mut search_process = SEARCH_PROCESS.lock().await;
            if let Some(mut child) = search_process.take() {
                child.wait().await
            } else {
                return Err("Search process was cancelled".to_string());
            }
        };

        exit_status.map_err(|e| format!("yt-dlp process error: {}", e))?;

        if results.is_empty() {
            let stderr_output = match stderr_handle.await {
                Ok(err) => err,
                Err(_) => String::new(),
            };
            let error_msg = if !stderr_output.is_empty() {
                format!("No results found. yt-dlp stderr: {}", stderr_output.trim())
            } else {
                "No results found".to_string()
            };
            return Err(error_msg);
        }

        Ok(results)
    }

    // Cancel the currently running search
    pub async fn cancel_search() {
        let mut search_process = SEARCH_PROCESS.lock().await;
        if let Some(mut child) = search_process.take() {
            println!("🚫 Cancelling ongoing search process...");
            let _ = child.kill().await; // Kill the process
            println!("✅ Search process cancelled");
        }
    }

    pub async fn get_audio_url(&self, video_id: String) -> Result<(String, String), String> {
        // Try with bypass methods
        Self::try_with_bypass(|bypass_method| {
            let video_id = video_id.clone();
            Box::pin(async move {
                Self::get_audio_url_with_method(video_id, bypass_method).await
            })
        }).await
    }

    async fn get_audio_url_with_method(video_id: String, bypass_method: YouTubeBotBypassMethod) -> Result<(String, String), String> {
        let ytdlp_path = Self::get_ytdlp_path();
        let url = format!("https://www.youtube.com/watch?v={}", video_id);
        let bypass_args = Self::build_bypass_args(bypass_method);

        let mut args = vec![
            "--dump-json".to_string(),
            "-f".to_string(),
            "bestaudio[ext=webm]/bestaudio[ext=opus]/bestaudio".to_string(),
            "--no-warnings".to_string(),
        ];
        args.extend(bypass_args);
        args.push(url);

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

        let output = command_no_window(&ytdlp_path)
            .args(&args_refs)
            .output()
            .await
            .map_err(|e| format!("Failed to get audio URL: {}", e))?;

        if !output.status.success() {
            return Err("Failed to extract audio URL from YouTube".to_string());
        }

        let json: Value = serde_json::from_slice(&output.stdout)
            .map_err(|e| format!("Failed to parse yt-dlp output: {}", e))?;

        let audio_url = json.get("url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "No audio URL found in response".to_string())?;

        let ext = json.get("ext")
            .and_then(|v| v.as_str())
            .unwrap_or("m4a")
            .to_string();

        Ok((audio_url, ext))
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
            duration: json.get("duration")
                .and_then(|v| v.as_f64())
                .map(|f| f as i64)
                .unwrap_or(0),
            thumbnail_url: json
                .get("thumbnails")
                .and_then(|v| v.as_array())
                .and_then(|arr| arr.first())
                .and_then(|thumb| thumb.get("url"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| {
                    // Fallback to single "thumbnail" field (for full metadata)
                    json.get("thumbnail")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                }),
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

        command_no_window(&ytdlp_path)
            .arg("--version")
            .output()
            .await
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    pub async fn update_ytdlp(&self) -> Result<(), String> {
        let ytdlp_path = Self::get_ytdlp_path();

        let output = command_no_window(&ytdlp_path)
            .arg("-U")
            .output()
            .await
            .map_err(|e| format!("Failed to update yt-dlp: {}", e))?;

        if !output.status.success() {
            return Err("Failed to update yt-dlp".to_string());
        }

        Ok(())
    }

    // Fetch basic info for a single video by ID — fast, same shallow flags as search
    pub async fn get_video_info_fast(&self, video_id: String) -> Result<YTVideoInfo, String> {
        let ytdlp_path = Self::get_ytdlp_path();
        let url = format!("https://www.youtube.com/watch?v={}", video_id);

        let output = command_no_window(&ytdlp_path)
            .args(["--flat-playlist", "-j", "--no-warnings", &url])
            .output()
            .await
            .map_err(|e| format!("Failed to get video info: {}", e))?;

        if !output.status.success() {
            return Err("Failed to fetch video info".to_string());
        }

        let json: Value = serde_json::from_slice(&output.stdout)
            .map_err(|e| format!("Failed to parse video info: {}", e))?;

        Self::parse_video_info(&json)
            .ok_or_else(|| "Failed to parse video info".to_string())
    }

    // Fetch detailed metadata for a single video (for lazy loading durations)
    pub async fn get_video_details(&self, video_id: String) -> Result<YTVideoInfo, String> {
        let ytdlp_path = Self::get_ytdlp_path();
        let url = format!("https://www.youtube.com/watch?v={}", video_id);

        let args = vec![
            "--dump-json",
            "--no-warnings",
            &url,
        ];

        let output = command_no_window(&ytdlp_path)
            .args(&args)
            .output()
            .await
            .map_err(|e| format!("Failed to get video details: {}", e))?;

        if !output.status.success() {
            return Err("Failed to extract video details".to_string());
        }

        let json: Value = serde_json::from_slice(&output.stdout)
            .map_err(|e| format!("Failed to parse video details: {}", e))?;

        Self::parse_video_info(&json)
            .ok_or_else(|| "Failed to parse video info".to_string())
    }
}
