use crate::ffmpeg_installer::FfmpegInstaller;
use crate::models::{YTPlaylistInfo, YTPlaylistPreview, YTVideoInfo};
use crate::ytdlp_installer::YTDLPInstaller;
use crate::ytdlp_manager::YTDLPManager;
use crate::AppState;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn search_youtube(
    query: String,
    state: State<'_, AppState>,
) -> Result<Vec<YTVideoInfo>, String> {
    state.analytics.track("search_performed");
    state.ytdlp.search(query).await
}

#[tauri::command]
pub async fn search_playlists(
    query: String,
    state: State<'_, AppState>,
) -> Result<Vec<YTPlaylistInfo>, String> {
    state.analytics.track("search_performed");
    state.ytdlp.search_playlists(query).await
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
