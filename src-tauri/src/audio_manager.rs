use crate::models::{AudioState, YTVideoInfo};
use crate::ytdlp_installer::YTDLPInstaller;
use crate::ffmpeg_installer::FfmpegInstaller;
use crate::ytdlp_manager::{YTDLPManager, YouTubeBotBypassMethod};
use crate::command_utils::command_no_window_blocking;
use cpal::traits::{DeviceTrait, HostTrait};
use rodio::{OutputStream, Sink, Source};
use std::process::{Child, Stdio};
use std::sync::{Arc, Mutex as StdMutex};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use tokio::sync::{mpsc, Mutex};
use tauri::{AppHandle, Emitter};
use std::sync::mpsc as std_mpsc;

// Sent as ffmpeg's User-Agent when fetching audio -- YouTube's CDN can reject or
// throttle requests from clients with no/unusual User-Agent strings.
const FFMPEG_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

// Streams raw PCM from ffmpeg's stdout. This is the single decode path for every
// playback scenario -- ffmpeg's `-i` flag treats a local file path and a remote URL
// identically, and it decodes every codec YouTube can serve (Opus/WebM included),
// so there's no need for a separate in-process decoder for local files.
struct FfmpegStreamSource {
    stdout: std::process::ChildStdout,
    sample_rate: u32,
    channels: u16,
    buf: Vec<i16>,
    buf_index: usize,
}

impl FfmpegStreamSource {
    fn new(stdout: std::process::ChildStdout) -> Self {
        let mut source = Self {
            stdout,
            sample_rate: SAMPLE_RATE,
            channels: CHANNELS,
            buf: Vec::new(),
            buf_index: 0,
        };
        // Pre-read first chunk so timer only starts after ffmpeg is producing audio
        source.read_chunk();
        source
    }

    fn read_chunk(&mut self) -> bool {
        let mut raw_buf = [0u8; 16384]; // 8192 samples
        match std::io::Read::read(&mut self.stdout, &mut raw_buf) {
            Ok(0) => false,
            Ok(n) => {
                // Ensure we have an even number of bytes for i16 conversion
                let usable = n - (n % 2);
                self.buf.clear();
                for chunk in raw_buf[..usable].chunks_exact(2) {
                    self.buf.push(i16::from_le_bytes([chunk[0], chunk[1]]));
                }
                self.buf_index = 0;
                !self.buf.is_empty()
            }
            Err(_) => false,
        }
    }
}

impl Iterator for FfmpegStreamSource {
    type Item = i16;
    fn next(&mut self) -> Option<i16> {
        if self.buf_index >= self.buf.len() {
            if !self.read_chunk() {
                return None;
            }
        }
        let sample = self.buf[self.buf_index];
        self.buf_index += 1;
        Some(sample)
    }
}

impl Source for FfmpegStreamSource {
    fn current_frame_len(&self) -> Option<usize> {
        if self.buf_index < self.buf.len() {
            Some(self.buf.len() - self.buf_index)
        } else {
            None
        }
    }
    fn channels(&self) -> u16 { self.channels }
    fn sample_rate(&self) -> u32 { self.sample_rate }
    fn total_duration(&self) -> Option<std::time::Duration> { None }
}

// Spawns ffmpeg to decode `source` (a local file path or a URL) into raw PCM on
// stdout, optionally starting at `start_offset_secs`. Used for every playback
// scenario: initial play, seek, restart-from-beginning, and auto-retry after a
// dropped stream -- one function instead of duplicating the spawn args each time.
//
// Also captures ffmpeg's stderr in the background into the returned handle, so
// callers can report the real reason (e.g. an HTTP error from the CDN) when a
// stream fails or ends unexpectedly quickly, instead of just "it stopped."
fn spawn_ffmpeg_pcm_stream(source: &str, start_offset_secs: f64) -> Result<(Child, FfmpegStreamSource, Arc<StdMutex<String>>), String> {
    let mut args: Vec<String> = Vec::new();
    if start_offset_secs > 0.0 {
        args.push("-ss".to_string());
        args.push(format!("{:.3}", start_offset_secs));
    }
    // -user_agent is an HTTP-protocol option -- ffmpeg rejects it outright as an
    // unrecognized option when the input is a plain local file path.
    if source.starts_with("http://") || source.starts_with("https://") {
        args.push("-user_agent".to_string());
        args.push(FFMPEG_USER_AGENT.to_string());
    }
    args.push("-i".to_string());
    args.push(source.to_string());
    args.extend([
        "-f".to_string(), "s16le".to_string(),
        "-acodec".to_string(), "pcm_s16le".to_string(),
        "-ar".to_string(), SAMPLE_RATE.to_string(),
        "-ac".to_string(), CHANNELS.to_string(),
        "-loglevel".to_string(), "error".to_string(),
        "pipe:1".to_string(),
    ]);

    let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

    let mut child = command_no_window_blocking(&AudioManager::get_ffmpeg_command())
        .args(&args_refs)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn ffmpeg: {}", e))?;

    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            let _ = child.kill();
            return Err("Failed to get ffmpeg stdout".to_string());
        }
    };

    let stderr_log: Arc<StdMutex<String>> = Arc::new(StdMutex::new(String::new()));
    if let Some(mut stderr) = child.stderr.take() {
        let stderr_log_clone = Arc::clone(&stderr_log);
        std::thread::spawn(move || {
            use std::io::Read;
            let mut buf = String::new();
            let _ = stderr.read_to_string(&mut buf);
            if let Ok(mut guard) = stderr_log_clone.lock() {
                *guard = buf;
            }
        });
    }

    Ok((child, FfmpegStreamSource::new(stdout), stderr_log))
}

