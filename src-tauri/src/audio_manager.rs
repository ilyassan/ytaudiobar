use crate::models::{AudioState, YTVideoInfo};
use crate::ytdlp_installer::YTDLPInstaller;
use crate::ffmpeg_installer::FfmpegInstaller;
use crate::ytdlp_manager::{YTDLPManager, YouTubeBotBypassMethod};
use crate::command_utils::command_no_window_blocking;
use crate::analytics::{truncate_for_analytics, Analytics};
use serde_json::json;
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
/// Decodes little-endian PCM bytes into `out`, carrying a sample split across
/// two reads via `partial`.
///
/// A pipe read can end mid-sample. Discarding that odd byte shifts every
/// following sample by one, so the remainder of the track decodes as noise with
/// the channels swapped -- hence threading the leftover through to the next call.
fn decode_pcm_bytes(bytes: &[u8], partial: &mut Option<u8>, out: &mut Vec<i16>) {
    let mut rest = bytes;

    if let Some(leading) = *partial {
        match rest.split_first() {
            Some((&first, tail)) => {
                out.push(i16::from_le_bytes([leading, first]));
                *partial = None;
                rest = tail;
            }
            // Nothing arrived to complete it -- keep holding the byte.
            None => return,
        }
    }

    let mut pairs = rest.chunks_exact(2);
    for chunk in pairs.by_ref() {
        out.push(i16::from_le_bytes([chunk[0], chunk[1]]));
    }
    *partial = pairs.remainder().first().copied();
}

// Bundles ffmpeg's captured stderr with a timestamp of the last successful
// read from its stdout. The timestamp exists because rodio's `Sink::empty()`
// stays false the entire time this source's `Iterator::next()` is blocked
// inside a read -- however long that takes -- so a mid-stream network death
// (as opposed to a stream that never started at all) isn't visible via
// sink-emptiness at all. The main loop instead watches this timestamp
// directly to detect "ffmpeg is stuck," independent of -- and not reliant on
// -- ffmpeg's own -rw_timeout ever firing (which isn't consistently honored
// across platforms/TLS backends for HTTPS inputs).
struct StreamHealth {
    stderr: Arc<StdMutex<String>>,
    last_data_at: Arc<StdMutex<Instant>>,
}

struct FfmpegStreamSource {
    stdout: std::process::ChildStdout,
    sample_rate: u32,
    channels: u16,
    buf: Vec<i16>,
    buf_index: usize,
    // Trailing byte of an odd-length read, carried into the next one. A pipe
    // read can split a 16-bit sample down the middle; dropping that byte would
    // shift every following sample by one, so the rest of the track decodes as
    // noise with the channels swapped.
    partial_byte: Option<u8>,
    last_data_at: Arc<StdMutex<Instant>>,
}

impl FfmpegStreamSource {
    // None means the very first read hit EOF/an error immediately -- ffmpeg
    // spawned successfully as an OS process (which almost never fails) but
    // then exited before producing any audio at all, e.g. because the network
    // was already down or the resolved URL was rejected outright. Returning
    // that as a real failure (instead of an empty-but-"successful" source)
    // matters: every caller treats `Ok`/`Some` from this constructor as proof
    // playback actually started, including resuming the position timer on an
    // auto-retry -- so silently succeeding here made the retry logic believe
    // it had recovered when nothing was ever going to play.
    fn new(stdout: std::process::ChildStdout, last_data_at: Arc<StdMutex<Instant>>) -> Option<Self> {
        let mut source = Self {
            stdout,
            sample_rate: SAMPLE_RATE,
            channels: CHANNELS,
            buf: Vec::new(),
            buf_index: 0,
            partial_byte: None,
            last_data_at,
        };
        // Pre-read first chunk so timer only starts after ffmpeg is producing audio
        if !source.read_chunk() {
            return None;
        }
        Some(source)
    }

