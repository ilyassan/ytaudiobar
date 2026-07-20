use crate::commands::search::get_video_details;
use crate::models::{AudioState, YTVideoInfo};
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn play_track(mut track: YTVideoInfo, state: State<'_, AppState>) -> Result<(), String> {
    let _ = state.audio.stop().await;

    state.audio.set_loading_state(&track).await;

    // Keep the queue's current_index in lockstep with what's actually playing
    // so drag-reorder, play_next, and play_previous can reason from the right anchor.
    state.queue.sync_current_index_to(&track.id).await;

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
pub async fn toggle_play_pause(state: State<'_, AppState>) -> Result<(), String> {
    state.audio.toggle_play_pause().await
}

#[tauri::command]
pub async fn pause_playback(state: State<'_, AppState>) -> Result<(), String> {
    state.audio.pause().await
}

#[tauri::command]
pub async fn stop_playback(state: State<'_, AppState>) -> Result<(), String> {
    state.audio.stop().await
}

#[tauri::command]
pub async fn seek_to(position: f64, state: State<'_, AppState>) -> Result<(), String> {
    state.audio.seek(position).await
}

#[tauri::command]
pub async fn set_volume(volume: f32, state: State<'_, AppState>) -> Result<(), String> {
    state.audio.set_volume(volume).await
}

#[tauri::command]
pub async fn set_playback_speed(rate: f32, state: State<'_, AppState>) -> Result<(), String> {
    state.audio.set_playback_rate(rate).await
}

#[tauri::command]
pub async fn play_next(state: State<'_, AppState>) -> Result<Option<YTVideoInfo>, String> {
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
pub async fn play_previous(state: State<'_, AppState>) -> Result<Option<YTVideoInfo>, String> {
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
pub async fn get_audio_state(state: State<'_, AppState>) -> Result<AudioState, String> {
    Ok(state.audio.get_state().await)
}

#[tauri::command]
pub async fn reinit_audio(state: State<'_, AppState>) -> Result<(), String> {
    state.audio.reinit_audio().await;
    Ok(())
}