// Resolves the direct audio URL for a video, escalating through the same bot-bypass
// methods ytdlp_manager's search uses (plain -> rate-limit -> UA rotation -> geo-bypass
// -> browser cookies) instead of giving up after a single plain attempt. Some videos
// trip YouTube's bot detection even when most don't, and this call previously had no
// retry logic at all -- it would just fail outright for those specific videos.
//
// This runs on its own thread (see AudioCommand::Play), since some bypass methods
// deliberately sleep between requests and the whole ladder can take many seconds for
// a video that's genuinely unavailable. `play_generation`/`my_generation` let a newer
// play request abort this one early instead of blocking behind it to the end.
fn get_audio_url_with_bypass(
    ytdlp_path: &str,
    video_url: &str,
    play_generation: &Arc<AtomicU64>,
    my_generation: u64,
) -> Result<String, String> {
    let methods = [
        YouTubeBotBypassMethod::None,
        YouTubeBotBypassMethod::RateLimit,
        YouTubeBotBypassMethod::UserAgentRotation,
        YouTubeBotBypassMethod::GeoBypass,
        YouTubeBotBypassMethod::CookiesFromBrowser,
    ];

    let mut last_err = String::new();

    for (i, method) in methods.iter().enumerate() {
        if play_generation.load(Ordering::SeqCst) != my_generation {
            return Err("Cancelled - superseded by a newer play request".to_string());
        }

        println!("🔄 Resolving audio URL, attempt {}/{}: {:?}", i + 1, methods.len(), method);

        let bypass_args = YTDLPManager::build_bypass_args(*method);

        let mut ytdlp_args = vec![
            "-f".to_string(),
            "bestaudio[ext=m4a]/bestaudio[ext=mp3]/bestaudio".to_string(),
            "-g".to_string(), // Get URL only
            "--no-warnings".to_string(),
        ];
        ytdlp_args.extend(bypass_args);
        ytdlp_args.push(video_url.to_string());

        let args_refs: Vec<&str> = ytdlp_args.iter().map(|s| s.as_str()).collect();

        let output = match command_no_window_blocking(ytdlp_path).args(&args_refs).output() {
            Ok(output) => output,
            Err(e) => {
                last_err = format!("Failed to run yt-dlp: {}", e);
                continue;
            }
        };

        if output.status.success() {
            let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !url.is_empty() {
                println!("✅ Resolved audio URL with method: {:?}", method);
                return Ok(url);
            }
            last_err = "yt-dlp returned an empty URL".to_string();
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            last_err = if stderr.is_empty() {
                "yt-dlp exited with an error (no stderr output)".to_string()
            } else {
                stderr
            };
        }

        eprintln!("⚠️ Method {:?} failed: {}", method, last_err);
    }

    Err(format!("All bypass methods failed. Last error: {}", last_err))
}

// Bumps the play generation and hands URL resolution off to a background thread,
// which reports back via AudioCommand::UrlResolved. Shared by the initial Play
// command and by the retry-exhausted path re-resolving a possibly-expired URL --
// both are "start fresh from this track" in every way that matters here.
fn spawn_url_resolution_worker(
    track: YTVideoInfo,
    command_tx: &mpsc::UnboundedSender<AudioCommand>,
    play_generation: &Arc<AtomicU64>,
) {
    let my_generation = play_generation.fetch_add(1, Ordering::SeqCst) + 1;
    let video_url = format!("https://www.youtube.com/watch?v={}", track.id);
    let ytdlp_path = YTDLPInstaller::get_ytdlp_path().to_string_lossy().to_string();
    let worker_tx = command_tx.clone();
    let worker_generation_flag = Arc::clone(play_generation);

    std::thread::spawn(move || {
        let result = get_audio_url_with_bypass(&ytdlp_path, &video_url, &worker_generation_flag, my_generation);
        let _ = worker_tx.send(AudioCommand::UrlResolved(track, my_generation, result));
    });
}

// Commands that can be sent to the audio thread
enum AudioCommand {
    Play(YTVideoInfo),
    PlayFromFile(YTVideoInfo, String), // track, file_path
    // Sent by the background URL-resolution thread once it's done (or gave up).
    // The audio thread checks the generation before acting on it -- if a newer
    // Play/PlayFromFile/Stop has since been issued, this result is discarded.
    UrlResolved(YTVideoInfo, u64, Result<String, String>),
    TogglePlayPause,
    Pause,
    Stop,
    Seek(f64), // position in seconds
    SetVolume(f32),
    SetPlaybackRate(f32),
    ReinitAudio,
}

pub struct AudioManager {
    state: Arc<Mutex<AudioState>>,
    command_tx: mpsc::UnboundedSender<AudioCommand>,
    app_handle: Arc<Mutex<Option<AppHandle>>>,
    state_change_rx: Arc<Mutex<std_mpsc::Receiver<()>>>,
    track_ended_rx: Arc<Mutex<std_mpsc::Receiver<()>>>,
}

impl AudioManager {
    pub fn new() -> Self {
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let (state_change_tx, state_change_rx) = std_mpsc::channel();
        let (track_ended_tx, track_ended_rx) = std_mpsc::channel();
        let state = Arc::new(Mutex::new(AudioState::default()));

        // Spawn dedicated audio thread
        let state_clone = Arc::clone(&state);
        let command_tx_clone = command_tx.clone();
        std::thread::spawn(move || {
            audio_thread(command_rx, command_tx_clone, state_clone, state_change_tx, track_ended_tx);
        });

        Self {
            state,
            command_tx,
            app_handle: Arc::new(Mutex::new(None)),
            state_change_rx: Arc::new(Mutex::new(state_change_rx)),
            track_ended_rx: Arc::new(Mutex::new(track_ended_rx)),
        }
    }