    fn read_chunk(&mut self) -> bool {
        let mut raw_buf = [0u8; 16384]; // 8192 samples
        loop {
            match std::io::Read::read(&mut self.stdout, &mut raw_buf) {
                Ok(0) => return false,
                Ok(n) => {
                    if let Ok(mut t) = self.last_data_at.lock() {
                        *t = Instant::now();
                    }
                    self.buf.clear();
                    decode_pcm_bytes(&raw_buf[..n], &mut self.partial_byte, &mut self.buf);

                    if !self.buf.is_empty() {
                        self.buf_index = 0;
                        return true;
                    }
                    // A read that carried only half a sample yields nothing yet.
                    // Read again rather than returning false, which the playback
                    // loop reads as end-of-stream and answers with its retry
                    // ladder.
                }
                Err(_) => return false,
            }
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
fn spawn_ffmpeg_pcm_stream(source: &str, start_offset_secs: f64) -> Result<(Child, FfmpegStreamSource, StreamHealth), String> {
    let mut args: Vec<String> = Vec::new();
    if start_offset_secs > 0.0 {
        args.push("-ss".to_string());
        args.push(format!("{:.3}", start_offset_secs));
    }
    // -user_agent and the reconnect/timeout options below are HTTP-protocol-only
    // -- ffmpeg rejects them outright as unrecognized options when the input is
    // a plain local file path.
    if source.starts_with("http://") || source.starts_with("https://") {
        args.push("-user_agent".to_string());
        args.push(FFMPEG_USER_AGENT.to_string());

        // Without a read timeout, a dropped network connection (e.g. wifi
        // turning off mid-stream) leaves ffmpeg blocked forever on its socket
        // read -- it neither errors nor exits, so stdout never closes,
        // `sink.empty()` never becomes true, and the existing stall-detection/
        // auto-retry logic below (which is only driven by the decoder finishing)
        // never triggers. Meanwhile the wall-clock position timer keeps ticking,
        // so the UI shows playback continuing normally right up to the track's
        // full duration even though no audio is actually flowing.
        //
        // -rw_timeout bounds how long a read/write can block before ffmpeg
        // errors out and exits (microseconds); -reconnect* lets it silently
        // recover on its own for a brief blip instead of erroring unnecessarily.
        args.push("-reconnect".to_string());
        args.push("1".to_string());
        args.push("-reconnect_streamed".to_string());
        args.push("1".to_string());
        args.push("-reconnect_delay_max".to_string());
        args.push("5".to_string());
        args.push("-rw_timeout".to_string());
        args.push("15000000".to_string());
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

    let last_data_at = Arc::new(StdMutex::new(Instant::now()));

    let Some(source) = FfmpegStreamSource::new(stdout, Arc::clone(&last_data_at)) else {
        let _ = child.kill();
        // ffmpeg already exited (that's why the pre-read hit EOF), so the
        // stderr-reading thread above should finish almost immediately --
        // give it a brief moment to capture the real reason before falling
        // back to a generic message.
        std::thread::sleep(std::time::Duration::from_millis(100));
        let reason = stderr_log
            .lock()
            .map(|g| g.trim().to_string())
            .unwrap_or_default();
        return Err(if reason.is_empty() {
            "ffmpeg exited immediately without producing audio".to_string()
        } else {
            reason
        });
    };

    Ok((child, source, StreamHealth { stderr: stderr_log, last_data_at }))
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
    // CookiesFromBrowser is TCC-protected on macOS (Safari) — skip it there.
    #[cfg(target_os = "macos")]
    let methods: Vec<YouTubeBotBypassMethod> = vec![
        YouTubeBotBypassMethod::None,
        YouTubeBotBypassMethod::RateLimit,
        YouTubeBotBypassMethod::UserAgentRotation,
        YouTubeBotBypassMethod::GeoBypass,
    ];
    #[cfg(not(target_os = "macos"))]
    let methods: Vec<YouTubeBotBypassMethod> = vec![
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
            "-g".to_string(),
            "--no-warnings".to_string(),
            "--socket-timeout".to_string(), "15".to_string(),
            "--retries".to_string(), "2".to_string(),
        ];
        ytdlp_args.extend(bypass_args);
        // Append AFTER bypass args so these override any conflicting extractor-args.
        // RateLimit bypass uses player_skip=configs,webpage which breaks -g (returns
        // empty URL); player_skip=configs alone (no webpage) cuts time from ~40s to
        // ~17s while keeping stream URL extraction intact. skip=dash,hls removes the
        // separate manifest fetch — YouTube embeds audio URLs in the player response
        // so the manifest step is unnecessary for direct audio playback.
        ytdlp_args.extend([
            "--extractor-args".to_string(),
            "youtube:player_skip=configs".to_string(),
            "--extractor-args".to_string(),
            "youtube:skip=dash,hls".to_string(),
        ]);
        ytdlp_args.push(video_url.to_string());

        let args_refs: Vec<&str> = ytdlp_args.iter().map(|s| s.as_str()).collect();

        // Stream stdout instead of waiting for process exit. On macOS, yt-dlp
        // (an un-notarized PyInstaller binary) is blocked by the OS security
        // scanner for 20–25s before it's allowed to exit — but it outputs the
        // URL to stdout well before that. Reading the first non-empty line and
        // then killing the process cuts audio URL resolution from ~25s to ~17s.
        let mut child = match command_no_window_blocking(ytdlp_path)
            .args(&args_refs)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                last_err = format!("Failed to run yt-dlp: {}", e);
                continue;
            }
        };

        // Read the URL from stdout as soon as yt-dlp emits it.
        let url = if let Some(stdout) = child.stdout.take() {
            use std::io::BufRead;
            std::io::BufReader::new(stdout)
                .lines()
                .find_map(|l| {
                    let line = l.ok()?;
                    let trimmed = line.trim().to_string();
                    if trimmed.starts_with("http") { Some(trimmed) } else { None }
                })
                .unwrap_or_default()
        } else {
            String::new()
        };

        // Kill immediately — don't wait for the OS security scan to finish.
        let _ = child.kill();

        if !url.is_empty() {
            println!("✅ Resolved audio URL with method: {:?}", method);
            return Ok(url);
        }

        // No URL on stdout — collect stderr for the error message.
        let stderr = child.stderr.take().map(|s| {
            use std::io::Read;
            let mut buf = String::new();
            let _ = std::io::BufReader::new(s).read_to_string(&mut buf);
            buf.trim().to_string()
        }).unwrap_or_default();

        last_err = if stderr.is_empty() {
            "yt-dlp returned an empty URL".to_string()
        } else {
            stderr
        };

        eprintln!("⚠️ Method {:?} failed: {}", method, last_err);
    }

    Err(format!("All bypass methods failed. Last error: {}", last_err))
}

// Bumps the play generation and hands URL resolution off to a background thread,
// which reports back via AudioCommand::UrlResolved. Shared by the initial Play
// command (resume_position 0.0) and by the retry-exhausted path re-resolving a
// possibly-expired URL (resume_position wherever playback actually was) -- the
// URL lookup itself is identical either way, only where the fresh stream should
// pick up differs.
fn spawn_url_resolution_worker(
    track: YTVideoInfo,
    command_tx: &mpsc::UnboundedSender<AudioCommand>,
    play_generation: &Arc<AtomicU64>,
    resume_position: f64,
) {
    let my_generation = play_generation.fetch_add(1, Ordering::SeqCst) + 1;
    let video_url = format!("https://www.youtube.com/watch?v={}", track.id);
    let ytdlp_path = YTDLPInstaller::get_ytdlp_path().to_string_lossy().to_string();
    let worker_tx = command_tx.clone();
    let worker_generation_flag = Arc::clone(play_generation);

    std::thread::spawn(move || {
        let result = get_audio_url_with_bypass(&ytdlp_path, &video_url, &worker_generation_flag, my_generation);
        let _ = worker_tx.send(AudioCommand::UrlResolved(track, my_generation, result, resume_position));
    });
}

// Commands that can be sent to the audio thread
enum AudioCommand {
    Play(YTVideoInfo),
    PlayFromFile(YTVideoInfo, String), // track, file_path
    // Sent by the background URL-resolution thread once it's done (or gave up).
    // The audio thread checks the generation before acting on it -- if a newer
    // Play/PlayFromFile/Stop has since been issued, this result is discarded.
    // The f64 is where playback should resume once the fresh URL is ready --
    // 0.0 for a genuinely new track, or the position it was at when a
    // mid-stream re-resolve was triggered.
    UrlResolved(YTVideoInfo, u64, Result<String, String>, f64),
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
    analytics: Arc<Analytics>,
}

impl AudioManager {
    pub fn new(analytics: Arc<Analytics>) -> Self {
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let (state_change_tx, state_change_rx) = std_mpsc::channel();
        let (track_ended_tx, track_ended_rx) = std_mpsc::channel();
        let state = Arc::new(Mutex::new(AudioState::default()));

        // Spawn dedicated audio thread
        let state_clone = Arc::clone(&state);
        let command_tx_clone = command_tx.clone();
        let analytics_clone = Arc::clone(&analytics);
        std::thread::spawn(move || {
            audio_thread(command_rx, command_tx_clone, state_clone, state_change_tx, track_ended_tx, analytics_clone);
        });

        Self {
            state,
            command_tx,
            app_handle: Arc::new(Mutex::new(None)),
            state_change_rx: Arc::new(Mutex::new(state_change_rx)),
            track_ended_rx: Arc::new(Mutex::new(track_ended_rx)),
            analytics,
        }
    }

