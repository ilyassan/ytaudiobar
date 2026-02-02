// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod models;
mod database;
mod ytdlp_manager;
mod ytdlp_installer;
mod audio_manager;
mod queue_manager;
mod download_manager;

use std::sync::Arc;
use tauri::{
    Manager, State, WindowEvent, tray::{TrayIconBuilder, TrayIconEvent, MouseButton, MouseButtonState},
    menu::{Menu, MenuItem}
};

use crate::database::DatabaseManager;
use crate::models::{AudioState, Playlist, RepeatMode, Track, YTVideoInfo};
use crate::ytdlp_manager::YTDLPManager;
use crate::ytdlp_installer::YTDLPInstaller;
use crate::audio_manager::AudioManager;
use crate::queue_manager::QueueManager;
use crate::download_manager::DownloadManager;

#[derive(Clone)]
pub struct AppState {
    audio: Arc<AudioManager>,
    queue: Arc<QueueManager>,
    db: Arc<DatabaseManager>,
    ytdlp: Arc<YTDLPManager>,
    downloads: Arc<DownloadManager>,
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
    // Check if track is downloaded and use local file if available
    if let Some(file_path) = state.downloads.get_downloaded_file_path(&track.id).await {
        println!("🎵 Playing from local file: {}", file_path);
        state.audio.play_from_file(track, file_path).await
    } else {
        // Play track directly WITHOUT adding to queue
        // Queue is only populated via "Play All" playlist action
        state.audio.play(track).await
    }
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

#[tauri::command]
async fn reorder_queue(new_queue: Vec<YTVideoInfo>, state: State<'_, AppState>) -> Result<(), String> {
    state.queue.reorder_queue(new_queue).await
}

// ===== PLAYLIST COMMANDS =====

#[tauri::command]
async fn get_all_playlists(state: State<'_, AppState>) -> Result<Vec<Playlist>, String> {
    state
        .db
        .get_all_playlists()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn create_playlist(name: String, state: State<'_, AppState>) -> Result<String, String> {
    state.db.create_playlist(&name).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_playlist(id: String, state: State<'_, AppState>) -> Result<(), String> {
    state.db.delete_playlist(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_playlist_tracks(playlist_id: String, state: State<'_, AppState>) -> Result<Vec<Track>, String> {
    state
        .db
        .get_playlist_tracks(&playlist_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn add_track_to_playlist(
    track: YTVideoInfo,
    playlist_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // First save the track to database
    let db_track = Track {
        id: track.id.clone(),
        title: track.title,
        author: Some(track.uploader),
        duration: track.duration,
        thumbnail_url: track.thumbnail_url,
        added_date: chrono::Utc::now().timestamp(),
        file_path: None,
    };

    state.db.save_track(&db_track).await.map_err(|e| e.to_string())?;

    // Then add to playlist
    state
        .db
        .add_track_to_playlist(&track.id, &playlist_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn remove_track_from_playlist(
    track_id: String,
    playlist_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .db
        .remove_track_from_playlist(&track_id, &playlist_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn add_to_favorites(track: YTVideoInfo, state: State<'_, AppState>) -> Result<(), String> {
    // Save track first
    let db_track = Track {
        id: track.id.clone(),
        title: track.title,
        author: Some(track.uploader),
        duration: track.duration,
        thumbnail_url: track.thumbnail_url,
        added_date: chrono::Utc::now().timestamp(),
        file_path: None,
    };

    state.db.save_track(&db_track).await.map_err(|e| e.to_string())?;

    // Add to favorites
    state
        .db
        .add_to_favorites(&track.id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn remove_from_favorites(track_id: String, state: State<'_, AppState>) -> Result<(), String> {
    state
        .db
        .remove_from_favorites(&track_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn play_playlist(playlist_id: String, state: State<'_, AppState>) -> Result<(), String> {
    // Get all tracks from playlist
    let tracks = state
        .db
        .get_playlist_tracks(&playlist_id)
        .await
        .map_err(|e| e.to_string())?;

    if tracks.is_empty() {
        return Err("Playlist is empty".to_string());
    }

    // Convert to YTVideoInfo
    let video_tracks: Vec<YTVideoInfo> = tracks
        .into_iter()
        .map(|t| YTVideoInfo {
            id: t.id,
            title: t.title,
            uploader: t.author.unwrap_or_else(|| "Unknown".to_string()),
            duration: t.duration,
            thumbnail_url: t.thumbnail_url,
            audio_url: None,
            description: None,
        })
        .collect();

    // Clear queue and add all playlist tracks
    state.queue.clear_queue().await;
    state.queue.add_to_queue_batch(video_tracks.clone()).await;

    // Set current index to first track
    state.queue.set_current_index(0).await;

    // Play first track
    if let Some(first_track) = video_tracks.first() {
        state.audio.play(first_track.clone()).await?;
    }

    Ok(())
}

// ===== DOWNLOAD COMMANDS =====

#[tauri::command]
async fn download_track(track: YTVideoInfo, state: State<'_, AppState>) -> Result<(), String> {
    state.downloads.download_track(track).await
}

#[tauri::command]
async fn get_active_downloads(state: State<'_, AppState>) -> Result<Vec<crate::download_manager::DownloadProgress>, String> {
    Ok(state.downloads.get_active_downloads().await)
}

#[tauri::command]
async fn get_downloaded_tracks(state: State<'_, AppState>) -> Result<Vec<crate::download_manager::DownloadedTrack>, String> {
    Ok(state.downloads.get_downloaded_tracks().await)
}

#[tauri::command]
async fn get_storage_used(state: State<'_, AppState>) -> Result<i64, String> {
    Ok(state.downloads.get_storage_used().await)
}

#[tauri::command]
async fn is_track_downloaded(video_id: String, state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.downloads.is_downloaded(&video_id).await)
}

#[tauri::command]
async fn delete_download(video_id: String, state: State<'_, AppState>) -> Result<(), String> {
    state.downloads.delete_download(&video_id).await
}

#[tauri::command]
async fn cancel_download(video_id: String, state: State<'_, AppState>) -> Result<(), String> {
    state.downloads.cancel_download(&video_id).await
}

// ===== SETTINGS COMMANDS =====

#[tauri::command]
async fn get_downloads_directory(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.downloads.get_downloads_directory().await)
}

#[tauri::command]
async fn set_downloads_directory(path: String, state: State<'_, AppState>) -> Result<(), String> {
    use std::path::PathBuf;
    let path_buf = PathBuf::from(path);
    state.downloads.set_downloads_dir(path_buf).await
}

#[tauri::command]
async fn get_audio_quality(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.downloads.get_audio_quality().await)
}

#[tauri::command]
async fn set_audio_quality(quality: String, state: State<'_, AppState>) -> Result<(), String> {
    state.downloads.set_audio_quality(quality).await
}

#[tauri::command]
async fn get_app_version() -> Result<String, String> {
    Ok(env!("CARGO_PKG_VERSION").to_string())
}

#[tokio::main]
async fn main() {
    // Initialize database
    let db = DatabaseManager::new()
        .await
        .expect("Failed to initialize database");

    // Create app state
    let audio_manager = Arc::new(AudioManager::new());
    let download_manager = Arc::new(DownloadManager::new());
    let app_state = AppState {
        audio: Arc::clone(&audio_manager),
        queue: Arc::new(QueueManager::new()),
        db: Arc::new(db),
        ytdlp: Arc::new(YTDLPManager::new()),
        downloads: Arc::clone(&download_manager),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(app_state)
        .setup(move |app| {
            // Set app handle in audio manager for events
            let handle = app.handle().clone();
            let audio_clone = Arc::clone(&audio_manager);
            tauri::async_runtime::spawn(async move {
                audio_clone.set_app_handle(handle).await;
            });

            // Set app handle in download manager for events
            let handle = app.handle().clone();
            let download_clone = Arc::clone(&download_manager);
            tauri::async_runtime::spawn(async move {
                download_clone.set_app_handle(handle).await;
            });

            // Listen for track-ended events and auto-play next track
            let handle_clone = app.handle().clone();
            let state_clone = app.state::<AppState>().inner().clone();
            tauri::async_runtime::spawn(async move {
                use tauri::Listener;
                handle_clone.listen("track-ended", move |_event| {
                    let state = state_clone.clone();
                    tauri::async_runtime::spawn(async move {
                        println!("🎵 Track ended, attempting to play next...");
                        if let Some(track) = state.queue.play_next().await {
                            println!("▶️ Auto-playing next track: {}", track.title);
                            let _ = state.audio.play(track).await;
                        } else {
                            println!("⏹️ No more tracks in queue");
                        }
                    });
                });
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
                .show_menu_on_left_click(false)
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
            get_queue_info,
            reorder_queue,
            // Playlist commands
            get_all_playlists,
            create_playlist,
            delete_playlist,
            get_playlist_tracks,
            add_track_to_playlist,
            remove_track_from_playlist,
            add_to_favorites,
            remove_from_favorites,
            play_playlist,
            // Download commands
            download_track,
            get_active_downloads,
            get_downloaded_tracks,
            get_storage_used,
            is_track_downloaded,
            delete_download,
            cancel_download,
            // Settings commands
            get_downloads_directory,
            set_downloads_directory,
            get_audio_quality,
            set_audio_quality,
            get_app_version
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
