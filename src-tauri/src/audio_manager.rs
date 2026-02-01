use crate::models::{AudioState, YTVideoInfo};
use crate::ytdlp_installer::YTDLPInstaller;
use rodio::{buffer::SamplesBuffer, OutputStream, Sink, Source};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, Mutex};
use tauri::{AppHandle, Emitter};
use std::sync::mpsc as std_mpsc;

// Commands that can be sent to the audio thread
enum AudioCommand {
    Play(YTVideoInfo),
    TogglePlayPause,
    Pause,
    Stop,
    Seek(f64), // position in seconds
    SetVolume(f32),
    SetPlaybackRate(f32),
}

pub struct AudioManager {
    state: Arc<Mutex<AudioState>>,
    command_tx: mpsc::UnboundedSender<AudioCommand>,
    app_handle: Arc<Mutex<Option<AppHandle>>>,
    state_change_rx: Arc<Mutex<std_mpsc::Receiver<()>>>,
}

impl AudioManager {
    pub fn new() -> Self {
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let (state_change_tx, state_change_rx) = std_mpsc::channel();
        let state = Arc::new(Mutex::new(AudioState::default()));

        // Spawn dedicated audio thread
        let state_clone = Arc::clone(&state);
        std::thread::spawn(move || {
            audio_thread(command_rx, state_clone, state_change_tx);
        });

        Self {
            state,
            command_tx,
            app_handle: Arc::new(Mutex::new(None)),
            state_change_rx: Arc::new(Mutex::new(state_change_rx)),
        }
    }

    pub async fn set_app_handle(&self, handle: AppHandle) {
        *self.app_handle.lock().await = Some(handle.clone());

        // Spawn a task to listen for state changes and emit events
        let state = Arc::clone(&self.state);
        let state_change_rx = Arc::clone(&self.state_change_rx);

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

                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        });
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

        // Send play command to audio thread
        self.command_tx
            .send(AudioCommand::Play(track))
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

