use crate::models::{RepeatMode, YTVideoInfo};
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn add_to_queue(track: YTVideoInfo, state: State<'_, AppState>) -> Result<(), String> {
    state.queue.add_to_queue(track).await;
    Ok(())
}

#[tauri::command]
pub async fn add_to_queue_next(track: YTVideoInfo, state: State<'_, AppState>) -> Result<(), String> {
    state.queue.insert_next(track).await;
    Ok(())
}

#[tauri::command]
pub async fn get_queue(state: State<'_, AppState>) -> Result<Vec<YTVideoInfo>, String> {
    Ok(state.queue.get_queue().await)
}

#[tauri::command]
pub async fn clear_queue(state: State<'_, AppState>) -> Result<(), String> {
    state.queue.clear_queue().await;
    Ok(())
}

#[tauri::command]
pub async fn toggle_shuffle(state: State<'_, AppState>) -> Result<bool, String> {
    state.analytics.track_with_data("queue_action", serde_json::json!({ "action": "shuffle_toggled" }));
    Ok(state.queue.toggle_shuffle().await)
}

#[tauri::command]
pub async fn get_shuffle_mode(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.queue.get_shuffle_mode().await)
}

#[tauri::command]
pub async fn cycle_repeat_mode(state: State<'_, AppState>) -> Result<RepeatMode, String> {
    state.analytics.track_with_data("queue_action", serde_json::json!({ "action": "repeat_changed" }));
    Ok(state.queue.cycle_repeat_mode().await)
}

#[tauri::command]
pub async fn get_repeat_mode(state: State<'_, AppState>) -> Result<RepeatMode, String> {
    Ok(state.queue.get_repeat_mode().await)
}

#[tauri::command]
pub async fn get_queue_info(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.queue.get_queue_info().await)
}

#[tauri::command]
pub async fn reorder_queue(new_queue: Vec<YTVideoInfo>, state: State<'_, AppState>) -> Result<(), String> {
    state.analytics.track_with_data("queue_action", serde_json::json!({ "action": "reordered" }));
    let playing_track_id = state.audio.get_state().await.current_track.map(|t| t.id);
    state.queue.reorder_queue(new_queue, playing_track_id).await
}

#[tauri::command]
pub async fn remove_from_queue(index: usize, state: State<'_, AppState>) -> Result<(), String> {
    state.queue.remove_from_queue(index).await
}
