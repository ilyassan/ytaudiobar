// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod models;
mod database;
mod ytdlp_manager;
mod ytdlp_installer;
mod audio_manager;
mod queue_manager;

use std::sync::Arc;
use tauri::{
    Manager, State, WindowEvent, tray::{TrayIconBuilder, TrayIconEvent, MouseButton, MouseButtonState},
    menu::{Menu, MenuItem}
};

use crate::database::DatabaseManager;
use crate::models::{AudioState, RepeatMode, YTVideoInfo};
use crate::ytdlp_manager::YTDLPManager;
use crate::ytdlp_installer::YTDLPInstaller;
use crate::audio_manager::AudioManager;
use crate::queue_manager::QueueManager;

pub struct AppState {
    audio: Arc<AudioManager>,
    queue: Arc<QueueManager>,
    db: Arc<DatabaseManager>,
    ytdlp: Arc<YTDLPManager>,
}

#[tauri::command]
async fn search_youtube(
    query: String,
    music_mode: bool,
    state: State<'_, AppState>,
) -> Result<Vec<YTVideoInfo>, String> {
    state.ytdlp.search(query, music_mode).await
}

#[tauri::command]
async fn check_ytdlp_installed() -> Result<bool, String> {
    Ok(YTDLPInstaller::is_installed().await)
}

#[tauri::command]
async fn install_ytdlp() -> Result<(), String> {
    YTDLPInstaller::install().await
}

#[tauri::command]
async fn get_ytdlp_version() -> Result<String, String> {
    YTDLPInstaller::get_version().await
}

// Audio playback commands
#[tauri::command]
async fn play_track(track: YTVideoInfo, state: State<'_, AppState>) -> Result<(), String> {
    // Add to queue first
    state.queue.add_to_queue(track.clone()).await;

    // Set current index to the last added track
    let queue_len = state.queue.get_queue().await.len();
    state.queue.set_current_index((queue_len - 1) as i32).await;

    // Play the track
    state.audio.play(track).await
}

#[tauri::command]
async fn toggle_play_pause(state: State<'_, AppState>) -> Result<(), String> {
    state.audio.toggle_play_pause().await
}

#[tauri::command]
async fn pause_playback(state: State<'_, AppState>) -> Result<(), String> {
    state.audio.pause().await
}

#[tauri::command]
async fn stop_playback(state: State<'_, AppState>) -> Result<(), String> {
    state.audio.stop().await
}

#[tauri::command]
async fn seek_to(position: f64, state: State<'_, AppState>) -> Result<(), String> {
    state.audio.seek(position).await
}

#[tauri::command]
async fn set_volume(volume: f32, state: State<'_, AppState>) -> Result<(), String> {
    state.audio.set_volume(volume).await
}

#[tauri::command]
async fn set_playback_speed(rate: f32, state: State<'_, AppState>) -> Result<(), String> {
    state.audio.set_playback_rate(rate).await
}

#[tauri::command]
async fn play_next(state: State<'_, AppState>) -> Result<Option<YTVideoInfo>, String> {
    if let Some(track) = state.queue.play_next().await {
        state.audio.play(track.clone()).await?;
        Ok(Some(track))
    } else {
        Ok(None)
    }
}

#[tauri::command]
async fn play_previous(state: State<'_, AppState>) -> Result<Option<YTVideoInfo>, String> {
    if let Some(track) = state.queue.play_previous().await {
        state.audio.play(track.clone()).await?;
        Ok(Some(track))
    } else {
        Ok(None)
    }
}

#[tauri::command]
async fn get_audio_state(state: State<'_, AppState>) -> Result<AudioState, String> {
    Ok(state.audio.get_state().await)
}

// Queue commands
#[tauri::command]
async fn add_to_queue(track: YTVideoInfo, state: State<'_, AppState>) -> Result<(), String> {
    state.queue.add_to_queue(track).await;
    Ok(())
}

#[tauri::command]
async fn get_queue(state: State<'_, AppState>) -> Result<Vec<YTVideoInfo>, String> {
    Ok(state.queue.get_queue().await)
}