// The dedicated audio thread - owns OutputStream and Sink
fn audio_thread(
    mut command_rx: mpsc::UnboundedReceiver<AudioCommand>,
    state: Arc<Mutex<AudioState>>,
    state_change_tx: std_mpsc::Sender<()>,
) {
    // Create audio output stream once for this thread
    let Ok((_stream, stream_handle)) = OutputStream::try_default() else {
        eprintln!("❌ Failed to create audio output");
        return;
    };
    println!("✅ Audio output stream created");

    let mut current_sink: Option<Sink> = None;
    let mut current_samples: Option<Vec<i16>> = None; // Store samples for seeking
    let mut position_timer = PlaybackTimer::new(); // Track playback position
    let mut last_position_update = Instant::now();

    // Process commands with polling to allow periodic position updates
    loop {
        // Try to receive a command (non-blocking)
        let command = command_rx.try_recv().ok();

        // Check if track has ended (sink is empty)
        if let Some(sink) = &current_sink {
            if sink.empty() && position_timer.is_playing() {
                println!("🏁 Track ended (sink empty)");
                position_timer.stop();
                // Keep current_samples so we can restart the track if user presses play

                let mut state_guard = state.blocking_lock();
                let duration = state_guard.duration;
                state_guard.is_playing = false;
                state_guard.current_position = duration; // Set to exact duration
                drop(state_guard);

                let _ = state_change_tx.send(());
                current_sink = None; // Clear sink to stop the empty check, but samples remain
            }
        }

        // Periodically update position in state while playing (every 500ms)
        if position_timer.is_playing() && last_position_update.elapsed() > std::time::Duration::from_millis(500) {
            let current_pos = position_timer.current_position();
            let duration = state.blocking_lock().duration;

            // Don't exceed duration
            let clamped_pos = current_pos.min(duration);
            {
                let mut state_guard = state.blocking_lock();
                state_guard.current_position = clamped_pos;
            }
            let _ = state_change_tx.send(());
            last_position_update = Instant::now();
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
                current_samples = None;

                let video_url = format!("https://www.youtube.com/watch?v={}", track.id);
                println!("📥 Fetching audio via yt-dlp + ffmpeg pipeline...");

                // Get yt-dlp path
                let ytdlp_path = YTDLPInstaller::get_ytdlp_path();

                // Use yt-dlp to pipe audio through ffmpeg to get raw PCM
                let ytdlp_child = match Command::new(&ytdlp_path)
                    .args(&[
                        "-f", "bestaudio",
                        "-o", "-",
                        "--no-warnings",
                        "--quiet",
                        &video_url,
                    ])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .spawn()
                {
                    Ok(child) => child,
                    Err(e) => {
                        eprintln!("❌ Failed to spawn yt-dlp: {}", e);
                        continue;
                    }
                };

                let ytdlp_stdout = match ytdlp_child.stdout {
                    Some(stdout) => stdout,
                    None => {
                        eprintln!("❌ Failed to capture yt-dlp stdout");
                        continue;
                    }
                };

                // Pipe yt-dlp output through ffmpeg to convert to raw PCM
                let ffmpeg_output = match Command::new("ffmpeg")
                    .args(&[
                        "-i", "pipe:0",
                        "-f", "s16le",
                        "-acodec", "pcm_s16le",
                        "-ar", &SAMPLE_RATE.to_string(),
                        "-ac", &CHANNELS.to_string(),
                        "-loglevel", "error",
                        "pipe:1",
                    ])
                    .stdin(ytdlp_stdout)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .output()
                {
                    Ok(output) => output,
                    Err(e) => {
                        eprintln!("❌ Failed to run ffmpeg: {}", e);
                        eprintln!("Make sure ffmpeg is installed and in PATH");
                        continue;
                    }
                };

                if !ffmpeg_output.status.success() {
                    eprintln!("❌ ffmpeg conversion failed");
                    continue;
                }

                let pcm_bytes = ffmpeg_output.stdout;
                println!("✅ Got {} bytes of raw PCM audio", pcm_bytes.len());

                if pcm_bytes.is_empty() {
                    eprintln!("❌ No audio data received");
                    continue;
                }

                // Convert bytes to i16 samples
                let samples: Vec<i16> = pcm_bytes
                    .chunks_exact(2)
                    .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
                    .collect();

                println!("✅ Converted to {} samples", samples.len());

                // Store samples for seeking
                current_samples = Some(samples.clone());

                // Create source and sink
                let source = SamplesBuffer::new(CHANNELS, SAMPLE_RATE, samples);

                println!("🔊 Creating audio sink...");
                let Ok(sink) = Sink::try_new(&stream_handle) else {
                    eprintln!("❌ Failed to create sink");
                    continue;
                };

                // Get current settings from state
                let (volume, rate) = {
                    let state_guard = state.blocking_lock();
                    (state_guard.volume, state_guard.playback_rate)
                };

                sink.set_volume(volume);
                sink.set_speed(rate);
                sink.append(source.convert_samples::<f32>());
                sink.play();

                current_sink = Some(sink);

                // Start position timer
                position_timer.start(0.0, rate);
                last_position_update = Instant::now();

                // Update state
                {
                    let mut state_guard = state.blocking_lock();
                    state_guard.is_loading = false;
                    state_guard.is_playing = true;
                    state_guard.current_position = 0.0;
                }
                let _ = state_change_tx.send(());

                println!("▶️ Playing: {} (position timer started at 0.0s)", track.title);
            }
            AudioCommand::Seek(position) => {
                if let Some(samples) = &current_samples {
                    // Stop current playback
                    if let Some(sink) = current_sink.take() {
                        sink.stop();
                    }

                    // Calculate sample index from position
                    // position_secs * sample_rate * channels = sample index
                    let sample_index = (position * SAMPLE_RATE as f64 * CHANNELS as f64) as usize;
                    let sample_index = sample_index.min(samples.len());

                    // Get samples from position onwards
                    let remaining_samples: Vec<i16> = samples[sample_index..].to_vec();

                    if remaining_samples.is_empty() {
                        println!("⏩ Seek position at end of track");
                        continue;
                    }

                    // Create source from remaining samples
                    let source = SamplesBuffer::new(CHANNELS, SAMPLE_RATE, remaining_samples);

                    // Create new sink
                    let Ok(sink) = Sink::try_new(&stream_handle) else {
                        eprintln!("❌ Failed to create sink for seek");
                        continue;
                    };

                    // Get current settings from state
                    let (volume, rate) = {
                        let state_guard = state.blocking_lock();
                        (state_guard.volume, state_guard.playback_rate)
                    };

                    sink.set_volume(volume);
                    sink.set_speed(rate);
                    sink.append(source.convert_samples::<f32>());
                    sink.play();

                    current_sink = Some(sink);

                    // Update position timer - always restart from seek position
                    position_timer.start(position, rate);
                    last_position_update = Instant::now();

                    // Update state with actual position
                    {
                        let mut state_guard = state.blocking_lock();
                        state_guard.current_position = position;
                        state_guard.is_playing = true;
                    }
                    let _ = state_change_tx.send(());

                    println!("⏩ Seeked to {:.1}s (timer reset to {:.1}s)", position, position);
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
                let track_ended = (current_pos >= duration - 0.5 && duration > 0.0) ||
                                  (current_samples.is_some() && current_sink.is_none());

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
                    if let Some(samples) = &current_samples {
                        // Stop current sink if exists
                        if let Some(sink) = current_sink.take() {
                            sink.stop();
                        }

                        // Create new sink from the beginning
                        let source = SamplesBuffer::new(CHANNELS, SAMPLE_RATE, samples.clone());
                        if let Ok(sink) = Sink::try_new(&stream_handle) {
                            sink.set_volume(volume);
                            sink.set_speed(rate);
                            sink.append(source.convert_samples::<f32>());
                            sink.play();
                            current_sink = Some(sink);

                            // Reset position timer to 0
                            position_timer.start(0.0, rate);
                            last_position_update = Instant::now();

                            let mut state_guard = state.blocking_lock();
                            state_guard.is_playing = true;
                            state_guard.current_position = 0.0;
                            drop(state_guard);
                            let _ = state_change_tx.send(());
                            println!("🔄 Restarted track from beginning");
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
                current_samples = None;
                position_timer.stop();
                let mut state_guard = state.blocking_lock();
                state_guard.is_playing = false;
                state_guard.current_position = 0.0;
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
        }
    }
}
