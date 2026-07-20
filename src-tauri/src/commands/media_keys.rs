use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn update_media_metadata(
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
pub async fn update_media_playback_state(
    is_playing: bool,
    position: f64,
    duration: f64,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.media_keys.update_playback_state(is_playing, position, duration).await;
    Ok(())
}

#[tauri::command]
pub async fn clear_media_info(state: State<'_, AppState>) -> Result<(), String> {
    state.media_keys.clear().await;
    Ok(())
}