#[tauri::command]
async fn clear_queue(state: State<'_, AppState>) -> Result<(), String> {
    state.queue.clear_queue().await;
    Ok(())
}

#[tauri::command]
async fn toggle_shuffle(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.queue.toggle_shuffle().await)
}

#[tauri::command]
async fn cycle_repeat_mode(state: State<'_, AppState>) -> Result<RepeatMode, String> {
    Ok(state.queue.cycle_repeat_mode().await)
}

#[tauri::command]
async fn get_queue_info(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.queue.get_queue_info().await)
}

#[tokio::main]
async fn main() {
    // Initialize database
    let db = DatabaseManager::new()
        .await
        .expect("Failed to initialize database");

    // Create app state
    let audio_manager = Arc::new(AudioManager::new());
    let app_state = AppState {
        audio: Arc::clone(&audio_manager),
        queue: Arc::new(QueueManager::new()),
        db: Arc::new(db),
        ytdlp: Arc::new(YTDLPManager::new()),
    };

    tauri::Builder::default()
        .manage(app_state)
        .setup(move |app| {
            // Set app handle in audio manager for events
            let handle = app.handle().clone();
            let audio_clone = Arc::clone(&audio_manager);
            tauri::async_runtime::spawn(async move {
                audio_clone.set_app_handle(handle).await;
            });

            let app = app;
            // Create tray menu
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let show_item = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

            // Create tray icon
            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .menu_on_left_click(false)
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            if window.is_visible().unwrap_or(false) {
                                let _ = window.hide();
                            } else {
                                // Position window in bottom-right before showing
                                #[cfg(target_os = "windows")]
                                {
                                    use tauri::PhysicalPosition;
                                    if let Ok(Some(monitor)) = window.current_monitor() {
                                        let screen_size = monitor.size();
                                        if let Ok(window_size) = window.outer_size() {
                                            let x = screen_size.width as i32 - window_size.width as i32 - 5;
                                            let y = screen_size.height as i32 - window_size.height as i32 - 80;
                                            let _ = window.set_position(PhysicalPosition::new(x, y));
                                        }
                                    }
                                }
                                let _ = window.show().and_then(|_| window.set_focus());
                            }
                        }
                    }
                })
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        app.exit(0);
                    }
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show().and_then(|_| window.set_focus());
                        }
                    }
                    _ => {}
                })
                .build(app)?;

            // Get the main window
            let window = app.get_webview_window("main").unwrap();

            // Position window near system tray (bottom-right on Windows, top-right on Linux)
            #[cfg(target_os = "windows")]
            {
                use tauri::PhysicalPosition;
                if let Some(monitor) = window.current_monitor()? {
                    let screen_size = monitor.size();
                    if let Ok(window_size) = window.outer_size() {
                        // Position in bottom-right corner - close to right edge, more space from taskbar
                        let x = screen_size.width as i32 - window_size.width as i32 - 5;
                        let y = screen_size.height as i32 - window_size.height as i32 - 80;

                        let _ = window.set_position(PhysicalPosition::new(x, y));
                    }
                }
            }

            #[cfg(target_os = "linux")]
            {
                use tauri::PhysicalPosition;
                if let Some(monitor) = window.current_monitor()? {
                    let screen_size = monitor.size();
                    if let Ok(window_size) = window.outer_size() {
                        // Position in top-right corner with some padding
                        let x = screen_size.width as i32 - window_size.width as i32 - 10;
                        let y = 50;

                        let _ = window.set_position(PhysicalPosition::new(x, y));
                    }
                }
            }

            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                // Hide window instead of closing
                let _ = window.hide();
                api.prevent_close();
            }
            WindowEvent::Focused(false) => {
                // Auto-hide when clicking outside (loses focus)
                let _ = window.hide();
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            search_youtube,
            check_ytdlp_installed,
            install_ytdlp,
            get_ytdlp_version,
            play_track,
            toggle_play_pause,
            pause_playback,
            stop_playback,
            seek_to,
            set_volume,
            set_playback_speed,
            play_next,
            play_previous,
            get_audio_state,
            add_to_queue,
            get_queue,
            clear_queue,
            toggle_shuffle,
            cycle_repeat_mode,
            get_queue_info
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
