// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod models;
mod database;
mod ytdlp_manager;
mod ytdlp_installer;
mod ffmpeg_installer;
mod audio_manager;
mod queue_manager;
mod download_manager;
mod media_key_manager;
mod command_utils;

use std::sync::Arc;
use tauri::{
    AppHandle, Manager, State, WindowEvent, tray::{TrayIconBuilder, TrayIconEvent, MouseButton, MouseButtonState},
    menu::{Menu, MenuItem}
};
use tauri_plugin_autostart::ManagerExt;

use crate::database::DatabaseManager;
use crate::models::{AudioState, Playlist, RepeatMode, Track, YTVideoInfo};
use crate::ytdlp_manager::YTDLPManager;
use crate::ytdlp_installer::YTDLPInstaller;
use crate::ffmpeg_installer::FfmpegInstaller;
use crate::audio_manager::AudioManager;
use crate::queue_manager::QueueManager;
use crate::download_manager::DownloadManager;
use crate::media_key_manager::MediaKeyManager;

#[derive(Clone)]
pub struct AppState {
    audio: Arc<AudioManager>,
    queue: Arc<QueueManager>,
    db: Arc<DatabaseManager>,
    ytdlp: Arc<YTDLPManager>,
    downloads: Arc<DownloadManager>,
    media_keys: Arc<MediaKeyManager>,
}