    pub async fn set_app_handle(&self, handle: AppHandle) {
        *self.app_handle.lock().await = Some(handle.clone());

        // Spawn a task to listen for state changes and emit events
        let state = Arc::clone(&self.state);
        let state_change_rx = Arc::clone(&self.state_change_rx);
        let track_ended_rx = Arc::clone(&self.track_ended_rx);
        let handle_clone = handle.clone();

        tokio::spawn(async move {
            loop {
                // Check for state change notifications (non-blocking)
                let has_change = {
                    let rx = state_change_rx.lock().await;
                    rx.try_recv().is_ok()
                };

                if has_change {
                    let current_state = state.lock().await.clone();
                    let _ = handle.emit("playback-state-changed", current_state);
                }

                // Check for track-ended notifications
                let track_ended = {
                    let rx = track_ended_rx.lock().await;
                    rx.try_recv().is_ok()
                };

                if track_ended {
                    println!("🔔 Emitting track-ended event");
                    let _ = handle_clone.emit("track-ended", ());
                }

                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        });
    }

    // Helper function to get ffmpeg path (system or local)
    fn get_ffmpeg_command() -> String {
        // First check if system ffmpeg exists in PATH
        let system_check = command_no_window_blocking("ffmpeg")
            .arg("-version")
            .output();

        if let Ok(output) = system_check {
            if output.status.success() {
                return "ffmpeg".to_string(); // Use system ffmpeg
            }
        }

        // Fall back to local installed ffmpeg
        let local_path = FfmpegInstaller::get_ffmpeg_path();
        local_path.to_string_lossy().to_string()
    }

    pub async fn set_loading_state(&self, track: &YTVideoInfo) {
        let mut state = self.state.lock().await;
        state.current_track = Some(track.clone());
        state.is_loading = true;
        state.is_playing = false;
        state.current_position = 0.0;
        state.duration = track.duration as f64;
        drop(state);
        self.emit_state_change().await;
    }

    pub async fn update_track_duration(&self, duration: f64) {
        let mut state = self.state.lock().await;
        state.duration = duration;
        drop(state);
        self.emit_state_change().await;
    }

    pub async fn play(&self, track: YTVideoInfo) -> Result<(), String> {
        println!("🎵 Playing track: {}", track.title);

        // Update state immediately for UI feedback
        {
            let mut state = self.state.lock().await;
            state.current_track = Some(track.clone());
            state.is_loading = true;
            state.is_playing = false;
            state.current_position = 0.0;
            state.duration = track.duration as f64;
        }

        self.emit_state_change().await;

        // Update OS media controls with the correct thumbnail_url
        self.update_media_controls(&track).await;

        // Send play command to audio thread
        self.command_tx
            .send(AudioCommand::Play(track))
            .map_err(|_| "Audio thread disconnected".to_string())?;

        Ok(())
    }

    pub async fn play_from_file(&self, track: YTVideoInfo, file_path: String) -> Result<(), String> {
        println!("🎵 Playing track from file: {} ({})", track.title, file_path);

        // Update state immediately for UI feedback
        {
            let mut state = self.state.lock().await;
            state.current_track = Some(track.clone());
            state.is_loading = true;
            state.is_playing = false;
            state.current_position = 0.0;
            state.duration = track.duration as f64;
        }

        self.emit_state_change().await;

        // Update OS media controls with the correct thumbnail_url
        self.update_media_controls(&track).await;

        // Send play from file command to audio thread
        self.command_tx
            .send(AudioCommand::PlayFromFile(track, file_path))
            .map_err(|_| "Audio thread disconnected".to_string())?;

        Ok(())
    }

    pub async fn toggle_play_pause(&self) -> Result<(), String> {
        self.command_tx
            .send(AudioCommand::TogglePlayPause)
            .map_err(|_| "Audio thread disconnected".to_string())?;
        Ok(())
    }

    pub async fn pause(&self) -> Result<(), String> {
        self.command_tx
            .send(AudioCommand::Pause)
            .map_err(|_| "Audio thread disconnected".to_string())?;
        Ok(())
    }

    pub async fn stop(&self) -> Result<(), String> {
        self.command_tx
            .send(AudioCommand::Stop)
            .map_err(|_| "Audio thread disconnected".to_string())?;
        Ok(())
    }

    pub async fn seek(&self, position: f64) -> Result<(), String> {
        let duration = self.state.lock().await.duration;
        let position = position.min(duration).max(0.0);

        // Send seek command to audio thread
        self.command_tx
            .send(AudioCommand::Seek(position))
            .map_err(|_| "Audio thread disconnected".to_string())?;

        Ok(())
    }

    pub async fn set_volume(&self, volume: f32) -> Result<(), String> {
        let volume = volume.max(0.0).min(1.0);

        // Update state
        self.state.lock().await.volume = volume;

        // Send to audio thread
        self.command_tx
            .send(AudioCommand::SetVolume(volume))
            .map_err(|_| "Audio thread disconnected".to_string())?;

        self.emit_state_change().await;
        Ok(())
    }

    pub async fn set_playback_rate(&self, rate: f32) -> Result<(), String> {
        let rate = rate.max(0.25).min(2.0);

        // Update state
        self.state.lock().await.playback_rate = rate;

        // Send to audio thread
        self.command_tx
            .send(AudioCommand::SetPlaybackRate(rate))
            .map_err(|_| "Audio thread disconnected".to_string())?;

        self.emit_state_change().await;
        Ok(())
    }

    pub async fn reinit_audio(&self) {
        let _ = self.command_tx.send(AudioCommand::ReinitAudio);
    }

    pub async fn get_state(&self) -> AudioState {
        self.state.lock().await.clone()
    }

    async fn emit_state_change(&self) {
        let app_guard = self.app_handle.lock().await;
        if let Some(handle) = app_guard.as_ref() {
            let state = self.state.lock().await;
            let _ = handle.emit("playback-state-changed", state.clone());
        }
    }

    async fn update_media_controls(&self, track: &YTVideoInfo) {
        let app_guard = self.app_handle.lock().await;
        if let Some(handle) = app_guard.as_ref() {
            // Always use a jpg thumbnail URL from the video ID (webp not supported by Windows SMTC)
            let cover_url = Some(format!("https://i.ytimg.com/vi/{}/hqdefault.jpg", track.id));
            println!("🎵 Updating media controls with cover_url: {:?}", cover_url);
            use tauri::Manager;
            let state = handle.state::<crate::AppState>();
            state.media_keys.update_metadata(
                track.title.clone(),
                track.uploader.clone(),
                track.duration as f64,
                cover_url,
            ).await;
        }
    }
}

// Audio playback constants
const SAMPLE_RATE: u32 = 44100;
const CHANNELS: u16 = 2;

// Tracks playback position using elapsed time
struct PlaybackTimer {
    start_instant: Option<Instant>,
    start_position: f64,
    playback_rate: f32,
}

impl PlaybackTimer {
    fn new() -> Self {
        Self {
            start_instant: None,
            start_position: 0.0,
            playback_rate: 1.0,
        }
    }

