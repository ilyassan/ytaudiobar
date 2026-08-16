use souvlaki::{MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig};
use std::sync::Arc;
use tokio::sync::Mutex;
use tauri::{AppHandle, Emitter};
// Only the Windows branch of `initialize` needs a window handle, and
// `get_webview_window` comes from this trait -- so the import is unused (and
// warns) on every other platform.
#[cfg(target_os = "windows")]
use tauri::Manager;

pub struct MediaKeyManager {
    controls: Arc<Mutex<Option<MediaControls>>>,
    app_handle: Arc<Mutex<Option<AppHandle>>>,
}

impl MediaKeyManager {
    pub fn new() -> Self {
        Self {
            controls: Arc::new(Mutex::new(None)),
            app_handle: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn initialize(&self, app_handle: AppHandle) -> Result<(), String> {
        *self.app_handle.lock().await = Some(app_handle.clone());

        // Get window handle for Windows platform
        #[cfg(target_os = "windows")]
        let hwnd = {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};

            if let Some(window) = app_handle.get_webview_window("main") {
                let handle_result: Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> = window.window_handle();
                match handle_result {
                    Ok(handle) => {
                        let raw: RawWindowHandle = handle.as_raw();
                        match raw {
                            RawWindowHandle::Win32(win32_handle) => {
                                Some(win32_handle.hwnd.get() as *mut std::ffi::c_void)
                            }
                            _ => None
                        }
                    }
                    Err(_) => None
                }
            } else {
                None
            }
        };

        #[cfg(not(target_os = "windows"))]
        let hwnd = None;

        // Configure platform settings
        let platform_config = PlatformConfig {
            display_name: "YTAudioBar",
            dbus_name: "ytaudiobar",
            hwnd,
        };

        // Create media controls
        let mut controls = match MediaControls::new(platform_config) {
            Ok(controls) => controls,
            Err(e) => {
                eprintln!("Failed to create media controls: {:?}", e);
                return Err(format!("Failed to create media controls: {:?}", e));
            }
        };

        // Attach event handler
        let app_handle_clone = app_handle.clone();
        if let Err(e) = controls.attach(move |event| {
            let app_handle = app_handle_clone.clone();
            // Use Tauri's async runtime instead of tokio::spawn
            tauri::async_runtime::spawn(async move {
                handle_media_event(event, app_handle).await;
            });
        }) {
            eprintln!("Failed to attach media controls: {:?}", e);
            return Err(format!("Failed to attach media controls: {:?}", e));
        }

        *self.controls.lock().await = Some(controls);
        println!("🎹 MediaKeyManager: Initialized successfully");

        Ok(())
    }

    pub async fn update_metadata(&self, title: String, artist: String, duration: f64, cover_url: Option<String>) {
        if let Some(controls) = self.controls.lock().await.as_mut() {
            // Duration::from_secs_f64 panics on a negative (or NaN/infinite)
            // value -- guard against whatever produced track.duration for a
            // given track, same as update_playback_state does for position.
            let duration = if duration.is_finite() { duration.max(0.0) } else { 0.0 };
            let metadata = MediaMetadata {
                title: Some(&title),
                artist: Some(&artist),
                album: Some("YouTube"),
                duration: Some(std::time::Duration::from_secs_f64(duration)),
                cover_url: cover_url.as_deref(),
            };

            if let Err(e) = controls.set_metadata(metadata) {
                eprintln!("Failed to set metadata: {:?}", e);
            }
        }
    }

    pub async fn update_playback_state(&self, is_playing: bool, position: f64, _duration: f64) {
        if let Some(controls) = self.controls.lock().await.as_mut() {
            // Duration::from_secs_f64 panics on a negative (or NaN/infinite)
            // value -- this crosses from the frontend's own copy of the
            // position, so it's a second, independent line of defense against
            // whatever upstream state glitch might produce one, not just a
            // formality.
            let position = if position.is_finite() { position.max(0.0) } else { 0.0 };
            let playback = if is_playing {
                MediaPlayback::Playing { progress: Some(MediaPosition(std::time::Duration::from_secs_f64(position))) }
            } else {
                MediaPlayback::Paused { progress: Some(MediaPosition(std::time::Duration::from_secs_f64(position))) }
            };

            if let Err(e) = controls.set_playback(playback) {
                eprintln!("Failed to set playback state: {:?}", e);
            }
        }
    }

    pub async fn clear(&self) {
        if let Some(controls) = self.controls.lock().await.as_mut() {
            if let Err(e) = controls.set_playback(MediaPlayback::Stopped) {
                eprintln!("Failed to clear playback: {:?}", e);
            }
        }
    }
}

async fn handle_media_event(event: MediaControlEvent, app_handle: AppHandle) {
    match event {
        MediaControlEvent::Play => {
            println!("🎹 Media Key: Play");
            let _ = app_handle.emit("media-key-play", ());
        }
        MediaControlEvent::Pause => {
            println!("🎹 Media Key: Pause");
            let _ = app_handle.emit("media-key-pause", ());
        }
        MediaControlEvent::Toggle => {
            println!("🎹 Media Key: Toggle Play/Pause");
            let _ = app_handle.emit("media-key-toggle", ());
        }
        MediaControlEvent::Next => {
            println!("🎹 Media Key: Next Track");
            let _ = app_handle.emit("media-key-next", ());
        }
        MediaControlEvent::Previous => {
            println!("🎹 Media Key: Previous Track");
            let _ = app_handle.emit("media-key-previous", ());
        }
        MediaControlEvent::SeekBy(direction, duration) => {
            let seconds = duration.as_secs_f64();
            let offset = match direction {
                souvlaki::SeekDirection::Forward => seconds,
                souvlaki::SeekDirection::Backward => -seconds,
            };
            println!("🎹 Media Key: Seek by {} seconds", offset);
            let _ = app_handle.emit("media-key-seek", offset);
        }
        MediaControlEvent::SetPosition(position) => {
            let seconds = position.0.as_secs_f64();
            println!("🎹 Media Key: Seek to {} seconds", seconds);
            let _ = app_handle.emit("media-key-seek-to", seconds);
        }
        MediaControlEvent::Stop => {
            println!("🎹 Media Key: Stop");
            let _ = app_handle.emit("media-key-stop", ());
        }
        _ => {}
    }
}
