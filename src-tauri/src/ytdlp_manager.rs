use crate::models::{YTVideoInfo, YTPlaylistInfo, YTPlaylistPreview};
use crate::ytdlp_installer::YTDLPInstaller;
use crate::command_utils::{command_no_window, friendly_ytdlp_error};
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

/// Marks a failure as "the user cancelled", not "this bypass method didn't
/// work". `try_with_bypass` escalates through increasingly aggressive methods
/// on failure, so without a way to tell the two apart, cancelling a search
/// kicked off up to four *more* yt-dlp runs -- ending with the slow
/// rate-limited and cookies-from-browser attempts.
pub const SEARCH_CANCELLED: &str = "Search cancelled";

pub struct YTDLPManager;

impl YTDLPManager {
    pub fn new() -> Self {
        Self
    }

    // Detect default browser for cookie extraction
    fn detect_default_browser() -> &'static str {
        #[cfg(target_os = "windows")]
        {
            // Firefox first, deliberately: since Chrome 127 (mid-2024)
            // shipped "App-Bound Encryption," no external process can
            // decrypt Chrome/Edge's cookie database anymore, even running as
            // the exact same Windows user -- confirmed dead, not just flaky
            // (yt-dlp maintainers have no fix planned, see
            // github.com/yt-dlp/yt-dlp/issues/10927). This bypass rung is the
            // last resort in the ladder, so reaching it with Chrome/Edge on a
            // modern install means a guaranteed "Failed to decrypt with
            // DPAPI" failure. Firefox stores cookies in a plain, undecrypted
            // SQLite file and is unaffected -- use it whenever present.
            if std::path::Path::new(&format!("{}\\Mozilla\\Firefox\\Profiles",
                std::env::var("APPDATA").unwrap_or_default())).exists() {
                return "firefox";
            }
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
                // Method 1: Skip the slow YouTube player config/webpage fetch.
                // --sleep-interval was removed: it sleeps before every download
                // (even single-item), making the UI show "Starting..." for 2-8s
                // then jump straight to done -- it doesn't meaningfully help with
                // bot detection, player_skip does the actual work.
                args.push("--extractor-args".to_string());
                args.push("youtube:player_skip=configs,webpage".to_string());
                println!("⏱️ Using player-skip bypass method");
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
                // Method 3: Geo-bypass with JS player skip and a browser UA.
                args.push("--geo-bypass-country".to_string());
                args.push("US".to_string());
                args.push("--extractor-args".to_string());
                args.push("youtube:player_skip=configs,js".to_string());
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

    // Try bypass methods in sequence until one works — full ladder for downloads.
    // Order: None -> RateLimit -> UserAgentRotation -> GeoBypass -> CookiesFromBrowser (last resort)
    pub(crate) async fn try_with_bypass<F, T>(operation: F) -> Result<T, String>
    where
        F: Fn(YouTubeBotBypassMethod) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, String>> + Send>>,
    {
        // CookiesFromBrowser reads from ~/Library/Containers/com.apple.Safari (or
        // Chrome's container), which is TCC-protected on macOS -- the subprocess
        // always gets "Operation not permitted". Skip it there entirely.
        #[cfg(target_os = "macos")]
        let methods = vec![
            YouTubeBotBypassMethod::None,
            YouTubeBotBypassMethod::RateLimit,
            YouTubeBotBypassMethod::UserAgentRotation,
            YouTubeBotBypassMethod::GeoBypass,
        ];
        #[cfg(not(target_os = "macos"))]
        let methods = vec![
            YouTubeBotBypassMethod::None,
            YouTubeBotBypassMethod::RateLimit,
            YouTubeBotBypassMethod::UserAgentRotation,
            YouTubeBotBypassMethod::GeoBypass,
            YouTubeBotBypassMethod::CookiesFromBrowser,
        ];

        Self::run_bypass_ladder(methods, operation).await
    }

    // Search-specific bypass ladder: RateLimit is excluded because its sleep
    // flags (removed, but kept named for clarity) caused the search to stall
    // silently. For a ytsearch10: query even 1s of sleep per item adds up fast.
    pub(crate) async fn try_with_bypass_for_search<F, T>(operation: F) -> Result<T, String>
    where
        F: Fn(YouTubeBotBypassMethod) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, String>> + Send>>,
    {
        #[cfg(target_os = "macos")]
        let methods = vec![
            YouTubeBotBypassMethod::None,
            YouTubeBotBypassMethod::UserAgentRotation,
            YouTubeBotBypassMethod::GeoBypass,
        ];
        #[cfg(not(target_os = "macos"))]
        let methods = vec![
            YouTubeBotBypassMethod::None,
            YouTubeBotBypassMethod::UserAgentRotation,
            YouTubeBotBypassMethod::GeoBypass,
            YouTubeBotBypassMethod::CookiesFromBrowser,
        ];

        Self::run_bypass_ladder(methods, operation).await
    }

    async fn run_bypass_ladder<F, T>(methods: Vec<YouTubeBotBypassMethod>, operation: F) -> Result<T, String>
    where
        F: Fn(YouTubeBotBypassMethod) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, String>> + Send>>,
    {
        for (i, method) in methods.iter().enumerate() {
            println!("🔄 Attempt {}/{}: {:?}", i + 1, methods.len(), method);
            match operation(*method).await {
                Ok(result) => {
                    println!("✅ Success with method: {:?}", method);
                    return Ok(result);
                }
                Err(e) if e == SEARCH_CANCELLED => {
                    // Deliberate stop -- don't escalate to the next method.
                    println!("🚫 Search cancelled, not escalating");
                    return Err(e);
                }
                Err(e) => {
                    println!("⚠️ Method {:?} failed: {}", method, e);
                    if i == methods.len() - 1 {
                        return Err(friendly_ytdlp_error(&e));
                    }
                    println!("⏭️ Trying next method...");
                }
            }
        }

        Err("Search is unavailable right now. Please try again.".to_string())
    }

    pub async fn search(&self, query: String) -> Result<Vec<YTVideoInfo>, String> {
        let search_query = format!("ytsearch10:{}", query);

        Self::try_with_bypass_for_search(|bypass_method| {
            let search_query = search_query.clone();
            Box::pin(async move {
                Self::search_with_method(search_query, bypass_method).await
            })
        }).await
    }

    pub async fn search_playlists(&self, query: String) -> Result<Vec<YTPlaylistInfo>, String> {
        Self::try_with_bypass_for_search(|bypass_method| {
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
            "--extractor-args".to_string(),
            "youtube:player_skip=configs,webpage".to_string(),
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
            .map_err(|_| "Connection failed. Check your internet connection and try again.".to_string())?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(friendly_ytdlp_error(stderr.trim()));
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
        // playlist_url ends up as the final, unprefixed argv token passed to
        // yt-dlp. The frontend already only ever sends a real youtube.com
        // playlist URL (src/lib/youtube-url.ts), but that's not something the
        // backend can rely on -- any other caller of this Tauri command
        // (compromised update, a future stored-XSS, devtools) could pass an
        // arbitrary string, and yt-dlp's argparse treats a leading-dash token
        // as a flag rather than a positional argument (e.g. "--exec=...").
        // Requiring an http(s) URL here closes that off at the source
        // regardless of what calls this.
        if !playlist_url.starts_with("http://") && !playlist_url.starts_with("https://") {
            return Err("Invalid playlist URL".to_string());
        }

        Self::try_with_bypass_for_search(|bypass_method| {
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
            "--extractor-args".to_string(),
            "youtube:player_skip=configs,webpage".to_string(),
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
                return Err(SEARCH_CANCELLED.to_string());
            }
        };
        exit_status.map_err(|_| "An error occurred. Please try again.".to_string())?;

        if tracks.is_empty() {
            let stderr_output = stderr_handle.await.unwrap_or_default();
            let error_msg = if !stderr_output.is_empty() {
                friendly_ytdlp_error(&stderr_output)
            } else {
                "This playlist is empty or unavailable.".to_string()
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
            // Skip player config and webpage fetches: flat-playlist searches only need
            // basic metadata (title/id/duration) which comes from the search response
            // JSON, not the player. Cuts per-search time from ~37s to ~16s on macOS.
            "--extractor-args".to_string(),
            "youtube:player_skip=configs,webpage".to_string(),
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
                return Err(SEARCH_CANCELLED.to_string());
            }
        };

        exit_status.map_err(|_| "An error occurred. Please try again.".to_string())?;

        if results.is_empty() {
            let stderr_output = match stderr_handle.await {
                Ok(err) => err,
                Err(_) => String::new(),
            };
            let error_msg = if !stderr_output.is_empty() {
                friendly_ytdlp_error(&stderr_output)
            } else {
                "No results found.".to_string()
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

    // Fetch basic info for a single video by ID — fast, same shallow flags as search
    pub async fn get_video_info_fast(&self, video_id: String) -> Result<YTVideoInfo, String> {
        let ytdlp_path = Self::get_ytdlp_path();
        let url = format!("https://www.youtube.com/watch?v={}", video_id);

        let output = command_no_window(&ytdlp_path)
            .args([
                "--flat-playlist", "-j", "--no-warnings",
                "--extractor-args", "youtube:player_skip=configs,webpage",
                &url,
            ])
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

#[cfg(test)]
mod tests {
    use super::*;

    // The bot-bypass escalation ladder is the highest-risk untested logic in
    // this codebase -- a regression here silently breaks search/playback for
    // everyone. These tests pin down the exact flags each method is supposed
    // to add, so a future edit can't accidentally drop or rename one.

    #[test]
    fn none_method_adds_no_args() {
        assert!(YTDLPManager::build_bypass_args(YouTubeBotBypassMethod::None).is_empty());
    }

    #[test]
    fn rate_limit_uses_player_skip_without_sleep_delays() {
        let args = YTDLPManager::build_bypass_args(YouTubeBotBypassMethod::RateLimit);
        // Sleep flags were removed: they caused "Starting..." → instant-done UX
        // by sleeping before the download with no visible progress.
        assert!(!args.contains(&"--sleep-interval".to_string()));
        assert!(!args.contains(&"--max-sleep-interval".to_string()));
        // player_skip is the bypass that actually helps.
        assert!(args.contains(&"--extractor-args".to_string()));
        // Common anti-detection flags should be appended to every non-None method.
        assert!(args.contains(&"--no-check-certificate".to_string()));
        assert!(args.contains(&"--geo-bypass".to_string()));
    }

    #[test]
    fn user_agent_rotation_picks_a_known_user_agent() {
        let args = YTDLPManager::build_bypass_args(YouTubeBotBypassMethod::UserAgentRotation);
        let ua_index = args
            .iter()
            .position(|a| a == "--user-agent")
            .expect("--user-agent flag should be present");
        let ua_value = &args[ua_index + 1];
        assert!(
            ua_value.contains("Mozilla/5.0"),
            "expected a browser-shaped User-Agent, got: {}",
            ua_value
        );
    }

    #[test]
    fn geo_bypass_sets_country_and_extractor_args() {
        let args = YTDLPManager::build_bypass_args(YouTubeBotBypassMethod::GeoBypass);
        assert!(args.contains(&"--geo-bypass-country".to_string()));
        assert!(args.contains(&"US".to_string()));
        assert!(args.contains(&"--extractor-args".to_string()));
    }

    #[test]
    fn cookies_from_browser_is_last_resort_and_sets_cookie_flag() {
        let args = YTDLPManager::build_bypass_args(YouTubeBotBypassMethod::CookiesFromBrowser);
        assert!(args.contains(&"--cookies-from-browser".to_string()));
    }

    #[test]
    fn every_non_none_method_includes_common_anti_detection_flags() {
        for method in [
            YouTubeBotBypassMethod::RateLimit,
            YouTubeBotBypassMethod::UserAgentRotation,
            YouTubeBotBypassMethod::GeoBypass,
            YouTubeBotBypassMethod::CookiesFromBrowser,
        ] {
            let args = YTDLPManager::build_bypass_args(method);
            assert!(
                args.contains(&"--no-check-certificate".to_string()),
                "{:?} missing --no-check-certificate",
                method
            );
            assert!(
                args.contains(&"--geo-bypass".to_string()),
                "{:?} missing --geo-bypass",
                method
            );
        }
    }

    // parse_video_info turns yt-dlp's raw JSON output into our YTVideoInfo --
    // every field here is "trust yt-dlp's shape," which is exactly the kind of
    // assumption that breaks silently when yt-dlp changes its output format.

    #[test]
    fn parse_video_info_reads_all_fields_from_full_json() {
        let json = serde_json::json!({
            "id": "abc123",
            "title": "A Song",
            "uploader": "An Artist",
            "duration": 213.7,
            "thumbnails": [{"url": "https://example.com/thumb.jpg"}],
            "description": "A description"
        });

        let info = YTDLPManager::parse_video_info(&json).unwrap();
        assert_eq!(info.id, "abc123");
        assert_eq!(info.title, "A Song");
        assert_eq!(info.uploader, "An Artist");
        assert_eq!(info.duration, 213); // truncated, not rounded
        assert_eq!(info.thumbnail_url, Some("https://example.com/thumb.jpg".to_string()));
        assert_eq!(info.description, Some("A description".to_string()));
        assert_eq!(info.audio_url, None); // never populated by this parser
    }

    #[test]
    fn parse_video_info_returns_none_without_a_required_field() {
        assert!(YTDLPManager::parse_video_info(&serde_json::json!({ "title": "No ID" })).is_none());
        assert!(YTDLPManager::parse_video_info(&serde_json::json!({ "id": "no-title" })).is_none());
    }

    #[test]
    fn parse_video_info_defaults_missing_optional_fields() {
        let json = serde_json::json!({ "id": "abc123", "title": "A Song" });
        let info = YTDLPManager::parse_video_info(&json).unwrap();

        assert_eq!(info.uploader, "Unknown");
        assert_eq!(info.duration, 0);
        assert_eq!(info.thumbnail_url, None);
        assert_eq!(info.description, None);
    }

    #[test]
    fn parse_video_info_falls_back_to_singular_thumbnail_field() {
        let json = serde_json::json!({
            "id": "abc123",
            "title": "A Song",
            "thumbnail": "https://example.com/fallback.jpg"
        });

        let info = YTDLPManager::parse_video_info(&json).unwrap();
        assert_eq!(info.thumbnail_url, Some("https://example.com/fallback.jpg".to_string()));
    }

    #[test]
    fn parse_video_info_prefers_thumbnails_array_over_singular_field() {
        let json = serde_json::json!({
            "id": "abc123",
            "title": "A Song",
            "thumbnails": [{"url": "https://example.com/array.jpg"}],
            "thumbnail": "https://example.com/singular.jpg"
        });

        let info = YTDLPManager::parse_video_info(&json).unwrap();
        assert_eq!(info.thumbnail_url, Some("https://example.com/array.jpg".to_string()));
    }

    #[test]
    fn parse_video_info_handles_an_empty_thumbnails_array() {
        let json = serde_json::json!({
            "id": "abc123",
            "title": "A Song",
            "thumbnails": []
        });

        let info = YTDLPManager::parse_video_info(&json).unwrap();
        assert_eq!(info.thumbnail_url, None);
    }

    #[test]
    fn detect_default_browser_returns_a_non_empty_platform_default() {
        assert!(!YTDLPManager::detect_default_browser().is_empty());
    }
}
