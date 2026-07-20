use crate::download_manager::{DownloadedTrack, DownloadProgress};
use crate::models::YTVideoInfo;
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn download_track(track: YTVideoInfo, state: State<'_, AppState>) -> Result<(), String> {
    state.downloads.download_track(track).await
}

#[tauri::command]
pub async fn get_active_downloads(state: State<'_, AppState>) -> Result<Vec<DownloadProgress>, String> {
    Ok(state.downloads.get_active_downloads().await)
}

#[tauri::command]
pub async fn get_downloaded_tracks(state: State<'_, AppState>) -> Result<Vec<DownloadedTrack>, String> {
    Ok(state.downloads.get_downloaded_tracks().await)
}

#[tauri::command]
pub async fn get_storage_used(state: State<'_, AppState>) -> Result<i64, String> {
    Ok(state.downloads.get_storage_used().await)
}

#[tauri::command]
pub async fn delete_download(video_id: String, state: State<'_, AppState>) -> Result<(), String> {
    state.downloads.delete_download(&video_id).await
}

#[tauri::command]
pub async fn cancel_download(video_id: String, state: State<'_, AppState>) -> Result<(), String> {
    state.downloads.cancel_download(&video_id).await
}

#[tauri::command]
pub async fn get_downloads_directory(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.downloads.get_downloads_directory().await)
}

#[tauri::command]
pub async fn set_downloads_directory(path: String, state: State<'_, AppState>) -> Result<(), String> {
    state.analytics.track("downloads_directory_changed");
    let path_buf = std::path::PathBuf::from(&path);
    state.downloads.set_downloads_dir(path_buf).await?;
    if let Ok(mut settings) = state.db.load_settings().await.map_err(|e| e.to_string()) {
        settings.default_download_path = path;
        let _ = state.db.save_settings(&settings).await;
    }
    Ok(())
}

#[tauri::command]
pub async fn get_audio_quality(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.downloads.get_audio_quality().await)
}

#[tauri::command]
pub async fn set_audio_quality(quality: String, state: State<'_, AppState>) -> Result<(), String> {
    state.analytics.track_with_data("audio_quality_changed", serde_json::json!({ "quality": quality }));
    state.downloads.set_audio_quality(quality.clone()).await?;
    if let Ok(mut settings) = state.db.load_settings().await.map_err(|e| e.to_string()) {
        settings.preferred_audio_quality = quality;
        let _ = state.db.save_settings(&settings).await;
    }
    Ok(())
}