    fn start(&mut self, position: f64, rate: f32) {
        self.start_instant = Some(Instant::now());
        self.start_position = position;
        self.playback_rate = rate;
    }

    fn pause(&mut self) -> f64 {
        let position = self.current_position();
        self.start_position = position; // Save current position so resume works correctly
        self.start_instant = None;
        position
    }

    fn seek(&mut self, position: f64) {
        self.start_position = position;
        if self.start_instant.is_some() {
            self.start_instant = Some(Instant::now());
        }
    }

    // Set position without starting the elapsed-time clock -- for seeking while
    // paused, where the track shouldn't start advancing until explicitly resumed.
    fn set_position_paused(&mut self, position: f64) {
        self.start_position = position;
        self.start_instant = None;
    }

    fn set_rate(&mut self, rate: f32) {
        // Update position before changing rate
        if self.start_instant.is_some() {
            self.start_position = self.current_position();
            self.start_instant = Some(Instant::now());
        }
        self.playback_rate = rate;
    }

    fn current_position(&self) -> f64 {
        match self.start_instant {
            Some(start) => {
                let elapsed = start.elapsed().as_secs_f64();
                self.start_position + (elapsed * self.playback_rate as f64)
            }
            None => self.start_position,
        }
    }

    fn is_playing(&self) -> bool {
        self.start_instant.is_some()
    }

