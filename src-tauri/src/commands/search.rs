use crate::analytics::Analytics;
use crate::ffmpeg_installer::FfmpegInstaller;
use crate::models::{YTPlaylistInfo, YTPlaylistPreview, YTVideoInfo};
use crate::ytdlp_installer::YTDLPInstaller;
use crate::ytdlp_manager::{YTDLPManager, SEARCH_CANCELLED};
use crate::AppState;
use serde_json::json;
use std::time::Instant;
use tauri::{AppHandle, State};

// Times how long the search actually took to return results, so `search_performed`
// tells us more than a bare count -- a `cancelled` search (the user typed again
// before results came back) is reported as such rather than as a fast/slow search,
// since its duration reflects when it was cut off, not how long a real search takes.
fn track_search_performed<T>(analytics: &Analytics, result: &Result<T, String>, started_at: Instant) {
    let data = match result {
        Err(e) if e == SEARCH_CANCELLED => json!({ "result": "cancelled" }),
        _ => json!({ "duration_seconds": started_at.elapsed().as_secs_f64() }),
    };
    analytics.track_with_data("search_performed", data);
}

#[tauri::command]
pub async fn search_youtube(
    query: String,
    state: State<'_, AppState>,
) -> Result<Vec<YTVideoInfo>, String> {
    let started_at = Instant::now();
    let result = state.ytdlp.search(query).await;
    track_search_performed(&state.analytics, &result, started_at);
    result
}

#[tauri::command]
pub async fn search_playlists(
    query: String,
    state: State<'_, AppState>,
) -> Result<Vec<YTPlaylistInfo>, String> {
    let started_at = Instant::now();
    let result = state.ytdlp.search_playlists(query).await;
    track_search_performed(&state.analytics, &result, started_at);
    result
}

#[tauri::command]
pub async fn get_playlist_preview(
    playlist_url: String,
    state: State<'_, AppState>,
) -> Result<YTPlaylistPreview, String> {
    state.ytdlp.get_playlist_preview(playlist_url).await
}

#[tauri::command]
pub async fn cancel_search() -> Result<(), String> {
    YTDLPManager::cancel_search().await;
    Ok(())
}

#[tauri::command]
pub async fn get_video_details(
    video_id: String,
    state: State<'_, AppState>,
) -> Result<YTVideoInfo, String> {
    state.ytdlp.get_video_details(video_id).await
}

#[tauri::command]
pub async fn get_video_info_fast(
    video_id: String,
    state: State<'_, AppState>,
) -> Result<YTVideoInfo, String> {
    state.ytdlp.get_video_info_fast(video_id).await
}

#[tauri::command]
pub async fn check_ytdlp_installed() -> Result<bool, String> {
    Ok(YTDLPInstaller::is_installed().await)
}

#[tauri::command]
pub async fn install_ytdlp(app_handle: AppHandle) -> Result<(), String> {
    YTDLPInstaller::install(&app_handle).await
}

#[tauri::command]
pub async fn get_ytdlp_version() -> Result<String, String> {
    YTDLPInstaller::get_version().await
}

#[tauri::command]
pub async fn check_ytdlp_update(app_handle: AppHandle) -> Result<Option<String>, String> {
    YTDLPInstaller::check_and_update(&app_handle).await
}

#[tauri::command]
pub async fn check_ffmpeg_available() -> Result<bool, String> {
    Ok(FfmpegInstaller::is_available().await)
}

#[tauri::command]
pub async fn install_ffmpeg(app_handle: AppHandle) -> Result<(), String> {
    FfmpegInstaller::ensure_available(&app_handle).await
}