    pub async fn set_app_handle(&self, handle: AppHandle) {
        *self.app_handle.lock().await = Some(handle.clone());

        // Block on each channel's recv() from a dedicated OS thread instead of
        // polling try_recv() on a timer -- these threads then wake up exactly
        // when the audio thread actually signals a change, instead of waking
        // 10x/second forever for the life of the app regardless of activity.
        let state = Arc::clone(&self.state);
        let state_change_rx = Arc::clone(&self.state_change_rx);
        let handle_for_state = handle.clone();
        std::thread::spawn(move || loop {
            let recv_result = state_change_rx.blocking_lock().recv();
            if recv_result.is_err() {
                break; // audio thread's sender dropped, nothing more will come
            }
            let current_state = state.blocking_lock().clone();
            let _ = handle_for_state.emit("playback-state-changed", current_state);
        });

        let track_ended_rx = Arc::clone(&self.track_ended_rx);
        std::thread::spawn(move || loop {
            let recv_result = track_ended_rx.blocking_lock().recv();
            if recv_result.is_err() {
                break;
            }
            println!("🔔 Emitting track-ended event");
            let _ = handle.emit("track-ended", ());
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

        self.analytics.track("track_played");

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

        self.analytics.track("track_played");

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
            // Derive a jpg thumbnail URL directly from the video id (webp
            // isn't supported by Windows SMTC, and this is more reliable
            // than trusting whatever yt-dlp happened to put in
            // thumbnail_url at this shallow query depth).
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

// Tracks playback position anchored to the audio pipeline's own
// consumed-sample clock (rodio's Sink::get_pos()) rather than wall-clock
// elapsed time. Wall-clock extrapolation drifts from reality any time real
// audio isn't actually flowing -- a paused sink, a network stall too brief
// to trip the retry ladder, decoder buffering -- because it advances
// unconditionally regardless of whether anything was actually produced.
// get_pos() only advances as samples are actually pulled through the sink's
// source chain, so it stays accurate through exactly those situations for
// free, without this needing to know why nothing is flowing.
//
// Deliberately takes the elapsed time as a plain Duration rather than a
// &Sink directly: the arithmetic here doesn't care where the duration came
// from, and decoupling it from rodio's type keeps this unit-testable with
// plain Duration values instead of requiring a real audio output stream.
// Biggest jump in the sink clock we'll fully believe between two consecutive
// readings. The periodic position tick runs every 500ms and command-driven
// readings are more frequent than that, so a real delta is comfortably under
// this in the common case; anything larger is capped rather than trusted or
// discarded outright, since it's usually the sink clock having been rescaled
// out from under us (see `set_rate`) but could instead be a genuinely delayed
// reading -- see the comment in `current_position` for why capping beats
// either extreme.
const MAX_PLAUSIBLE_ELAPSED_DELTA_SECS: f64 = 2.0;

struct PlaybackTimer {
    // Position in track-time seconds, accumulated incrementally from sink-clock
    // deltas rather than recomputed from an absolute anchor. This is deliberate:
    // rodio derives get_pos() as (samples consumed / (sample_rate * speed
    // factor)), where the divisor is the *live* speed-adjusted rate, and it
    // only stops dividing the running total by that live rate once a "frame
    // boundary" resets its internal counters. That boundary is detected by
    // coincidence-matching the running sample count against the source's
    // current buffer length -- for our ffmpeg PCM source, whose chunk sizes
    // come from arbitrary OS pipe reads rather than fixed frames, that match
    // becomes statistically unreachable almost immediately, so in practice the
    // running total just keeps growing and getting divided by whatever the
    // CURRENT speed factor is. Changing speed therefore re-divides nearly the
    // WHOLE accumulated sample count by the new factor, retroactively
    // rescaling almost all previously-elapsed time by old_rate/new_rate. An
    // absolute `start_position + elapsed * rate` model reads that rescale as
    // real progress and teleports (forward when slowing down, backward when speeding
    // up, exactly proportionally). Deltas confine the damage to the single
    // reading that straddles the change, which `set_rate` then discards.
    position: f64,
    // Previous sink-clock reading, to difference against. None means "no
    // baseline yet" -- the next reading establishes one without moving the
    // position, which is how a rescale discontinuity gets dropped.
    last_elapsed: Option<f64>,
    playback_rate: f32,
    // Whether we should currently be treated as "playing" -- get_pos() itself
    // already freezes while genuinely paused/stalled, so this exists purely
    // to distinguish "nothing loaded / stopped" from "actively playing," not
    // to drive the position math.
    active: bool,
}

impl PlaybackTimer {
    fn new() -> Self {
        Self {
            position: 0.0,
            last_elapsed: None,
            playback_rate: 1.0,
            active: false,
        }
    }

    fn start(&mut self, position: f64, rate: f32) {
        self.position = position;
        // A fresh sink restarts its clock at 0, and even a reused one needs a
        // new baseline rather than a delta against the old sink's readings.
        self.last_elapsed = None;
        self.playback_rate = rate;
        self.active = true;
    }

    fn pause(&mut self, elapsed: Option<std::time::Duration>) -> f64 {
        let position = self.current_position(elapsed);
        self.active = false;
        position
    }

    // Set position without starting the clock -- for seeking while paused,
    // where the track shouldn't start advancing until explicitly resumed.
    fn set_position_paused(&mut self, position: f64) {
        self.position = position;
        self.last_elapsed = None;
        self.active = false;
    }

    fn set_rate(&mut self, rate: f32, elapsed: Option<std::time::Duration>) {
        // Bank whatever progress happened at the OLD rate first...
        self.current_position(elapsed);
        self.playback_rate = rate;
        // ...then drop the baseline. rodio is about to retroactively rescale
        // its clock by old_rate/new_rate (see the struct comment), so the next
        // reading is not comparable to this one -- it re-baselines instead of
        // being differenced, which discards the discontinuity entirely rather
        // than banking it as a giant fake jump in position.
        self.last_elapsed = None;
    }

    // `elapsed` is the sink's own clock (get_pos()). Advances the accumulated
    // position by however much that clock moved since the last reading, scaled
    // by the current rate. None (nothing currently playing) reports the frozen
    // position rather than guessing from a wall clock.
    //
    // Takes &mut because it's the single place the accumulated position and its
    // baseline move -- every caller reads through here, so there's no way to
    // observe a position that skipped the delta bookkeeping.
    fn current_position(&mut self, elapsed: Option<std::time::Duration>) -> f64 {
        if !self.active {
            return self.position.max(0.0);
        }

        if let Some(elapsed) = elapsed {
            let elapsed_secs = elapsed.as_secs_f64();
            match self.last_elapsed {
                Some(last) => {
                    let delta = elapsed_secs - last;
                    // A negative delta only happens via a rescale (speeding up
                    // divides by a bigger factor, pulling the reading
                    // backward) -- every rate change already drops the
                    // baseline via set_rate, so seeing one here means this
                    // diff itself straddled a rescale outside that path. There's
                    // no real elapsed time to credit, so treat it as 0 rather
                    // than letting position go backward.
                    //
                    // An implausibly large positive delta is ambiguous, unlike
                    // the negative case: usually a rescale (slowing down
                    // divides by a smaller factor, inflating the reading), but
                    // it could instead be this command thread having been
                    // delayed for an unrelated reason (lock contention, a slow
                    // println under a redirected/slow stdout) while real audio
                    // kept flowing via cpal's own independent callback thread --
                    // in which case that elapsed time is genuine. Capping
                    // rather than discarding bounds a true rescale to a small,
                    // safe increment instead of teleporting, while a real long
                    // delay still credits up to the cap instead of losing all
                    // of it permanently.
                    let credited = delta.max(0.0).min(MAX_PLAUSIBLE_ELAPSED_DELTA_SECS);
                    self.position += credited * self.playback_rate as f64;
                }
                // First reading since start/rate change: baseline only.
                None => {}
            }
            self.last_elapsed = Some(elapsed_secs);
        }

        // A position is never legitimately negative, and this is the one choke
        // point every caller goes through -- a negative would otherwise panic
        // later converting to a Duration (see
        // media_key_manager::update_playback_state).
        self.position = self.position.max(0.0);
        self.position
    }

    fn is_playing(&self) -> bool {
        self.active
    }

    fn stop(&mut self) {
        self.active = false;
        self.position = 0.0;
        self.last_elapsed = None;
    }
}

#[cfg(test)]
mod pcm_decode_tests {
    use super::decode_pcm_bytes;

    fn decode_all(chunks: &[&[u8]]) -> Vec<i16> {
        let mut partial = None;
        let mut out = Vec::new();
        for chunk in chunks {
            decode_pcm_bytes(chunk, &mut partial, &mut out);
        }
        out
    }

    #[test]
    fn decodes_whole_samples_from_a_single_even_length_read() {
        // 0x0001, 0x0102 little-endian
        assert_eq!(decode_all(&[&[0x01, 0x00, 0x02, 0x01]]), vec![1, 258]);
    }

    #[test]
    fn a_sample_split_across_two_reads_survives_intact() {
        // The same four bytes, but the pipe splits mid-sample.
        assert_eq!(
            decode_all(&[&[0x01, 0x00, 0x02], &[0x01]]),
            vec![1, 258],
            "the byte carried over must be the low half of the next sample"
        );
    }

    #[test]
    fn splitting_at_every_offset_yields_identical_output() {
        let stream: Vec<u8> = (0..64u8).collect();
        let whole = decode_all(&[&stream]);

        for split in 0..stream.len() {
            let (head, tail) = stream.split_at(split);
            assert_eq!(
                decode_all(&[head, tail]),
                whole,
                "stream split at byte {} decoded differently",
                split
            );
        }
    }

    #[test]
    fn a_lone_byte_produces_no_sample_but_is_not_lost() {
        let mut partial = None;
        let mut out = Vec::new();

        decode_pcm_bytes(&[0xAB], &mut partial, &mut out);
        assert!(out.is_empty());
        assert_eq!(partial, Some(0xAB));

        // An empty read must not drop the held byte.
        decode_pcm_bytes(&[], &mut partial, &mut out);
        assert_eq!(partial, Some(0xAB));

        decode_pcm_bytes(&[0xCD], &mut partial, &mut out);
        assert_eq!(out, vec![i16::from_le_bytes([0xAB, 0xCD])]);
        assert_eq!(partial, None);
    }
}

#[cfg(test)]
mod playback_timer_tests {
    use super::PlaybackTimer;
    use std::time::Duration;

    // PlaybackTimer takes elapsed time as a plain Duration rather than
    // measuring wall-clock time itself, so these tests pass fake elapsed
    // values directly instead of sleeping for real time -- deterministic,
    // instant, and exercising the exact same arithmetic real playback does
    // (real code passes Sink::get_pos() in place of these fake durations).
    fn approx_eq(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "expected ~{}, got {}", b, a);
    }

    #[test]
    fn new_timer_starts_stopped_at_zero() {
        let mut timer = PlaybackTimer::new();
        assert!(!timer.is_playing());
        approx_eq(timer.current_position(None), 0.0);
    }

    #[test]
    fn start_makes_position_advance_at_normal_rate() {
        let mut timer = PlaybackTimer::new();
        timer.start(10.0, 1.0);

        assert!(timer.is_playing());
        // First reading only establishes the baseline...
        approx_eq(timer.current_position(Some(Duration::from_millis(500))), 10.0);
        // ...then 500ms more of sink clock advances position by 500ms.
        approx_eq(timer.current_position(Some(Duration::from_millis(1000))), 10.5);
    }

    #[test]
    fn playback_rate_scales_elapsed_time() {
        let mut timer = PlaybackTimer::new();
        timer.start(0.0, 2.0);

        approx_eq(timer.current_position(Some(Duration::from_millis(500))), 0.0);
        // At 2x speed, 500ms of actual playback advances position by 1.0s.
        approx_eq(timer.current_position(Some(Duration::from_millis(1000))), 1.0);
    }

    #[test]
    fn no_elapsed_data_falls_back_to_the_frozen_position() {
        // When we don't have a real sink to ask (nothing currently playing,
        // or a caller without one in scope), the timer must not guess from a
        // wall clock -- it should report exactly where it was last anchored.
        let mut timer = PlaybackTimer::new();
        timer.start(12.5, 1.0);

        approx_eq(timer.current_position(None), 12.5);
    }

    #[test]
    fn pause_freezes_the_position() {
        let mut timer = PlaybackTimer::new();
        timer.start(0.0, 1.0);
        timer.current_position(Some(Duration::from_millis(200)));

        let paused_at = timer.pause(Some(Duration::from_millis(400)));
        assert!(!timer.is_playing());
        approx_eq(paused_at, 0.2);

        // Position must not keep advancing once paused, no matter what
        // elapsed value would otherwise be passed in.
        approx_eq(timer.current_position(Some(Duration::from_millis(900))), 0.2);
    }

    #[test]
    fn set_position_paused_sets_position_without_starting_the_clock() {
        let mut timer = PlaybackTimer::new();
        timer.start(0.0, 1.0);
        timer.set_position_paused(42.0);

        assert!(!timer.is_playing());
        approx_eq(timer.current_position(None), 42.0);
    }

    #[test]
    fn set_rate_preserves_current_position_at_the_moment_of_the_change() {
        let mut timer = PlaybackTimer::new();
        timer.start(0.0, 1.0);
        timer.current_position(Some(Duration::from_millis(500)));
        // A full second of sink clock at 1x == 1.0s of track.
        approx_eq(timer.current_position(Some(Duration::from_millis(1500))), 1.0);

        timer.set_rate(3.0, Some(Duration::from_millis(1500)));
        // The change itself must not move the position...
        approx_eq(timer.current_position(Some(Duration::from_millis(1500))), 1.0);
        // ...and from here progress accrues at the NEW rate.
        approx_eq(timer.current_position(Some(Duration::from_millis(1700))), 1.0 + 0.2 * 3.0);
    }

    #[test]
    fn rate_change_rescaling_the_sink_clock_does_not_move_the_position() {
        // The real-world bug: rodio derives get_pos() from (samples /
        // (sample_rate * speed factor)), so changing speed retroactively
        // rescales the whole elapsed reading by old_rate/new_rate. Halving the
        // rate therefore roughly DOUBLES the next reading even though no extra
        // audio played -- which an absolute-anchor model banks as real
        // progress and teleports forward (and backward when speeding up).
        let mut timer = PlaybackTimer::new();
        timer.start(10.0, 1.0);
        timer.current_position(Some(Duration::from_secs_f64(20.0)));
        approx_eq(timer.current_position(Some(Duration::from_secs_f64(20.5))), 10.5);

        // Slow to 0.5x: rodio's clock rescales 20.5s -> 41.0s purely from the
        // speed change, with no extra audio actually played.
        timer.set_rate(0.5, Some(Duration::from_secs_f64(20.5)));
        approx_eq(timer.current_position(Some(Duration::from_secs_f64(41.0))), 10.5);

        // Real playback continues from there at the new rate.
        approx_eq(timer.current_position(Some(Duration::from_secs_f64(41.5))), 10.75);

        // Speed up to 2x: the clock rescales the other way, 41.5s -> 10.375s.
        timer.set_rate(2.0, Some(Duration::from_secs_f64(41.5)));
        approx_eq(timer.current_position(Some(Duration::from_secs_f64(10.375))), 10.75);
        approx_eq(timer.current_position(Some(Duration::from_secs_f64(10.875))), 11.75);
    }

    #[test]
    fn implausibly_large_clock_jumps_are_capped_not_lost() {
        // Secondary net for a rescale that isn't bracketed by a set_rate call
        // (rodio applies it asynchronously, so a reading can straddle it).
        // Capped rather than fully discarded: an ambiguous large gap could
        // instead be this command thread having been delayed for an unrelated
        // reason while real audio kept flowing via cpal's own callback thread,
        // in which case that time is genuine and shouldn't be lost outright.
        let mut timer = PlaybackTimer::new();
        timer.start(5.0, 1.0);
        timer.current_position(Some(Duration::from_secs_f64(30.0)));

        // A 30s leap in the sink clock is capped at MAX_PLAUSIBLE_ELAPSED_DELTA_SECS
        // (2.0s), not credited in full and not discarded to 0.
        approx_eq(timer.current_position(Some(Duration::from_secs_f64(60.0))), 7.0);
        // Normal progress resumes from the new baseline.
        approx_eq(timer.current_position(Some(Duration::from_secs_f64(60.5))), 7.5);
    }

    #[test]
    fn negative_clock_deltas_credit_nothing() {
        // Unlike an implausibly large positive delta, a negative one has no
        // ambiguous "maybe legitimate" interpretation here -- get_pos() is
        // monotonic outside of a rescale, and every rate change already drops
        // the baseline via set_rate, so a negative diff seen through this path
        // is unambiguously a rescale artifact with no real elapsed time in it.
        let mut timer = PlaybackTimer::new();
        timer.start(5.0, 1.0);
        timer.current_position(Some(Duration::from_secs_f64(30.0)));

        approx_eq(timer.current_position(Some(Duration::from_secs_f64(10.0))), 5.0);
        // Progress resumes correctly from the new (lower) baseline.
        approx_eq(timer.current_position(Some(Duration::from_secs_f64(10.5))), 5.5);
    }

    #[test]
    fn set_rate_while_paused_does_not_start_the_clock() {
        let mut timer = PlaybackTimer::new();
        timer.start(0.0, 1.0);
        timer.pause(Some(Duration::ZERO));

        timer.set_rate(2.0, None);
        assert!(!timer.is_playing());
    }

    #[test]
    fn stop_resets_to_zero_and_not_playing() {
        let mut timer = PlaybackTimer::new();
        timer.start(50.0, 1.0);

        timer.stop();

        assert!(!timer.is_playing());
        approx_eq(timer.current_position(None), 0.0);
    }
}

fn get_default_device_name() -> Option<String> {
    cpal::default_host()
        .default_output_device()
        .and_then(|d| d.name().ok())
}

// The one place `AudioState::is_playing` is ever written, so the OS-level output stream
// can never drift out of sync with it -- every call site in `audio_thread` that used to set
// `state_guard.is_playing = x` directly now goes through here instead. Deliberately takes
// the already-locked state guard (rather than locking internally) since every call site
// already holds one for other fields it needs to set in the same critical section.
//
// `output_stream_is_playing` is compared against the desired state so a redundant call
// (e.g. two consecutive Pause commands) doesn't needlessly call into cpal every time --
// harmless either way, but this keeps the log line below meaningful instead of firing on
// every single state check.
fn set_playing_state(
    output_stream: &OutputStream,
    output_stream_is_playing: &mut bool,
    state: &mut AudioState,
    playing: bool,
) {
    state.is_playing = playing;
    if *output_stream_is_playing == playing {
        return;
    }
    let result = if playing {
        output_stream.play().map_err(|e| e.to_string())
    } else {
        output_stream.pause().map_err(|e| e.to_string())
    };
    match result {
        Ok(()) => *output_stream_is_playing = playing,
        Err(e) => eprintln!(
            "⚠️ Failed to {} output stream: {}",
            if playing { "resume" } else { "pause" },
            e
        ),
    }
}

// The dedicated audio thread - owns OutputStream and Sink
fn audio_thread(
    mut command_rx: mpsc::UnboundedReceiver<AudioCommand>,
    command_tx: mpsc::UnboundedSender<AudioCommand>,
    state: Arc<Mutex<AudioState>>,
    state_change_tx: std_mpsc::Sender<()>,
    track_ended_tx: std_mpsc::Sender<()>,
    analytics: Arc<Analytics>,
) {
    // Create audio output stream once for this thread
    let Ok((mut output_stream, mut stream_handle)) = OutputStream::try_default() else {
        eprintln!("❌ Failed to create audio output");
        return;
    };
    println!("✅ Audio output stream created");
    // OutputStream::try_from_device_config() (which try_default() calls internally) plays
    // the underlying cpal::Stream immediately on construction -- so it starts out emitting
    // silence to the OS as "actively producing sound" even though nothing is loaded yet.
    // Paired with `set_playing_state` below (the single place this ever changes), so the
    // OS-visible state always matches whatever this app is actually doing, instead of a
    // paused/idle app looking like it's still playing indefinitely -- see
    // vendor/rodio/README.md for why this needs a vendored rodio patch to be possible at all.
    let mut output_stream_is_playing = true;
    if let Err(e) = output_stream.pause() {
        eprintln!("⚠️ Failed to pause output stream at startup: {}", e);
    } else {
        output_stream_is_playing = false;
    }

    // Every state mutation below needs to tell set_app_handle's listener thread to
    // re-emit "playback-state-changed" -- factored out since this fires ~20 times
    // across the command-handling match below.
    let notify_state_change = || {
        let _ = state_change_tx.send(());
    };

    let mut current_sink: Option<Sink> = None;
    // Local file path or URL -- ffmpeg's `-i` flag treats both identically, so a
    // single field now covers what used to be three separate tracking variables.
    let mut current_source: Option<String> = None;
    let mut current_ffmpeg_child: Option<Child> = None; // ffmpeg process to kill on stop
    let mut current_ffmpeg_health: Option<StreamHealth> = None; // for error reporting + stall detection
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
    // How long ffmpeg may go without producing any new data before we treat it
    // as stuck and force a retry, rather than waiting on ffmpeg's own
    // -rw_timeout (which isn't reliably honored on every platform/TLS backend).
    const NETWORK_STALL_TIMEOUT_SECS: u64 = 4;
    // Last thing the dying ffmpeg process printed to stderr, kept around so the
    // eventual playback_failed analytics event (if we give up) can report the
    // real cause (CDN HTTP error, connection reset, etc.) instead of a bare count.
    let mut last_stream_error: String = String::new();
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

        // Check if track has ended (sink is empty) or ffmpeg has gone silent
        // (no data read in NETWORK_STALL_TIMEOUT_SECS) -- the latter catches a
        // mid-stream network death that sink.empty() alone can't see, since
        // rodio still considers the sink non-empty for as long as the source's
        // blocked read hasn't returned.
        if let Some(sink) = &current_sink {
            let stalled = current_ffmpeg_health
                .as_ref()
                .and_then(|h| h.last_data_at.lock().ok())
                .map(|t| t.elapsed() > std::time::Duration::from_secs(NETWORK_STALL_TIMEOUT_SECS))
                .unwrap_or(false);

            if (sink.empty() || stalled) && position_timer.is_playing() {
                let current_pos = position_timer.current_position(Some(sink.get_pos()));
                let duration = state.blocking_lock().duration;

                // Only trigger "track ended" if position is actually near the end
                if duration <= 0.0 || current_pos >= duration - 3.0 {
                    println!("🏁 Track ended (sink empty)");
                    position_timer.stop();

                    let mut state_guard = state.blocking_lock();
                    set_playing_state(&output_stream, &mut output_stream_is_playing, &mut state_guard, false);
                    state_guard.current_position = duration;
                    drop(state_guard);

                    notify_state_change();
                    let _ = track_ended_tx.send(());

                    current_sink = None;
                } else {
                    // Stream died / stalled -- keep retrying (same URL first, then
                    // a freshly re-resolved one) until it recovers or we truly give
                    // up. This used to be a single attempt: a retry that itself
                    // failed (e.g. wifi still off) left current_sink as None with
                    // nothing left to ever re-trigger another one, so the app got
                    // stuck showing "loading" forever even after the network came
                    // back, instead of continuing to retry like MAX_AUTO_RETRIES
                    // implies it should.
                    'retry: loop {
                        consecutive_premature_ends += 1;
                        println!("⚠️ Stream ended prematurely at {:.1}s (duration: {:.1}s) - auto-retry {}/{}", current_pos, duration, consecutive_premature_ends, MAX_AUTO_RETRIES);

                        // Tell the frontend immediately, before attempting the retry
                        // itself -- otherwise it keeps showing "playing" with the
                        // position ticking for however long the retry takes, purely
                        // because nothing told it otherwise yet.
                        position_timer.pause(current_sink.as_ref().map(|s| s.get_pos()));
                        {
                            let mut state_guard = state.blocking_lock();
                            set_playing_state(&output_stream, &mut output_stream_is_playing, &mut state_guard, false);
                            state_guard.is_loading = true;
                        }
                        notify_state_change();

                        // Surface whatever the dying ffmpeg process printed to stderr --
                        // this is where an actual HTTP error (403, 404, connection reset,
                        // etc.) from the CDN would show up, instead of just "it stopped."
                        if let Some(health) = current_ffmpeg_health.take() {
                            if let Ok(log) = health.stderr.lock() {
                                if !log.trim().is_empty() {
                                    eprintln!("🔎 ffmpeg stderr from failed stream: {}", log.trim());
                                    last_stream_error = log.trim().to_string();
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
                                    notify_state_change();
                                    spawn_url_resolution_worker(track, &command_tx, &play_generation, current_pos);
                                    break 'retry; // hand off to the async re-resolution path
                                }
                            }

                            eprintln!("❌ Giving up after {} failed retries - stopping playback", MAX_AUTO_RETRIES);
                            analytics.track_with_data(
                                "playback_failed",
                                json!({
                                    "stage": "stream_died",
                                    "reason": if last_stream_error.is_empty() {
                                        "no ffmpeg stderr captured".to_string()
                                    } else {
                                        truncate_for_analytics(&last_stream_error)
                                    },
                                }),
                            );
                            position_timer.stop();
                            current_source = None;
                            let mut state_guard = state.blocking_lock();
                            set_playing_state(&output_stream, &mut output_stream_is_playing, &mut state_guard, false);
                            state_guard.is_loading = false;
                            state_guard.current_position = current_pos;
                            state_guard.playback_error = Some(
                                "Playback failed after multiple retries. The track may be unavailable.".to_string()
                            );
                            drop(state_guard);
                            notify_state_change();
                            break 'retry;
                        }

                        let Some(source) = current_source.clone() else {
                            break 'retry;
                        };

                        // Give a pending command (Stop, Pause, a fresh Play, ...)
                        // a chance to interrupt the ladder between attempts,
                        // instead of only ever being looked at once the whole
                        // thing finishes or gives up -- each attempt's own
                        // spawn_ffmpeg_pcm_stream call can itself block for
                        // several seconds, so without this a Stop pressed mid-
                        // ladder would otherwise sit unprocessed for multiple
                        // attempts in a row.
                        if let Ok(cmd) = command_rx.try_recv() {
                            println!("⏭️ Command received during retry ladder, deferring to outer loop");
                            // Stop/Pause abandon the ladder outright with nothing
                            // else left to clear the loading spinner this set --
                            // clear it here rather than in their own handlers,
                            // since those are also reached by a routine, already-
                            // resolved Stop-before-Play (see play_track), where
                            // clearing it would wipe out the fresh load that's
                            // about to start. A deferred Play/Seek/etc. manages
                            // is_loading itself once it actually runs.
                            if matches!(cmd, AudioCommand::Stop | AudioCommand::Pause) {
                                let mut state_guard = state.blocking_lock();
                                state_guard.is_loading = false;
                            }
                            pending_command = Some(cmd);
                            break 'retry;
                        }

                        match spawn_ffmpeg_pcm_stream(&source, current_pos) {
                            Ok((mut child, ffmpeg_source, stderr_log)) => {
                                let Ok(new_sink) = Sink::try_new(&stream_handle) else {
                                    eprintln!("❌ Failed to create sink for retry");
                                    let _ = child.kill();
                                    last_stream_error = "Failed to create audio sink".to_string();
                                    std::thread::sleep(std::time::Duration::from_millis(500));
                                    continue 'retry;
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
                                current_ffmpeg_health = Some(stderr_log);

                                // A real recovery, not just this loop's own retries --
                                // reset so a later, unrelated drop starts its own fresh
                                // count instead of inheriting this one's, which would
                                // otherwise make a connection with several separate,
                                // individually-successful reconnects eventually exceed
                                // MAX_AUTO_RETRIES and force a re-resolve/give-up even
                                // though the stream had been healthy in between.
                                consecutive_premature_ends = 0;

                                // Keep timer running from current position
                                position_timer.start(current_pos, rate);
                                last_position_update = Instant::now();

                                // Flip back out of the loading state set when we
                                // detected the stall above, now that it's actually
                                // recovered.
                                {
                                    let mut state_guard = state.blocking_lock();
                                    set_playing_state(&output_stream, &mut output_stream_is_playing, &mut state_guard, true);
                                    state_guard.is_loading = false;
                                }
                                notify_state_change();

                                println!("✅ Stream auto-retried from {:.1}s", current_pos);
                                break 'retry;
                            }
                            Err(e) => {
                                eprintln!("❌ Auto-retry failed: {}", e);
                                last_stream_error = e;
                                // Brief backoff before looping back to try again --
                                // otherwise a still-dead network makes this spin as
                                // fast as spawn_ffmpeg_pcm_stream can fail.
                                std::thread::sleep(std::time::Duration::from_millis(500));
                                continue 'retry;
                            }
                        }
                    }
                }
            }
        }

        // Periodically update position in state (every 500ms)
        if position_timer.is_playing() && last_position_update.elapsed() > std::time::Duration::from_millis(500) {
            let current_pos = position_timer.current_position(current_sink.as_ref().map(|s| s.get_pos()));
            let duration = state.blocking_lock().duration;

            // Don't exceed duration
            let clamped_pos = current_pos.min(duration);

            {
                let mut state_guard = state.blocking_lock();
                state_guard.current_position = clamped_pos;
                // 1.0 = seeking available (any source, since ffmpeg can seek by path or URL)
                state_guard.download_progress = if current_source.is_some() { 1.0 } else { 0.0 };
            }
            notify_state_change();
            last_position_update = Instant::now();
        }

        // Check for audio device changes every 2 seconds
        if last_device_check.elapsed() > std::time::Duration::from_secs(2) {
            last_device_check = Instant::now();
            let current_device = get_default_device_name();
            if current_device != last_known_device {
                println!("🔊 Audio device changed ({:?} → {:?}), reinitializing...", last_known_device, current_device);
                last_known_device = current_device;

                let current_pos = position_timer.current_position(current_sink.as_ref().map(|s| s.get_pos()));
                let was_playing = position_timer.is_playing();

                if let Ok((new_stream, new_handle)) = OutputStream::try_default() {
                    output_stream = new_stream;
                    stream_handle = new_handle;
                    // A freshly constructed OutputStream always starts out actually
                    // playing (see the comment where output_stream is first created) --
                    // reset the tracked flag to match reality before the sync below,
                    // otherwise a stale `false` from before this reassignment would make
                    // set_playing_state wrongly think it's already paused and skip
                    // calling pause() on this brand new stream.
                    output_stream_is_playing = true;

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
                    } else {
                        // No seek is coming to correct the stream's play/pause state
                        // (that only happens in the was_playing branch above), so sync
                        // it here directly -- otherwise a device change while paused
                        // would leave the new stream sitting unpaused indefinitely.
                        let mut state_guard = state.blocking_lock();
                        set_playing_state(&output_stream, &mut output_stream_is_playing, &mut state_guard, false);
                    }
                    println!("✅ Audio device reinitialized successfully");
                }
            }
        }

        let Some(command) = command else {
            // No command: only poll fast (50ms) while a track is actually loaded and
            // needs its position/sink-empty checked at that cadence. With nothing
            // loaded there's nothing to update, so back off to a much slower poll
            // (still bounded, so a new Play command is picked up almost immediately)
            // instead of spinning this thread at 20Hz indefinitely at idle.
            let idle_sleep = if current_sink.is_some() {
                std::time::Duration::from_millis(50)
            } else {
                std::time::Duration::from_millis(250)
            };
            std::thread::sleep(idle_sleep);
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
                current_ffmpeg_health = None;
                consecutive_premature_ends = 0;
                has_reresolved = false;
                current_streaming_track = Some(track.clone());
                {
                    let mut state_guard = state.blocking_lock();
                    state_guard.playback_error = None;
                }

                println!("📥 Getting audio URL from yt-dlp...");
                spawn_url_resolution_worker(track, &command_tx, &play_generation, 0.0);
            }
            AudioCommand::UrlResolved(track, generation, result, resume_position) => {
                if play_generation.load(Ordering::SeqCst) != generation {
                    println!("⏭️ Discarding stale resolved URL for: {}", track.title);
                    continue;
                }

                let audio_url = match result {
                    Ok(url) => url,
                    Err(e) => {
                        eprintln!("❌ Failed to get audio URL: {}", e);
                        analytics.track_with_data(
                            "playback_failed",
                            json!({
                                "stage": "resolve_url",
                                "reason": truncate_for_analytics(&e),
                            }),
                        );
                        let mut state_guard = state.blocking_lock();
                        state_guard.is_loading = false;
                        set_playing_state(&output_stream, &mut output_stream_is_playing, &mut state_guard, false);
                        state_guard.playback_error = Some(
                            "Couldn't load this track. It may be unavailable or region-restricted.".to_string()
                        );
                        drop(state_guard);
                        notify_state_change();
                        continue;
                    }
                };

                println!("✅ Got audio URL, starting ffmpeg stream from {:.1}s...", resume_position);

                // Retry starting this URL a few times, then re-resolve a fresh
                // one, before finally giving up -- mirrors the mid-stream
                // retry ladder below, but for a track that never even started
                // playing yet. A start_stream failure right here is often a
                // signed CDN URL (googlevideo.com) that expired between
                // resolution and this first real request, or a transient 403
                // from a flaky edge node -- both are commonly fixed by simply
                // trying again a moment later, or with a freshly re-resolved
                // URL, rather than being permanent. Previously this gave up
                // outright on the very first attempt with no retry at all.
                'initial_start: loop {
                    match spawn_ffmpeg_pcm_stream(&audio_url, resume_position) {
                        Ok((mut child, source, stderr_log)) => {
                            let Ok(sink) = Sink::try_new(&stream_handle) else {
                                eprintln!("❌ Failed to create sink");
                                let _ = child.kill();
                                break 'initial_start;
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
                            current_ffmpeg_health = Some(stderr_log);
                            current_source = Some(audio_url.clone());

                            // A real recovery, not a fresh problem -- reset so
                            // a later, unrelated mid-stream drop starts its
                            // own count/re-resolve budget instead of
                            // inheriting whatever this attempt already used.
                            consecutive_premature_ends = 0;
                            has_reresolved = false;

                            // Start position timer
                            position_timer.start(resume_position, rate);
                            last_position_update = Instant::now();

                            // Update state
                            {
                                let mut state_guard = state.blocking_lock();
                                state_guard.is_loading = false;
                                set_playing_state(&output_stream, &mut output_stream_is_playing, &mut state_guard, true);
                                state_guard.current_position = resume_position;
                                state_guard.download_progress = 1.0;
                            }
                            notify_state_change();

                            println!("▶️ Streaming: {}", track.title);
                            break 'initial_start;
                        }
                        Err(e) => {
                            consecutive_premature_ends += 1;
                            eprintln!("❌ Failed to start stream (attempt {}/{}): {}", consecutive_premature_ends, MAX_AUTO_RETRIES, e);
                            last_stream_error = e;

                            if consecutive_premature_ends <= MAX_AUTO_RETRIES {
                                std::thread::sleep(std::time::Duration::from_millis(500));
                                continue 'initial_start;
                            }

                            if !has_reresolved {
                                has_reresolved = true;
                                consecutive_premature_ends = 0;
                                eprintln!("⚠️ Giving up on this URL after {} failed start attempts - re-resolving a fresh audio URL", MAX_AUTO_RETRIES);
                                {
                                    let mut state_guard = state.blocking_lock();
                                    state_guard.is_loading = true;
                                }
                                notify_state_change();
                                spawn_url_resolution_worker(track.clone(), &command_tx, &play_generation, resume_position);
                                break 'initial_start;
                            }

                            eprintln!("❌ Giving up after exhausting retries and one re-resolution - stopping playback");
                            analytics.track_with_data(
                                "playback_failed",
                                json!({
                                    "stage": "start_stream",
                                    "reason": truncate_for_analytics(&last_stream_error),
                                }),
                            );
                            let mut state_guard = state.blocking_lock();
                            state_guard.is_loading = false;
                            set_playing_state(&output_stream, &mut output_stream_is_playing, &mut state_guard, false);
                            state_guard.playback_error = Some(
                                "Couldn't load this track. It may be unavailable or region-restricted.".to_string()
                            );
                            drop(state_guard);
                            notify_state_change();
                            break 'initial_start;
                        }
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
                current_ffmpeg_health = None;
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
                        current_ffmpeg_health = Some(stderr_log);
                        current_source = Some(file_path.clone());

                        position_timer.start(0.0, rate);
                        last_position_update = Instant::now();

                        {
                            let mut state_guard = state.blocking_lock();
                            state_guard.is_loading = false;
                            set_playing_state(&output_stream, &mut output_stream_is_playing, &mut state_guard, true);
                            state_guard.current_position = 0.0;
                            state_guard.download_progress = 1.0;
                        }
                        notify_state_change();

                        println!("▶️ Playing: {}", track.title);
                    }
                    Err(e) => {
                        eprintln!("❌ Failed to play file: {}", e);
                        let mut state_guard = state.blocking_lock();
                        state_guard.is_loading = false;
                        set_playing_state(&output_stream, &mut output_stream_is_playing, &mut state_guard, false);
                        drop(state_guard);
                        notify_state_change();
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
                        Ok(other) => {
                            // Hand it to the next loop iteration instead of
                            // dropping it. Releasing a seek slider and
                            // immediately clicking another track queues the
                            // Play behind a burst of Seeks -- discarding it
                            // left the UI stuck on "loading" while the previous
                            // track kept playing.
                            pending_command = Some(other);
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
                notify_state_change();

                // Stop current playback
                if let Some(sink) = current_sink.take() {
                    sink.stop();
                }
                if let Some(mut child) = current_ffmpeg_child.take() {
                    let _ = child.kill();
                }
                current_ffmpeg_health = None;
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
                                notify_state_change();
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
                            current_ffmpeg_health = Some(stderr_log);

                            last_position_update = Instant::now();

                            {
                                let mut state_guard = state.blocking_lock();
                                state_guard.current_position = position;
                                set_playing_state(&output_stream, &mut output_stream_is_playing, &mut state_guard, was_playing);
                                state_guard.is_loading = false;
                            }
                            notify_state_change();

                            let seek_ms = seek_start.elapsed().as_secs_f64() * 1000.0;
                            println!("⏩ Seeked to {:.1}s - took {:.1}ms", position, seek_ms);
                        }
                        Err(e) => {
                            eprintln!("❌ Failed to seek: {}", e);
                            let mut state_guard = state.blocking_lock();
                            state_guard.is_loading = false;
                            drop(state_guard);
                            notify_state_change();
                        }
                    }
                } else {
                    let mut state_guard = state.blocking_lock();
                    state_guard.is_loading = false;
                    drop(state_guard);
                    notify_state_change();
                }
            }
            AudioCommand::TogglePlayPause => {
                let state_guard = state.blocking_lock();
                let is_playing = state_guard.is_playing;
                let duration = state_guard.duration;
                let current_pos = position_timer.current_position(current_sink.as_ref().map(|s| s.get_pos()));
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
                        let paused_pos = position_timer.pause(Some(sink.get_pos()));
                        let mut state_guard = state.blocking_lock();
                        set_playing_state(&output_stream, &mut output_stream_is_playing, &mut state_guard, false);
                        state_guard.current_position = paused_pos;
                        println!("⏸️ Paused at {:.1}s", paused_pos);
                        drop(state_guard);
                        notify_state_change();
                    }
                } else if track_ended {
                    // Only a genuine end-of-track should restart from 0 -- the
                    // other thing that makes track_ended true is current_sink
                    // being None because a Pause/Stop got deferred mid-retry-
                    // ladder (see AudioCommand::Pause), which can leave current_pos
                    // anywhere in the middle of the track. Restarting that case
                    // from 0.0 would silently discard however far the user had
                    // actually listened.
                    let is_genuinely_at_end = current_pos >= duration - 0.5 && duration > 0.0;
                    let restart_position = if is_genuinely_at_end { 0.0 } else { current_pos };

                    if let Some(sink) = current_sink.take() {
                        sink.stop();
                    }
                    if let Some(mut child) = current_ffmpeg_child.take() {
                        let _ = child.kill();
                    }
                    current_ffmpeg_health = None;
                    consecutive_premature_ends = 0;

                    if let Some(source) = current_source.clone() {
                        match spawn_ffmpeg_pcm_stream(&source, restart_position) {
                            Ok((mut child, ffmpeg_source, stderr_log)) => {
                                if let Ok(sink) = Sink::try_new(&stream_handle) {
                                    sink.set_volume(volume);
                                    sink.set_speed(rate);
                                    sink.append(ffmpeg_source.convert_samples::<f32>());
                                    sink.play();
                                    current_sink = Some(sink);
                                    current_ffmpeg_child = Some(child);
                                    current_ffmpeg_health = Some(stderr_log);

                                    position_timer.start(restart_position, rate);
                                    last_position_update = Instant::now();

                                    let mut state_guard = state.blocking_lock();
                                    set_playing_state(&output_stream, &mut output_stream_is_playing, &mut state_guard, true);
                                    state_guard.current_position = restart_position;
                                    drop(state_guard);
                                    notify_state_change();
                                    println!("🔄 Restarted playback from {:.1}s", restart_position);
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
                        // The network-stall watchdog's last_data_at freezes the
                        // instant the sink is paused -- rodio stops pulling from
                        // the source at all while paused, so read_chunk() never
                        // runs and never refreshes it. Without this reset, any
                        // pause longer than NETWORK_STALL_TIMEOUT_SECS makes the
                        // very next watchdog check after resuming immediately
                        // treat a perfectly healthy, just-resumed stream as a
                        // dead network and tear it down into the retry ladder.
                        if let Some(health) = &current_ffmpeg_health {
                            if let Ok(mut t) = health.last_data_at.lock() {
                                *t = Instant::now();
                            }
                        }
                        sink.play();
                        // start() drops the sink-clock baseline, so resuming on this
                        // reused sink (whose clock is already wherever it was when
                        // paused, not 0) just re-baselines on the next reading rather
                        // than differencing against a pre-pause value.
                        position_timer.start(current_pos, rate);
                        let mut state_guard = state.blocking_lock();
                        set_playing_state(&output_stream, &mut output_stream_is_playing, &mut state_guard, true);
                        state_guard.current_position = current_pos;
                        println!("▶️ Resumed from {:.1}s (rate: {:.2})", current_pos, rate);
                        drop(state_guard);
                        last_position_update = Instant::now();
                        notify_state_change();
                    }
                }
            }
            AudioCommand::Pause => {
                // Unconditionally, not just `if let Some(sink)`: if this Pause was
                // deferred mid-retry-ladder (current_sink already torn down to
                // None while retrying), the ladder already set is_loading=true --
                // nothing else clears it, so without this the UI is left showing
                // a permanent loading spinner with no track and no way to self-
                // correct short of starting an entirely new Play.
                let current_pos = position_timer.pause(current_sink.as_ref().map(|s| s.get_pos()));
                if let Some(sink) = &current_sink {
                    sink.pause();
                }
                let mut state_guard = state.blocking_lock();
                set_playing_state(&output_stream, &mut output_stream_is_playing, &mut state_guard, false);
                state_guard.is_loading = false;
                state_guard.current_position = current_pos;
                println!("⏸️ Explicit pause at {:.1}s", current_pos);
                drop(state_guard);
                notify_state_change();
            }
            AudioCommand::Stop => {
                if let Some(sink) = current_sink.take() {
                    sink.stop();
                }
                if let Some(mut child) = current_ffmpeg_child.take() {
                    let _ = child.kill();
                }
                current_source = None;
                current_ffmpeg_health = None;
                consecutive_premature_ends = 0;
                has_reresolved = false;
                current_streaming_track = None;
                play_generation.fetch_add(1, Ordering::SeqCst);
                position_timer.stop();
                let mut state_guard = state.blocking_lock();
                set_playing_state(&output_stream, &mut output_stream_is_playing, &mut state_guard, false);
                // Not is_loading: Stop is also sent routinely as a "clean up
                // the previous track first" step before every Play (see
                // play_track), enqueued moments before that Play's own
                // set_loading_state() already set is_loading=true. Since Stop
                // is processed asynchronously by this thread, clearing it
                // here can land *after* that fresh true, wiping out the very
                // loading state the new track just started showing. The one
                // case that actually needs clearing it -- Stop abandoning a
                // stuck retry ladder -- is handled right where that's
                // detected instead (see the mid-ladder interrupt check above).
                state_guard.current_position = 0.0;
                state_guard.playback_error = None;
                drop(state_guard);
                notify_state_change();
                println!("⏹️ Stopped");
            }
            AudioCommand::SetVolume(volume) => {
                if let Some(sink) = &current_sink {
                    sink.set_volume(volume);
                }
            }
            AudioCommand::SetPlaybackRate(mut rate) => {
                // Same reasoning as Seek's own draining loop below: a rate
                // slider fires one of these per drag tick, faster than
                // rodio's own get_pos() refreshes internally (~5ms) -- reading
                // it once per intermediate value re-anchors the position
                // timer against the same stale elapsed reading more than
                // once, which is where the visible position jump/negative-
                // value bug came from. Collapsing a burst to just the final
                // rate means only one get_pos() read (and one re-anchor) ever
                // happens per drag, against whatever the real elapsed value
                // is by the time the user actually settles on a rate.
                loop {
                    match command_rx.try_recv() {
                        Ok(AudioCommand::SetPlaybackRate(new_rate)) => {
                            rate = new_rate;
                        }
                        Ok(other) => {
                            pending_command = Some(other);
                            break;
                        }
                        Err(_) => break,
                    }
                }

                if let Some(sink) = &current_sink {
                    // Read the sink clock BEFORE changing speed: it's still in
                    // the old scale here, which is the scale the timer's own
                    // baseline was taken in, so progress since that baseline
                    // banks correctly at the old rate.
                    let elapsed = sink.get_pos();
                    sink.set_speed(rate);
                    position_timer.set_rate(rate, Some(elapsed));

                    // rodio applies the speed change (and the retroactive
                    // rescale of its position clock that comes with it)
                    // asynchronously, via a ~5ms periodic access on the audio
                    // thread. Reading the clock right now would capture a
                    // still-old-scale value as the new baseline, so the next
                    // 500ms tick would measure a bogus rescale-inflated delta,
                    // reject it, and spend itself re-baselining instead --
                    // costing an extra half second before the position visibly
                    // starts moving at the new rate. A short wait lets the new
                    // scale settle so the baseline below is taken in it.
                    std::thread::sleep(std::time::Duration::from_millis(15));

                    // Every other position-affecting handler (Resume, Seek,
                    // restart-after-end) immediately pushes the new position
                    // to state, so the frontend updates right away instead of
                    // waiting for the next periodic tick. This one didn't --
                    // so on a speed change the displayed position sat frozen
                    // at its pre-change value (the underlying math was always
                    // correct, it just wasn't being shown), then lurched to
                    // catch up when the next unrelated tick fired.
                    let current_pos = position_timer.current_position(Some(sink.get_pos()));
                    {
                        let mut state_guard = state.blocking_lock();
                        state_guard.current_position = current_pos;
                        state_guard.playback_rate = rate;
                    }
                    notify_state_change();
                    last_position_update = Instant::now();
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
                current_ffmpeg_health = None;
                consecutive_premature_ends = 0;
                has_reresolved = false;
                current_streaming_track = None;
                play_generation.fetch_add(1, Ordering::SeqCst);

                // Reinitialize audio output device
                match OutputStream::try_default() {
                    Ok((new_stream, new_handle)) => {
                        output_stream = new_stream;
                        stream_handle = new_handle;
                        // Freshly constructed, so it's actually playing again -- see the
                        // matching comment at the other OutputStream::try_default() call
                        // site above for why this has to be reset before the sync below.
                        output_stream_is_playing = true;
                        println!("✅ Audio output device reinitialized");
                    }
                    Err(e) => {
                        eprintln!("❌ Failed to reinitialize audio device: {}", e);
                    }
                }

                let mut state_guard = state.blocking_lock();
                set_playing_state(&output_stream, &mut output_stream_is_playing, &mut state_guard, false);
                state_guard.current_track = None;
                drop(state_guard);
                notify_state_change();
            }
        }
    }
}
