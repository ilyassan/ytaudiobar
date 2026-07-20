use crate::command_utils::unix_timestamp;
use crate::models::{Playlist, PlaylistWithCount, Track, YTVideoInfo};
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn get_all_playlists(state: State<'_, AppState>) -> Result<Vec<Playlist>, String> {
    state
        .db
        .get_all_playlists()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_all_playlists_with_counts(state: State<'_, AppState>) -> Result<Vec<PlaylistWithCount>, String> {
    state
        .db
        .get_all_playlists_with_counts()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_playlist_ids_containing_track(track_id: String, state: State<'_, AppState>) -> Result<Vec<String>, String> {
    state
        .db
        .get_playlist_ids_containing_track(&track_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_playlist(name: String, state: State<'_, AppState>) -> Result<String, String> {
    state.analytics.track("playlist_created");
    state.db.create_playlist(&name).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_playlist(id: String, state: State<'_, AppState>) -> Result<(), String> {
    state.db.delete_playlist(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_playlist_name(id: String, name: String, state: State<'_, AppState>) -> Result<(), String> {
    state.db
        .update_playlist_name(&id, &name)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_playlist_tracks(playlist_id: String, state: State<'_, AppState>) -> Result<Vec<Track>, String> {
    state
        .db
        .get_playlist_tracks(&playlist_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn reorder_playlist_tracks(
    playlist_id: String,
    track_ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .db
        .reorder_playlist_tracks(&playlist_id, &track_ids)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn add_track_to_playlist(
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
        added_date: unix_timestamp(),
        file_path: None,
    };

    state.db.save_track(&db_track).await.map_err(|e| e.to_string())?;

    state.analytics.track("track_added_to_playlist");

    state
        .db
        .add_track_to_playlist(&track.id, &playlist_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn remove_track_from_playlist(
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
pub async fn add_to_favorites(track: YTVideoInfo, state: State<'_, AppState>) -> Result<(), String> {
    let db_track = Track {
        id: track.id.clone(),
        title: track.title,
        author: Some(track.uploader),
        duration: track.duration,
        thumbnail_url: track.thumbnail_url,
        added_date: unix_timestamp(),
        file_path: None,
    };

    state.db.save_track(&db_track).await.map_err(|e| e.to_string())?;

    state.analytics.track("track_favorited");

    state
        .db
        .add_to_favorites(&track.id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn remove_from_favorites(track_id: String, state: State<'_, AppState>) -> Result<(), String> {
    state
        .db
        .remove_from_favorites(&track_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn play_playlist(playlist_id: String, state: State<'_, AppState>) -> Result<(), String> {
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

    play_track_list_internal(video_tracks, &state).await
}

#[tauri::command]
pub async fn play_track_list(tracks: Vec<YTVideoInfo>, state: State<'_, AppState>) -> Result<(), String> {
    play_track_list_internal(tracks, &state).await
}

pub async fn play_track_list_internal(tracks: Vec<YTVideoInfo>, state: &State<'_, AppState>) -> Result<(), String> {
    if tracks.is_empty() {
        return Err("Playlist is empty".to_string());
    }

    state.queue.clear_queue().await;
    state.queue.add_to_queue_batch(tracks.clone()).await;

    state.queue.set_current_index(0).await;

    if let Some(first_track) = tracks.first() {
        if let Some(file_path) = state.downloads.get_downloaded_file_path(&first_track.id).await {
            println!("🎵 Playing playlist first track from local file: {}", file_path);
            state.audio.play_from_file(first_track.clone(), file_path).await?;
        } else {
            state.audio.play(first_track.clone()).await?;
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn import_playlist(name: String, tracks: Vec<YTVideoInfo>, state: State<'_, AppState>) -> Result<String, String> {
    state.analytics.track("playlist_imported");
    let playlist_id = state.db.create_playlist(&name).await.map_err(|e| e.to_string())?;

    for track in tracks {
        let db_track = Track {
            id: track.id.clone(),
            title: track.title,
            author: Some(track.uploader),
            duration: track.duration,
            thumbnail_url: track.thumbnail_url,
            added_date: unix_timestamp(),
            file_path: None,
        };

        state.db.save_track(&db_track).await.map_err(|e| e.to_string())?;
        state
            .db
            .add_track_to_playlist(&track.id, &playlist_id)
            .await
            .map_err(|e| e.to_string())?;
    }

    Ok(playlist_id)
}