    fn stop(&mut self) {
        self.start_instant = None;
        self.start_position = 0.0;
    }
}

fn get_default_device_name() -> Option<String> {
    cpal::default_host()
        .default_output_device()
        .and_then(|d| d.name().ok())
}

// The dedicated audio thread - owns OutputStream and Sink
fn audio_thread(
    mut command_rx: mpsc::UnboundedReceiver<AudioCommand>,
    command_tx: mpsc::UnboundedSender<AudioCommand>,
    state: Arc<Mutex<AudioState>>,
    state_change_tx: std_mpsc::Sender<()>,
    track_ended_tx: std_mpsc::Sender<()>,
) {
    // Create audio output stream once for this thread
    let Ok((mut _stream, mut stream_handle)) = OutputStream::try_default() else {
        eprintln!("❌ Failed to create audio output");
        return;
    };
    println!("✅ Audio output stream created");

    let mut current_sink: Option<Sink> = None;
    // Local file path or URL -- ffmpeg's `-i` flag treats both identically, so a
    // single field now covers what used to be three separate tracking variables.
    let mut current_source: Option<String> = None;
    let mut current_ffmpeg_child: Option<Child> = None; // ffmpeg process to kill on stop
    let mut current_ffmpeg_stderr: Option<Arc<StdMutex<String>>> = None; // for error reporting
    let mut position_timer = PlaybackTimer::new(); // Track playback position
    let mut last_position_update = Instant::now();
    let mut last_device_check = Instant::now();
    let mut last_known_device = get_default_device_name();
    let mut pending_command: Option<AudioCommand> = None;
    // Consecutive auto-retries after the stream died prematurely (not a real track
    // end). Reset on any fresh, user/system-initiated play/seek/restart. Without a
    // cap this could retry forever if the source keeps failing immediately.
    let mut consecutive_premature_ends: u32 = 0;
    const MAX_AUTO_RETRIES: u32 = 3;
    // Bumped on every fresh Play/PlayFromFile/Stop so a slow, still-resolving audio
    // URL lookup for an old request can detect it's been superseded and bail out
    // early, instead of blocking the whole audio thread until all bypass methods
    // are exhausted.
    let play_generation = Arc::new(AtomicU64::new(0));
    // The track behind the current streaming URL, if any -- kept so retries can
    // re-resolve a fresh URL (the old one may have simply expired) instead of only
    // ever retrying the exact URL that just failed. None for local file playback,
    // where re-resolving wouldn't make sense.
    let mut current_streaming_track: Option<YTVideoInfo> = None;
    // Whether we've already re-resolved once for the current playback attempt --
    // caps re-resolution at one retry cycle so a persistently broken video can't
    // loop between "exhaust retries" and "re-resolve" forever.
    let mut has_reresolved = false;

    // Process commands with polling to allow periodic position updates
    loop {
        // Try to receive a command (pending_command takes priority)
        let command = pending_command.take().map(Some).unwrap_or_else(|| command_rx.try_recv().ok());

        // Check if track has ended (sink is empty)
        if let Some(sink) = &current_sink {
            if sink.empty() && position_timer.is_playing() {
                let current_pos = position_timer.current_position();
                let duration = state.blocking_lock().duration;

                // Only trigger "track ended" if position is actually near the end
                if duration <= 0.0 || current_pos >= duration - 3.0 {
                    println!("🏁 Track ended (sink empty)");
                    position_timer.stop();

                    let mut state_guard = state.blocking_lock();
                    state_guard.is_playing = false;
                    state_guard.current_position = duration;
                    drop(state_guard);

                    let _ = state_change_tx.send(());
                    let _ = track_ended_tx.send(());

                    current_sink = None;
                } else {
                    // Stream died prematurely - auto-retry from current position
                    consecutive_premature_ends += 1;
                    println!("⚠️ Stream ended prematurely at {:.1}s (duration: {:.1}s) - auto-retry {}/{}", current_pos, duration, consecutive_premature_ends, MAX_AUTO_RETRIES);

                    // Surface whatever the dying ffmpeg process printed to stderr --
                    // this is where an actual HTTP error (403, 404, connection reset,
                    // etc.) from the CDN would show up, instead of just "it stopped."
                    if let Some(stderr_log) = current_ffmpeg_stderr.take() {
                        if let Ok(log) = stderr_log.lock() {
                            if !log.trim().is_empty() {
                                eprintln!("🔎 ffmpeg stderr from failed stream: {}", log.trim());
                            }
                        }
                    }

                    if let Some(mut child) = current_ffmpeg_child.take() {
                        let _ = child.kill();
                    }
                    current_sink = None;

                    if consecutive_premature_ends > MAX_AUTO_RETRIES {
                        // For a streaming track, the resolved URL may simply have expired --
                        // a very real failure mode for YouTube's signed CDN URLs, not just a
                        // hypothetical one. Re-resolve a fresh URL once before truly giving up,
                        // instead of only ever retrying the exact URL that just failed.
                        if !has_reresolved {
                            if let Some(track) = current_streaming_track.clone() {
                                has_reresolved = true;
                                consecutive_premature_ends = 0;
                                current_source = None;
                                eprintln!("⚠️ Giving up on current URL after {} failed retries - re-resolving a fresh audio URL", MAX_AUTO_RETRIES);
                                {
                                    let mut state_guard = state.blocking_lock();
                                    state_guard.is_loading = true;
                                }
                                let _ = state_change_tx.send(());
                                spawn_url_resolution_worker(track, &command_tx, &play_generation);
                                continue;
                            }
                        }

                        eprintln!("❌ Giving up after {} failed retries - stopping playback", MAX_AUTO_RETRIES);
                        position_timer.stop();
                        current_source = None;
                        let mut state_guard = state.blocking_lock();
                        state_guard.is_playing = false;
                        state_guard.is_loading = false;
                        state_guard.current_position = current_pos;
                        state_guard.playback_error = Some(
                            "Playback failed after multiple retries. The track may be unavailable.".to_string()
                        );
                        drop(state_guard);
                        let _ = state_change_tx.send(());
                    } else if let Some(source) = current_source.clone() {
                        match spawn_ffmpeg_pcm_stream(&source, current_pos) {
                            Ok((child, ffmpeg_source, stderr_log)) => {
                                let Ok(new_sink) = Sink::try_new(&stream_handle) else {
                                    eprintln!("❌ Failed to create sink for retry");
                                    position_timer.pause();
                                    let mut state_guard = state.blocking_lock();
                                    state_guard.is_playing = false;
                                    state_guard.current_position = current_pos;
                                    drop(state_guard);
                                    let _ = state_change_tx.send(());
                                    continue;
                                };

                                let (volume, rate) = {
                                    let state_guard = state.blocking_lock();
                                    (state_guard.volume, state_guard.playback_rate)
                                };

                                new_sink.set_volume(volume);
                                new_sink.set_speed(rate);
                                new_sink.append(ffmpeg_source.convert_samples::<f32>());
                                new_sink.play();

                                current_sink = Some(new_sink);
                                current_ffmpeg_child = Some(child);
                                current_ffmpeg_stderr = Some(stderr_log);

                                // Keep timer running from current position
                                position_timer.start(current_pos, rate);
                                last_position_update = Instant::now();

                                println!("✅ Stream auto-retried from {:.1}s", current_pos);
                            }
                            Err(e) => {
                                eprintln!("❌ Auto-retry failed: {}", e);
                                position_timer.pause();
                                let mut state_guard = state.blocking_lock();
                                state_guard.is_playing = false;
                                state_guard.current_position = current_pos;
                                drop(state_guard);
                                let _ = state_change_tx.send(());
                            }
                        }
                    }
                }
            }
        }

        // Periodically update position in state (every 500ms)
        if position_timer.is_playing() && last_position_update.elapsed() > std::time::Duration::from_millis(500) {
            let current_pos = position_timer.current_position();
            let duration = state.blocking_lock().duration;

            // Don't exceed duration
            let clamped_pos = current_pos.min(duration);

            {
                let mut state_guard = state.blocking_lock();
                state_guard.current_position = clamped_pos;
                // 1.0 = seeking available (any source, since ffmpeg can seek by path or URL)
                state_guard.download_progress = if current_source.is_some() { 1.0 } else { 0.0 };
            }
            let _ = state_change_tx.send(());
            last_position_update = Instant::now();
        }

        // Check for audio device changes every 2 seconds
        if last_device_check.elapsed() > std::time::Duration::from_secs(2) {
            last_device_check = Instant::now();
            let current_device = get_default_device_name();
            if current_device != last_known_device {
                println!("🔊 Audio device changed ({:?} → {:?}), reinitializing...", last_known_device, current_device);
                last_known_device = current_device;

                let current_pos = position_timer.current_position();
                let was_playing = position_timer.is_playing();

                if let Ok((new_stream, new_handle)) = OutputStream::try_default() {
                    _stream = new_stream;
                    stream_handle = new_handle;

                    // Stop old sink and ffmpeg
                    if let Some(sink) = current_sink.take() {
                        sink.stop();
                    }
                    if let Some(mut child) = current_ffmpeg_child.take() {
                        let _ = child.kill();
                    }

                    if was_playing && current_source.is_some() {
                        // Schedule a seek to restore playback position on the new device
                        pending_command = Some(AudioCommand::Seek(current_pos));
                    }
                    println!("✅ Audio device reinitialized successfully");
                }
            }
        }

        let Some(command) = command else {
            // No command, sleep briefly and continue loop for position updates
            std::thread::sleep(std::time::Duration::from_millis(50));
            continue;
        };

        match command {
            AudioCommand::Play(track) => {
                // Stop current playback
                if let Some(sink) = current_sink.take() {
                    sink.stop();
                }
                if let Some(mut child) = current_ffmpeg_child.take() {
                    let _ = child.kill();
                }
                current_source = None;
                current_ffmpeg_stderr = None;
                consecutive_premature_ends = 0;
                has_reresolved = false;
                current_streaming_track = Some(track.clone());
                {
                    let mut state_guard = state.blocking_lock();
                    state_guard.playback_error = None;
                }

                println!("📥 Getting audio URL from yt-dlp...");
                spawn_url_resolution_worker(track, &command_tx, &play_generation);
            }
            AudioCommand::UrlResolved(track, generation, result) => {
                if play_generation.load(Ordering::SeqCst) != generation {
                    println!("⏭️ Discarding stale resolved URL for: {}", track.title);
                    continue;
                }

                let audio_url = match result {
                    Ok(url) => url,
                    Err(e) => {
                        eprintln!("❌ Failed to get audio URL: {}", e);
                        let mut state_guard = state.blocking_lock();
                        state_guard.is_loading = false;
                        state_guard.is_playing = false;
                        state_guard.playback_error = Some(
                            "Couldn't load this track. It may be unavailable or region-restricted.".to_string()
                        );
                        drop(state_guard);
                        let _ = state_change_tx.send(());
                        continue;
                    }
                };

                println!("✅ Got audio URL, starting ffmpeg stream...");

                match spawn_ffmpeg_pcm_stream(&audio_url, 0.0) {
                    Ok((mut child, source, stderr_log)) => {
                        let Ok(sink) = Sink::try_new(&stream_handle) else {
                            eprintln!("❌ Failed to create sink");
                            let _ = child.kill();
                            continue;
                        };

                        let (volume, rate) = {
                            let state_guard = state.blocking_lock();
                            (state_guard.volume, state_guard.playback_rate)
                        };

                        sink.set_volume(volume);
                        sink.set_speed(rate);
                        sink.append(source.convert_samples::<f32>());
                        sink.play();

                        current_sink = Some(sink);
                        current_ffmpeg_child = Some(child);
                        current_ffmpeg_stderr = Some(stderr_log);
                        current_source = Some(audio_url.clone());

                        // Start position timer
                        position_timer.start(0.0, rate);
                        last_position_update = Instant::now();

                        // Update state
                        {
                            let mut state_guard = state.blocking_lock();
                            state_guard.is_loading = false;
                            state_guard.is_playing = true;
                            state_guard.current_position = 0.0;
                            state_guard.download_progress = 1.0;
                        }
                        let _ = state_change_tx.send(());

                        println!("▶️ Streaming: {}", track.title);
                    }
                    Err(e) => {
                        eprintln!("❌ Failed to start stream: {}", e);
                        let mut state_guard = state.blocking_lock();
                        state_guard.is_loading = false;
                        state_guard.is_playing = false;
                        drop(state_guard);
                        let _ = state_change_tx.send(());
                    }
                }
            }
            AudioCommand::PlayFromFile(track, file_path) => {
                // Stop current playback
                if let Some(sink) = current_sink.take() {
                    sink.stop();
                }
                if let Some(mut child) = current_ffmpeg_child.take() {
                    let _ = child.kill();
                }
                current_source = None;
                current_ffmpeg_stderr = None;
                consecutive_premature_ends = 0;
                has_reresolved = false;
                current_streaming_track = None;
                {
                    let mut state_guard = state.blocking_lock();
                    state_guard.playback_error = None;
                }
                // Invalidate any in-flight Play URL resolution -- its result would
                // otherwise arrive later and could stomp on this file playing now.
                play_generation.fetch_add(1, Ordering::SeqCst);

                println!("📥 Playing from local file: {}", file_path);

                match spawn_ffmpeg_pcm_stream(&file_path, 0.0) {
                    Ok((mut child, source, stderr_log)) => {
                        let Ok(sink) = Sink::try_new(&stream_handle) else {
                            eprintln!("❌ Failed to create sink");
                            let _ = child.kill();
                            continue;
                        };

                        let (volume, rate) = {
                            let state_guard = state.blocking_lock();
                            (state_guard.volume, state_guard.playback_rate)
                        };

                        sink.set_volume(volume);
                        sink.set_speed(rate);
                        sink.append(source.convert_samples::<f32>());
                        sink.play();

                        current_sink = Some(sink);
                        current_ffmpeg_child = Some(child);
                        current_ffmpeg_stderr = Some(stderr_log);
                        current_source = Some(file_path.clone());

                        position_timer.start(0.0, rate);
                        last_position_update = Instant::now();

                        {
                            let mut state_guard = state.blocking_lock();
                            state_guard.is_loading = false;
                            state_guard.is_playing = true;
                            state_guard.current_position = 0.0;
                            state_guard.download_progress = 1.0;
                        }
                        let _ = state_change_tx.send(());

                        println!("▶️ Playing: {}", track.title);
                    }
                    Err(e) => {
                        eprintln!("❌ Failed to play file: {}", e);
                        let mut state_guard = state.blocking_lock();
                        state_guard.is_loading = false;
                        state_guard.is_playing = false;
                        drop(state_guard);
                        let _ = state_change_tx.send(());
                    }
                }
            }
            AudioCommand::Seek(mut position) => {
                // Check if there are more Seek commands waiting - if so, skip to the latest
                loop {
                    match command_rx.try_recv() {
                        Ok(AudioCommand::Seek(new_position)) => {
                            println!("⏩ Skipping seek to {:.1}s, newer seek to {:.1}s found", position, new_position);
                            position = new_position;
                        }
                        Ok(_) => {
                            // Put non-Seek command back (we can't, so just break and it will be lost)
                            // This is acceptable since Seek commands should be the only ones spammed
                            eprintln!("⚠️ Non-Seek command found while draining Seek queue");
                            break;
                        }
                        Err(_) => break, // No more commands
                    }
                }

                println!("⏩ Processing final seek to {:.1}s", position);

                // Remember whether it was playing or paused before the seek, so we
                // land back in the same state instead of always forcing playback --
                // seeking while paused should stay paused, matching how it looks.
                let was_playing = position_timer.is_playing();

                // Signal loading immediately -- respawning ffmpeg takes ~150-200ms,
                // and without this the UI has nothing to show for that window.
                {
                    let mut state_guard = state.blocking_lock();
                    state_guard.is_loading = true;
                }
                let _ = state_change_tx.send(());

                // Stop current playback
                if let Some(sink) = current_sink.take() {
                    sink.stop();
                }
                if let Some(mut child) = current_ffmpeg_child.take() {
                    let _ = child.kill();
                }
                current_ffmpeg_stderr = None;
                consecutive_premature_ends = 0;

                if let Some(source) = current_source.clone() {
                    let seek_start = Instant::now();
                    println!("⏩ Seeking to {:.1}s...", position);

                    match spawn_ffmpeg_pcm_stream(&source, position) {
                        Ok((mut child, ffmpeg_source, stderr_log)) => {
                            let Ok(sink) = Sink::try_new(&stream_handle) else {
                                eprintln!("❌ Failed to create sink for seek");
                                let _ = child.kill();
                                let mut state_guard = state.blocking_lock();
                                state_guard.is_loading = false;
                                drop(state_guard);
                                let _ = state_change_tx.send(());
                                continue;
                            };

                            let (volume, rate) = {
                                let state_guard = state.blocking_lock();
                                (state_guard.volume, state_guard.playback_rate)
                            };

                            sink.set_volume(volume);
                            sink.set_speed(rate);
                            sink.append(ffmpeg_source.convert_samples::<f32>());
                            if was_playing {
                                sink.play();
                                position_timer.start(position, rate);
                            } else {
                                sink.pause();
                                position_timer.set_position_paused(position);
                            }

                            current_sink = Some(sink);
                            current_ffmpeg_child = Some(child);
                            current_ffmpeg_stderr = Some(stderr_log);

                            last_position_update = Instant::now();

                            {
                                let mut state_guard = state.blocking_lock();
                                state_guard.current_position = position;
                                state_guard.is_playing = was_playing;
                                state_guard.is_loading = false;
                            }
                            let _ = state_change_tx.send(());

                            let seek_ms = seek_start.elapsed().as_secs_f64() * 1000.0;
                            println!("⏩ Seeked to {:.1}s - took {:.1}ms", position, seek_ms);
                        }
                        Err(e) => {
                            eprintln!("❌ Failed to seek: {}", e);
                            let mut state_guard = state.blocking_lock();
                            state_guard.is_loading = false;
                            drop(state_guard);
                            let _ = state_change_tx.send(());
                        }
                    }
                } else {
                    let mut state_guard = state.blocking_lock();
                    state_guard.is_loading = false;
                    drop(state_guard);
                    let _ = state_change_tx.send(());
                }
            }
            AudioCommand::TogglePlayPause => {
                let state_guard = state.blocking_lock();
                let is_playing = state_guard.is_playing;
                let duration = state_guard.duration;
                let current_pos = position_timer.current_position();
                let rate = state_guard.playback_rate;
                let volume = state_guard.volume;
                drop(state_guard);

                // Check if track ended (at or near duration, or sink is gone) - need to restart
                let has_track = current_source.is_some();
                let track_ended = (current_pos >= duration - 0.5 && duration > 0.0) ||
                                  (has_track && current_sink.is_none());

                if is_playing {
                    // Pause
                    if let Some(sink) = &current_sink {
                        sink.pause();
                        let paused_pos = position_timer.pause();
                        let mut state_guard = state.blocking_lock();
                        state_guard.is_playing = false;
                        state_guard.current_position = paused_pos;
                        println!("⏸️ Paused at {:.1}s", paused_pos);
                        drop(state_guard);
                        let _ = state_change_tx.send(());
                    }
                } else if track_ended {
                    // Track ended, restart from beginning
                    if let Some(sink) = current_sink.take() {
                        sink.stop();
                    }
                    if let Some(mut child) = current_ffmpeg_child.take() {
                        let _ = child.kill();
                    }
                    current_ffmpeg_stderr = None;
                    consecutive_premature_ends = 0;

                    if let Some(source) = current_source.clone() {
                        match spawn_ffmpeg_pcm_stream(&source, 0.0) {
                            Ok((mut child, ffmpeg_source, stderr_log)) => {
                                if let Ok(sink) = Sink::try_new(&stream_handle) {
                                    sink.set_volume(volume);
                                    sink.set_speed(rate);
                                    sink.append(ffmpeg_source.convert_samples::<f32>());
                                    sink.play();
                                    current_sink = Some(sink);
                                    current_ffmpeg_child = Some(child);
                                    current_ffmpeg_stderr = Some(stderr_log);

                                    position_timer.start(0.0, rate);
                                    last_position_update = Instant::now();

                                    let mut state_guard = state.blocking_lock();
                                    state_guard.is_playing = true;
                                    state_guard.current_position = 0.0;
                                    drop(state_guard);
                                    let _ = state_change_tx.send(());
                                    println!("🔄 Restarted track from beginning");
                                } else {
                                    eprintln!("❌ Failed to create sink for restart");
                                    let _ = child.kill();
                                }
                            }
                            Err(e) => {
                                eprintln!("❌ Failed to restart track: {}", e);
                            }
                        }
                    }
                } else {
                    // Normal resume
                    if let Some(sink) = &current_sink {
                        sink.play();
                        position_timer.start(current_pos, rate);
                        let mut state_guard = state.blocking_lock();
                        state_guard.is_playing = true;
                        state_guard.current_position = current_pos;
                        println!("▶️ Resumed from {:.1}s (rate: {:.2})", current_pos, rate);
                        drop(state_guard);
                        last_position_update = Instant::now();
                        let _ = state_change_tx.send(());
                    }
                }
            }
            AudioCommand::Pause => {
                if let Some(sink) = &current_sink {
                    sink.pause();
                    // Pause timer and get current position
                    let current_pos = position_timer.pause();
                    let mut state_guard = state.blocking_lock();
                    state_guard.is_playing = false;
                    state_guard.current_position = current_pos;
                    println!("⏸️ Explicit pause at {:.1}s", current_pos);
                    drop(state_guard);
                    let _ = state_change_tx.send(());
                }
            }
            AudioCommand::Stop => {
                if let Some(sink) = current_sink.take() {
                    sink.stop();
                }
                if let Some(mut child) = current_ffmpeg_child.take() {
                    let _ = child.kill();
                }
                current_source = None;
                current_ffmpeg_stderr = None;
                consecutive_premature_ends = 0;
                has_reresolved = false;
                current_streaming_track = None;
                play_generation.fetch_add(1, Ordering::SeqCst);
                position_timer.stop();
                let mut state_guard = state.blocking_lock();
                state_guard.is_playing = false;
                state_guard.current_position = 0.0;
                state_guard.playback_error = None;
                drop(state_guard);
                let _ = state_change_tx.send(());
                println!("⏹️ Stopped");
            }
            AudioCommand::SetVolume(volume) => {
                if let Some(sink) = &current_sink {
                    sink.set_volume(volume);
                }
            }
            AudioCommand::SetPlaybackRate(rate) => {
                if let Some(sink) = &current_sink {
                    sink.set_speed(rate);
                    // Update position timer with new rate
                    position_timer.set_rate(rate);
                }
            }
            AudioCommand::ReinitAudio => {
                // Stop current playback
                if let Some(sink) = current_sink.take() {
                    sink.stop();
                }
                if let Some(mut child) = current_ffmpeg_child.take() {
                    let _ = child.kill();
                }
                position_timer.stop();
                current_source = None;
                current_ffmpeg_stderr = None;
                consecutive_premature_ends = 0;
                has_reresolved = false;
                current_streaming_track = None;
                play_generation.fetch_add(1, Ordering::SeqCst);

                // Reinitialize audio output device
                match OutputStream::try_default() {
                    Ok((new_stream, new_handle)) => {
                        _stream = new_stream;
                        stream_handle = new_handle;
                        println!("✅ Audio output device reinitialized");
                    }
                    Err(e) => {
                        eprintln!("❌ Failed to reinitialize audio device: {}", e);
                    }
                }

                let mut state_guard = state.blocking_lock();
                state_guard.is_playing = false;
                state_guard.current_track = None;
                drop(state_guard);
                let _ = state_change_tx.send(());
            }
        }
    }
}