fn show_and_focus_window(window: &tauri::WebviewWindow) {
    #[cfg(not(target_os = "linux"))]
    {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
    // On Linux, show() + set_focus() does NOT raise the window — the WM just
    // adds it to the taskbar/dock without bringing it to the front.
    // The only reliable workaround: hide() first to reset the WM state,
    // then unminimize() → set_focus() → show() so the WM treats it as a
    // fresh window appearance and raises it properly.
    // We save and restore the position because hide() causes the WM to forget it.
    // Finally, set_always_on_top(true/false) forces the window above any
    // currently focused or fullscreen app.
    #[cfg(target_os = "linux")]
    {
        let pos = window.outer_position().ok();
        let _ = window.hide();
        let _ = window.unminimize();
        let _ = window.set_focus();
        let _ = window.show();
        if let Some(pos) = pos {
            let _ = window.set_position(tauri::PhysicalPosition::new(pos.x, pos.y));
        }
        let _ = window.set_always_on_top(true);
        let _ = window.set_focus();
        let _ = window.set_always_on_top(false);
    }
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
async fn cancel_search() -> Result<(), String> {
    YTDLPManager::cancel_search().await;
    Ok(())
}

#[tauri::command]
async fn reinit_audio(state: State<'_, AppState>) -> Result<(), String> {
    state.audio.reinit_audio().await;
    Ok(())
}

#[tauri::command]
async fn set_mini_mode(is_mini: bool, state: State<'_, AppState>) -> Result<(), String> {
    state.db.save_mini_mode(is_mini).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_mini_mode(state: State<'_, AppState>) -> Result<bool, String> {
    state.db.load_mini_mode().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn resize_window(window: tauri::WebviewWindow, height: f64) -> Result<(), String> {
    use tauri::LogicalSize;
    if height < 500.0 {
        // Mini mode: remove min constraint first, then resize, then lock
        let _ = window.set_min_size(Some(LogicalSize::new(380.0f64, height)));
        let _ = window.set_size(LogicalSize::new(380.0f64, height));
        let _ = window.set_resizable(false);
    } else {
        // Max mode: resize, restore constraints, unlock
        let _ = window.set_resizable(true);
        let _ = window.set_size(LogicalSize::new(380.0f64, height));
        let _ = window.set_min_size(Some(LogicalSize::new(380.0f64, 500.0f64)));
    }
    Ok(())
}

#[tauri::command]
async fn reset_window(window: tauri::WebviewWindow, height: f64) -> Result<(), String> {
    use tauri::{PhysicalPosition, LogicalSize};
    if height < 500.0 {
        let _ = window.set_min_size(Some(LogicalSize::new(380.0f64, height)));
        let _ = window.set_size(LogicalSize::new(380.0f64, height));
        let _ = window.set_resizable(false);
    } else {
        let _ = window.set_resizable(true);
        let _ = window.set_size(LogicalSize::new(380.0f64, height));
        let _ = window.set_min_size(Some(LogicalSize::new(380.0f64, 500.0f64)));
    }
    if let Ok(Some(monitor)) = window.current_monitor() {
        let screen = monitor.size();
        let scale = monitor.scale_factor();
        let win_w = (380.0 * scale) as i32;
        let win_h = (height * scale) as i32;
        #[cfg(target_os = "windows")]
        {
            let x = screen.width as i32 - win_w - 5;
            let y = screen.height as i32 - win_h - 80;
            let _ = window.set_position(PhysicalPosition::new(x, y));
        }
        #[cfg(target_os = "linux")]
        {
            let x = screen.width as i32 - win_w - 30;
            let _ = window.set_position(PhysicalPosition::new(x, 40i32));
        }
    }
    Ok(())
}

#[tauri::command]
async fn get_video_details(
    video_id: String,
    state: State<'_, AppState>,
) -> Result<YTVideoInfo, String> {
    state.ytdlp.get_video_details(video_id).await
}

#[tauri::command]
async fn get_video_info_fast(
    video_id: String,
    state: State<'_, AppState>,
) -> Result<YTVideoInfo, String> {
    state.ytdlp.get_video_info_fast(video_id).await
}

#[tauri::command]
async fn check_ytdlp_installed() -> Result<bool, String> {
    Ok(YTDLPInstaller::is_installed().await)
}

#[tauri::command]
async fn install_ytdlp(app_handle: AppHandle) -> Result<(), String> {
    YTDLPInstaller::install(&app_handle).await
}

#[tauri::command]
async fn get_ytdlp_version() -> Result<String, String> {
    YTDLPInstaller::get_version().await
}

#[tauri::command]
async fn check_ytdlp_update(app_handle: AppHandle) -> Result<Option<String>, String> {
    YTDLPInstaller::check_and_update(&app_handle).await
}

// Ffmpeg commands
#[tauri::command]
async fn check_ffmpeg_available() -> Result<bool, String> {
    Ok(FfmpegInstaller::is_available().await)
}

#[tauri::command]
async fn install_ffmpeg(app_handle: AppHandle) -> Result<(), String> {
    FfmpegInstaller::ensure_available(&app_handle).await
}

#[tauri::command]
async fn play_track(mut track: YTVideoInfo, state: State<'_, AppState>) -> Result<(), String> {
    let _ = state.audio.stop().await;

    state.audio.set_loading_state(&track).await;

    if let Some(file_path) = state.downloads.get_downloaded_file_path(&track.id).await {
        println!("🎵 Playing from local file: {}", file_path);
        return state.audio.play_from_file(track, file_path).await;
    }

    if track.duration == 0 {
        println!("⏱️ Fetching duration for {} before playing...", track.id);
        match get_video_details(track.id.clone(), state.clone()).await {
            Ok(details) => {
                track.duration = details.duration;
                track.description = details.description;
                println!("✅ Got duration: {}s", track.duration);
                state.audio.update_track_duration(track.duration as f64).await;
            }
            Err(e) => {
                eprintln!("⚠️ Failed to fetch details, playing anyway: {}", e);
            }
        }
    }

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
        if let Some(file_path) = state.downloads.get_downloaded_file_path(&track.id).await {
            println!("🎵 Playing next from local file: {}", file_path);
            state.audio.play_from_file(track.clone(), file_path).await?;
        } else {
            state.audio.play(track.clone()).await?;
        }
        Ok(Some(track))
    } else {
        Ok(None)
    }
}

#[tauri::command]
async fn play_previous(state: State<'_, AppState>) -> Result<Option<YTVideoInfo>, String> {
    if let Some(track) = state.queue.play_previous().await {
        if let Some(file_path) = state.downloads.get_downloaded_file_path(&track.id).await {
            println!("🎵 Playing previous from local file: {}", file_path);
            state.audio.play_from_file(track.clone(), file_path).await?;
        } else {
            state.audio.play(track.clone()).await?;
        }
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
async fn add_to_queue_next(track: YTVideoInfo, state: State<'_, AppState>) -> Result<(), String> {
    state.queue.insert_next(track).await;
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
async fn get_shuffle_mode(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.queue.get_shuffle_mode().await)
}

#[tauri::command]
async fn cycle_repeat_mode(state: State<'_, AppState>) -> Result<RepeatMode, String> {
    Ok(state.queue.cycle_repeat_mode().await)
}

#[tauri::command]
async fn get_repeat_mode(state: State<'_, AppState>) -> Result<RepeatMode, String> {
    Ok(state.queue.get_repeat_mode().await)
}

#[tauri::command]
async fn get_queue_info(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.queue.get_queue_info().await)
}

#[tauri::command]
async fn reorder_queue(new_queue: Vec<YTVideoInfo>, state: State<'_, AppState>) -> Result<(), String> {
    state.queue.reorder_queue(new_queue).await
}

#[tauri::command]
async fn remove_from_queue(index: usize, state: State<'_, AppState>) -> Result<(), String> {
    state.queue.remove_from_queue(index).await
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
async fn update_playlist_name(id: String, name: String, state: State<'_, AppState>) -> Result<(), String> {
    state.db
        .update_playlist_name(&id, &name)
        .await
        .map_err(|e| e.to_string())
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

    state.queue.clear_queue().await;
    state.queue.add_to_queue_batch(video_tracks.clone()).await;

    state.queue.set_current_index(0).await;

    if let Some(first_track) = video_tracks.first() {
        if let Some(file_path) = state.downloads.get_downloaded_file_path(&first_track.id).await {
            println!("🎵 Playing playlist first track from local file: {}", file_path);
            state.audio.play_from_file(first_track.clone(), file_path).await?;
        } else {
            state.audio.play(first_track.clone()).await?;
        }
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
    let path_buf = std::path::PathBuf::from(&path);
    state.downloads.set_downloads_dir(path_buf).await?;
    if let Ok(mut settings) = state.db.load_settings().await.map_err(|e| e.to_string()) {
        settings.default_download_path = path;
        let _ = state.db.save_settings(&settings).await;
    }
    Ok(())
}

#[tauri::command]
async fn get_audio_quality(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.downloads.get_audio_quality().await)
}

#[tauri::command]
async fn set_audio_quality(quality: String, state: State<'_, AppState>) -> Result<(), String> {
    state.downloads.set_audio_quality(quality.clone()).await?;
    if let Ok(mut settings) = state.db.load_settings().await.map_err(|e| e.to_string()) {
        settings.preferred_audio_quality = quality;
        let _ = state.db.save_settings(&settings).await;
    }
    Ok(())
}

#[tauri::command]
async fn get_app_version() -> Result<String, String> {
    Ok(env!("CARGO_PKG_VERSION").to_string())
}

#[tauri::command]
async fn get_autostart_enabled(app: AppHandle) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_autostart_enabled(enabled: bool, app: AppHandle) -> Result<(), String> {
    let manager = app.autolaunch();
    if enabled {
        manager.enable().map_err(|e| e.to_string())
    } else {
        manager.disable().map_err(|e| e.to_string())
    }
}

#[tauri::command]
async fn check_for_updates_manual(app: tauri::AppHandle) -> Result<bool, String> {
    check_for_updates_silently(app).await;
    Ok(true)
}

// ===== MEDIA KEY COMMANDS =====

#[tauri::command]
async fn update_media_metadata(
    title: String,
    artist: String,
    duration: f64,
    cover_url: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.media_keys.update_metadata(title, artist, duration, cover_url).await;
    Ok(())
}

#[tauri::command]
async fn update_media_playback_state(
    is_playing: bool,
    position: f64,
    duration: f64,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.media_keys.update_playback_state(is_playing, position, duration).await;
    Ok(())
}

#[tauri::command]
async fn clear_media_info(state: State<'_, AppState>) -> Result<(), String> {
    state.media_keys.clear().await;
    Ok(())
}

// Silent auto-update function (like macOS Sparkle)
async fn check_for_updates_silently(app: tauri::AppHandle) {
    use tauri_plugin_updater::UpdaterExt;

    // On Linux, auto-update only works with AppImage, not .deb
    #[cfg(target_os = "linux")]
    {
        // Check for APPIMAGE environment variable (set by AppImage runtime)
        // This variable contains the path to the .AppImage file and is only set when running from AppImage
        if std::env::var("APPIMAGE").is_err() {
            println!("ℹ️ Skipping auto-update: .deb installations cannot be updated automatically.");
            println!("   To get auto-updates, use the AppImage version instead.");
            return;
        }
    }

    println!("🔍 Starting update check...");
    println!("📍 Update endpoint: https://github.com/ilyassan/ytaudiobar/releases/latest/download/latest.json");

    match app.updater() {
        Ok(updater) => {
            println!("✅ Updater initialized successfully");
            match updater.check().await {
                Ok(Some(update)) => {
                    println!("🔄 Update available!");
                    println!("   Current version: {}", update.current_version);
                    println!("   New version: {}", update.version);
                    println!("   Download URL: {}", update.download_url);

                    // Download silently in background
                    println!("📥 Downloading update in background...");
                    match update.download(
                        |chunk_len, content_len| {
                            if let Some(total) = content_len {
                                let progress = (chunk_len as f64 / total as f64) * 100.0;
                                if progress as u32 % 10 == 0 {
                                    println!("   Download progress: {:.0}%", progress);
                                }
                            }
                        },
                        || {
                            println!("📦 Download complete!");
                        }
                    ).await {
                        Ok(bytes) => {
                            println!("✅ Update downloaded successfully!");

                            // Finalize the update (instant, non-blocking)
                            match update.install(bytes) {
                                Ok(_) => {
                                    println!("✅ Update ready! Will be applied on next app launch");
                                }
                                Err(e) => {
                                    eprintln!("⚠️ Failed to finalize update: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("❌ Failed to download update: {}", e);
                        }
                    }
                }
                Ok(None) => {
                    println!("✅ App is already up to date");
                }
                Err(e) => {
                    eprintln!("⚠️ Failed to check for updates: {}", e);
                    eprintln!("   This could be due to:");
                    eprintln!("   - Network connectivity issues");
                    eprintln!("   - latest.json not found on server");
                    eprintln!("   - Invalid JSON format");
                }
            }
        }
        Err(e) => {
            eprintln!("⚠️ Updater not available: {}", e);
        }
    }
}

#[cfg(target_os = "linux")]
fn integrate_appimage_to_system() {
    // Only integrate if running from AppImage
    if let Ok(appimage_path) = std::env::var("APPIMAGE") {
        let home = match std::env::var("HOME") {
            Ok(h) => h,
            Err(_) => return,
        };

        let desktop_file = format!("{}/.local/share/applications/ytaudiobar.desktop", home);

        // Check if already integrated
        if std::path::Path::new(&desktop_file).exists() {
            return;
        }

        println!("📦 Integrating YTAudioBar to system app menu...");

        // Create .local/share/applications directory if it doesn't exist
        let apps_dir = format!("{}/.local/share/applications", home);
        if let Err(e) = std::fs::create_dir_all(&apps_dir) {
            eprintln!("⚠️ Failed to create applications directory: {}", e);
            return;
        }

        // Install icon - extract from AppImage and copy to user icons
        // Extract YTAudioBar.png from AppImage (not .DirIcon which is a broken symlink)
        let mut icon_installed = false;
        let icon_dir = format!("{}/.local/share/icons/hicolor/128x128/apps", home);
        let icon_dest = format!("{}/ytaudiobar.png", icon_dir);

        let extract_result = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("{} --appimage-extract YTAudioBar.png 2>/dev/null", appimage_path))
            .current_dir("/tmp")
            .output();

        if let Ok(output) = extract_result {
            if output.status.success() {
                let extracted_icon = "/tmp/squashfs-root/YTAudioBar.png";
                if std::path::Path::new(extracted_icon).exists() {
                    if std::fs::create_dir_all(&icon_dir).is_ok() {
                        if std::fs::copy(extracted_icon, &icon_dest).is_ok() {
                            println!("✅ Icon extracted and installed successfully to {}", icon_dest);
                            icon_installed = true;
                        }
                    }
                    // Clean up extracted files
                    let _ = std::fs::remove_dir_all("/tmp/squashfs-root");
                }
            }
        }

        if !icon_installed {
            eprintln!("⚠️ Could not extract icon from AppImage, using AppImage path as fallback");
        }

        // Determine icon value - use name if we installed it, otherwise use AppImage path
        let icon_value = if icon_installed {
            "ytaudiobar".to_string()
        } else {
            println!("⚠️ Could not extract icon from AppImage, using AppImage path as fallback");
            appimage_path.clone()
        };

        // Create desktop entry
        let desktop_content = format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Name=YTAudioBar\n\
             Comment=YouTube Audio Player\n\
             Exec={}\n\
             Icon={}\n\
             Categories=AudioVideo;Audio;Player;\n\
             Terminal=false\n\
             StartupWMClass=YTAudioBar\n\
             X-AppImage-Version={}\n",
            appimage_path,
            icon_value,
            env!("CARGO_PKG_VERSION")
        );

        if let Err(e) = std::fs::write(&desktop_file, desktop_content) {
            eprintln!("⚠️ Failed to create desktop entry: {}", e);
            return;
        }

        // Update icon cache
        if icon_installed {
            let _ = std::process::Command::new("gtk-update-icon-cache")
                .arg(format!("{}/.local/share/icons/hicolor", home))
                .arg("-f")
                .arg("-t")
                .output();
        }

        // Update desktop database
        let _ = std::process::Command::new("update-desktop-database")
            .arg(apps_dir)
            .output();

        println!("✅ YTAudioBar integrated! You can now find it in your app menu.");
    }
}

#[tokio::main]
async fn main() {
    // Force X11 backend on Linux - Wayland doesn't support:
    // - Window transparency (needed for rounded corners)
    // - Programmatic window positioning
    // - data-tauri-drag-region (custom titlebar dragging)
    // XWayland provides full compatibility for all these features.
    #[cfg(target_os = "linux")]
    {
        std::env::set_var("GDK_BACKEND", "x11");

        // Integrate AppImage to system on first run
        integrate_appimage_to_system();
    }

    // Initialize database
    let db = DatabaseManager::new()
        .await
        .expect("Failed to initialize database");

    // Create app state
    let audio_manager = Arc::new(AudioManager::new());
    let download_manager = Arc::new(DownloadManager::new());

    // Apply persisted settings (downloads dir, audio quality)
    if let Ok(settings) = db.load_settings().await {
        if !settings.default_download_path.is_empty() {
            let path = std::path::PathBuf::from(&settings.default_download_path);
            if path.exists() {
                download_manager.set_downloads_dir_silent(path).await;
            }
        }
        if !settings.preferred_audio_quality.is_empty() {
            let _ = download_manager.set_audio_quality(settings.preferred_audio_quality).await;
        }
    }
    let media_key_manager = Arc::new(MediaKeyManager::new());
    let app_state = AppState {
        audio: Arc::clone(&audio_manager),
        queue: Arc::new(QueueManager::new()),
        db: Arc::new(db),
        ytdlp: Arc::new(YTDLPManager::new()),
        downloads: Arc::clone(&download_manager),
        media_keys: Arc::clone(&media_key_manager),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // When a second instance tries to open, focus the existing main window
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec![])))
        .manage(app_state)
        .setup(move |app| {
            // Window positioning is handled later in setup with manual calculations
            // for better compatibility across different environments

            // Set app handle in audio manager for events
            let handle = app.handle().clone();
            let audio_clone = Arc::clone(&audio_manager);
            tauri::async_runtime::spawn(async move {
                audio_clone.set_app_handle(handle).await;
            });

            // Check for yt-dlp updates in background (max once per 24h)
            let update_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = YTDLPInstaller::check_and_update(&update_handle).await {
                    eprintln!("⚠️ Failed to check for yt-dlp updates: {}", e);
                }
            });

            // Set app handle in download manager and initialize existing downloads
            let handle = app.handle().clone();
            let download_clone = Arc::clone(&download_manager);
            tauri::async_runtime::spawn(async move {
                download_clone.set_app_handle(handle).await;
                download_clone.initialize().await;
            });

            // Initialize media key manager
            let handle = app.handle().clone();
            let media_key_clone = Arc::clone(&media_key_manager);
            tauri::async_runtime::spawn(async move {
                if let Err(e) = media_key_clone.initialize(handle).await {
                    eprintln!("Failed to initialize media keys: {}", e);
                }
            });

            // Check for updates silently in background (disabled in dev mode)
            #[cfg(not(debug_assertions))]
            {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    use std::time::Duration;
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    println!("🔍 Checking for updates in background...");
                    check_for_updates_silently(handle).await;
                });
            }

            // Enable autostart on first run
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let manager = handle.autolaunch();
                match manager.is_enabled() {
                    Ok(is_enabled) => {
                        if !is_enabled {
                            println!("🚀 Enabling autostart on system boot...");
                            if let Err(e) = manager.enable() {
                                eprintln!("⚠️ Failed to enable autostart: {}", e);
                            } else {
                                println!("✅ Autostart enabled successfully");
                            }
                        } else {
                            println!("✅ Autostart already enabled");
                        }
                    }
                    Err(e) => {
                        eprintln!("⚠️ Failed to check autostart status: {}", e);
                    }
                }
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
                            if let Some(file_path) = state.downloads.get_downloaded_file_path(&track.id).await {
                                println!("🎵 Auto-playing from local file: {}", file_path);
                                let _ = state.audio.play_from_file(track, file_path).await;
                            } else {
                                let _ = state.audio.play(track).await;
                            }
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

            // Create tray icon with Linux fallback
            let tray_icon = if cfg!(target_os = "linux") {
                // On Linux, use the PNG icon explicitly
                match app.default_window_icon() {
                    Some(icon) => icon.clone(),
                    None => {
                        // Fallback: load icon from file
                        let icon_path = app.path().resolve("icons/128x128.png", tauri::path::BaseDirectory::Resource)
                            .expect("Failed to resolve icon path");
                        tauri::image::Image::from_path(icon_path)
                            .expect("Failed to load tray icon")
                    }
                }
            } else {
                app.default_window_icon().unwrap().clone()
            };

            let _tray = TrayIconBuilder::new()
                .icon(tray_icon)
                .menu(&menu)
                // On Linux (AppIndicator protocol), show_menu_on_left_click is ignored
                // and the menu always appears on any click. We still set it false so that
                // on DEs using StatusNotifierItem (KDE, etc.) left-click shows the window.
                .show_menu_on_left_click(false)
                .tooltip("YTAudioBar")
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let is_minimized = window.is_minimized().unwrap_or(false);
                            let is_visible = window.is_visible().unwrap_or(false);
                            if is_visible && !is_minimized {
                                #[cfg(not(target_os = "linux"))]
                                let _ = window.minimize();
                                // On Linux don't minimize — left click should always show
                                #[cfg(target_os = "linux")]
                                show_and_focus_window(&window);
                            } else {
                                show_and_focus_window(&window);
                            }
                        }
                    }
                })
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        // Save window geometry before quitting
                        if let Some(window) = app.get_webview_window("main") {
                            if let (Ok(pos), Ok(size)) = (window.outer_position(), window.outer_size()) {
                                println!("📐 [QUIT] Saving geometry before exit: pos=({}, {}), size={}x{}", pos.x, pos.y, size.width, size.height);
                                let db = app.state::<AppState>().db.clone();
                                match tauri::async_runtime::block_on(
                                    db.save_window_geometry(pos.x, pos.y, size.width, size.height)
                                ) {
                                    Ok(_) => println!("📐 [QUIT] Geometry saved successfully"),
                                    Err(e) => println!("📐 [QUIT] ERROR saving geometry: {}", e),
                                }
                            } else {
                                println!("📐 [QUIT] ERROR: could not get window position/size");
                            }
                        }
                        app.exit(0);
                    }
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            show_and_focus_window(&window);
                        }
                    }
                    _ => {}
                })
                .build(app)?;

            // Get the main window
            let window = app.get_webview_window("main").unwrap();

            // Show window first so the WM maps it (Linux ignores set_position on hidden windows)
            show_and_focus_window(&window);

            // Try to restore last saved geometry; fall back to default if none or off-screen
            {
                use tauri::{PhysicalPosition, PhysicalSize};

                let db = app.state::<AppState>().db.clone();
                let saved = tokio::task::block_in_place(|| {
                    tauri::async_runtime::block_on(db.load_window_geometry())
                });
                let mut restored = false;

                println!("📐 [STARTUP] Loading saved geometry from DB: {:?}", saved);

                if let Ok(Some((x, y, width, height))) = saved {
                    println!("📐 [STARTUP] Found saved geometry: pos=({}, {}), size={}x{}", x, y, width, height);
                    // Check the saved position is on at least one available monitor
                    let monitors = window.available_monitors().unwrap_or_default();
                    println!("📐 [STARTUP] Available monitors: {}", monitors.len());
                    for (i, m) in monitors.iter().enumerate() {
                        let mp = m.position();
                        let ms = m.size();
                        println!("📐 [STARTUP]   Monitor {}: pos=({}, {}), size={}x{}", i, mp.x, mp.y, ms.width, ms.height);
                    }
                    let on_screen = monitors.iter().any(|m| {
                        let mp = m.position();
                        let ms = m.size();
                        x >= mp.x && y >= mp.y
                            && x < mp.x + ms.width as i32
                            && y < mp.y + ms.height as i32
                    });
                    println!("📐 [STARTUP] on_screen check: {}", on_screen);

                    if on_screen {
                        println!("📐 [STARTUP] Restoring geometry: set_size({}x{}), set_position({}, {})", width, height, x, y);
                        let _ = window.set_size(PhysicalSize::new(width, height));
                        let _ = window.set_position(PhysicalPosition::new(x, y));
                        restored = true;
                    } else {
                        println!("📐 [STARTUP] Saved position is OFF-SCREEN, using default");
                    }
                } else {
                    println!("📐 [STARTUP] No saved geometry found (first launch or DB error)");
                }

                if !restored {
                    println!("📐 [STARTUP] Using DEFAULT positioning");
                    // First launch or off-screen: use default 500px max mode positioning
                    if let Some(monitor) = window.current_monitor()? {
                        let screen_size = monitor.size();
                        if let Ok(window_size) = window.outer_size() {
                            println!("📐 [STARTUP] Screen: {}x{}, Window: {}x{}", screen_size.width, screen_size.height, window_size.width, window_size.height);
                            #[cfg(target_os = "windows")]
                            {
                                let x = screen_size.width as i32 - window_size.width as i32 - 5;
                                let y = screen_size.height as i32 - window_size.height as i32 - 80;
                                println!("📐 [STARTUP] Default position: ({}, {})", x, y);
                                let _ = window.set_position(PhysicalPosition::new(x, y));
                            }
                            #[cfg(target_os = "linux")]
                            {
                                let x = screen_size.width as i32 - window_size.width as i32 - 30;
                                let y = 40;
                                println!("📐 [STARTUP] Default position: ({}, {})", x, y);
                                let _ = window.set_position(PhysicalPosition::new(x, y));
                            }
                        }
                    }
                } else {
                    // Geometry was restored — now apply mini mode if needed
                    let db2 = app.state::<AppState>().db.clone();
                    let is_mini = tokio::task::block_in_place(|| {
                        tauri::async_runtime::block_on(db2.load_mini_mode())
                    }).unwrap_or(false);
                    println!("📐 [STARTUP] Mini mode from DB: {}", is_mini);
                    if is_mini {
                        use tauri::LogicalSize;
                        println!("📐 [STARTUP] Applying mini mode: 380x100, resizable=false");
                        let _ = window.set_min_size(Some(LogicalSize::new(380.0f64, 100.0f64)));
                        let _ = window.set_size(LogicalSize::new(380.0f64, 100.0f64));
                        let _ = window.set_resizable(false);
                    }
                }
            }

            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                // Save window geometry before hiding
                if let (Ok(pos), Ok(size)) = (window.outer_position(), window.outer_size()) {
                    println!("📐 [CLOSE] Saving geometry: pos=({}, {}), size={}x{}", pos.x, pos.y, size.width, size.height);
                    let db = window.app_handle().state::<AppState>().db.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = db.save_window_geometry(pos.x, pos.y, size.width, size.height).await;
                    });
                }
                let _ = window.hide();
                api.prevent_close();
            }
            WindowEvent::Moved(pos) => {
                if let Ok(size) = window.outer_size() {
                    println!("📐 [MOVED] pos=({}, {}), size={}x{}", pos.x, pos.y, size.width, size.height);
                    let db = window.app_handle().state::<AppState>().db.clone();
                    let px = pos.x;
                    let py = pos.y;
                    let sw = size.width;
                    let sh = size.height;
                    tauri::async_runtime::spawn(async move {
                        let _ = db.save_window_geometry(px, py, sw, sh).await;
                    });
                }
            }
            WindowEvent::Resized(size) => {
                if let Ok(pos) = window.outer_position() {
                    println!("📐 [RESIZED] pos=({}, {}), size={}x{}", pos.x, pos.y, size.width, size.height);
                    let db = window.app_handle().state::<AppState>().db.clone();
                    let px = pos.x;
                    let py = pos.y;
                    let sw = size.width;
                    let sh = size.height;
                    tauri::async_runtime::spawn(async move {
                        let _ = db.save_window_geometry(px, py, sw, sh).await;
                    });
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            search_youtube,
            cancel_search,
            get_video_details,
            check_ytdlp_installed,
            install_ytdlp,
            get_ytdlp_version,
            check_ytdlp_update,
            check_ffmpeg_available,
            install_ffmpeg,
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
            add_to_queue_next,
            get_queue,
            clear_queue,
            toggle_shuffle,
            get_shuffle_mode,
            cycle_repeat_mode,
            get_repeat_mode,
            get_queue_info,
            reorder_queue,
            remove_from_queue,
            // Playlist commands
            get_all_playlists,
            create_playlist,
            delete_playlist,
            update_playlist_name,
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
            get_app_version,
            // Media key commands
            update_media_metadata,
            update_media_playback_state,
            clear_media_info,
            // Window commands
            set_mini_mode,
            get_mini_mode,
            resize_window,
            reset_window,
            reinit_audio,
            // Updater commands
            check_for_updates_manual,
            // Autostart commands
            get_autostart_enabled,
            set_autostart_enabled,
            // Fast video info
            get_video_info_fast
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
