use crate::models::{QueueState, RepeatMode, YTVideoInfo};
use rand::seq::SliceRandom;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;

pub struct QueueManager {
    state: Arc<Mutex<QueueState>>,
    app_handle: Arc<Mutex<Option<AppHandle>>>,
}

impl QueueManager {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(QueueState::default())),
            app_handle: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn set_app_handle(&self, handle: AppHandle) {
        *self.app_handle.lock().await = Some(handle);
    }

    async fn emit_queue_update(&self) {
        if let Some(handle) = self.app_handle.lock().await.as_ref() {
            let _ = handle.emit("queue-updated", ());
        }
    }

    pub async fn add_to_queue(&self, track: YTVideoInfo) {
        let mut state = self.state.lock().await;

        if state.queue.iter().any(|t| t.id == track.id) {
            println!("⚠️ Track already in queue: {}", track.title);
            return;
        }

        state.queue.push(track);
        println!("➕ Added to queue. Total tracks: {}", state.queue.len());
        drop(state);
        self.emit_queue_update().await;
    }

    pub async fn add_to_queue_batch(&self, tracks: Vec<YTVideoInfo>) {
        let mut state = self.state.lock().await;
        state.queue.extend(tracks);
        println!("➕ Added batch to queue. Total tracks: {}", state.queue.len());
        drop(state);
        self.emit_queue_update().await;
    }

    pub async fn insert_next(&self, track: YTVideoInfo) {
        let mut state = self.state.lock().await;

        if state.queue.iter().any(|t| t.id == track.id) {
            println!("⚠️ Track already in queue: {}", track.title);
            return;
        }

        let insert_index = (state.current_index + 1).max(0) as usize;

        if insert_index >= state.queue.len() {
            state.queue.push(track);
        } else {
            state.queue.insert(insert_index, track);
        }

        println!("⏭️ Inserted track to play next");
        drop(state);
        self.emit_queue_update().await;
    }

    pub async fn remove_from_queue(&self, index: usize) -> Result<(), String> {
        let mut state = self.state.lock().await;

        if index >= state.queue.len() {
            return Err("Invalid queue index".to_string());
        }

        if state.current_index == index as i32 {
             if index == state.queue.len() - 1 {
                 state.current_index -= 1;
             }
        }
        else if (index as i32) < state.current_index {
             state.current_index -= 1;
        }

        state.queue.remove(index);


        println!("🗑️ Removed track from queue. Remaining: {}", state.queue.len());
        drop(state);
        self.emit_queue_update().await;
        Ok(())
    }

    pub async fn clear_queue(&self) {
        let mut state = self.state.lock().await;
        state.queue.clear();
        state.current_index = -1;
        println!("🧹 Queue cleared");
        drop(state);
        self.emit_queue_update().await;
    }

    pub async fn play_next(&self) -> Option<YTVideoInfo> {
        let mut state = self.state.lock().await;

        if state.queue.is_empty() {
            return None;
        }

        let result = match state.repeat_mode {
            RepeatMode::One => {
                if state.current_index >= 0 && (state.current_index as usize) < state.queue.len() {
                    state.queue.get(state.current_index as usize).cloned()
                } else {
                    None
                }
            }
            RepeatMode::All => {
                state.current_index = (state.current_index + 1) % state.queue.len() as i32;
                state.queue.get(state.current_index as usize).cloned()
            }
            RepeatMode::Off => {
                let next_index = state.current_index + 1;
                if (next_index as usize) < state.queue.len() {
                    state.current_index = next_index;
                    state.queue.get(state.current_index as usize).cloned()
                } else {
                    None
                }
            }
        };
        drop(state);
        self.emit_queue_update().await;
        result
    }

    pub async fn play_previous(&self) -> Option<YTVideoInfo> {
        let mut state = self.state.lock().await;

        if state.queue.is_empty() {
            return None;
        }

        let result = match state.repeat_mode {
            RepeatMode::One => {
                if state.current_index >= 0 && (state.current_index as usize) < state.queue.len() {
                    state.queue.get(state.current_index as usize).cloned()
                } else {
                    None
                }
            }
            RepeatMode::All => {
                state.current_index = if state.current_index <= 0 {
                    state.queue.len() as i32 - 1
                } else {
                    state.current_index - 1
                };
                state.queue.get(state.current_index as usize).cloned()
            }
            RepeatMode::Off => {
                if state.current_index > 0 {
                    state.current_index -= 1;
                    state.queue.get(state.current_index as usize).cloned()
                } else {
                    state.queue.get(0).cloned()
                }
            }
        };
        drop(state);
        self.emit_queue_update().await;
        result
    }

    pub async fn toggle_shuffle(&self) -> bool {
        let mut state = self.state.lock().await;

        state.shuffle_mode = !state.shuffle_mode;

        if state.shuffle_mode {
            state.original_queue = state.queue.clone();

            let current_track = if state.current_index >= 0 && (state.current_index as usize) < state.queue.len() {
                Some(state.queue[state.current_index as usize].clone())
            } else {
                None
            };

            let mut rng = rand::thread_rng();
            state.queue.shuffle(&mut rng);

            if let Some(track) = current_track {
                if let Some(pos) = state.queue.iter().position(|t| t.id == track.id) {
                    state.queue.swap(0, pos);
                    state.current_index = 0;
                }
            }

            println!("🔀 Shuffle enabled");
        } else {
            if !state.original_queue.is_empty() {
                let current_track = if state.current_index >= 0 && (state.current_index as usize) < state.queue.len() {
                    Some(state.queue[state.current_index as usize].clone())
                } else {
                    None
                };

                state.queue = state.original_queue.clone();

                if let Some(track) = current_track {
                    if let Some(pos) = state.queue.iter().position(|t| t.id == track.id) {
                        state.current_index = pos as i32;
                    }
                }
            }

            println!("🔀 Shuffle disabled");
        }

        let shuffle_mode = state.shuffle_mode;
        drop(state);
        self.emit_queue_update().await;
        shuffle_mode
    }

    pub async fn cycle_repeat_mode(&self) -> RepeatMode {
        let mut state = self.state.lock().await;
        state.repeat_mode = state.repeat_mode.cycle();

        println!("🔁 Repeat mode: {}", state.repeat_mode.as_str());
        let repeat_mode = state.repeat_mode;
        drop(state);
        self.emit_queue_update().await;
        repeat_mode
    }

    pub async fn get_shuffle_mode(&self) -> bool {
        let state = self.state.lock().await;
        state.shuffle_mode
    }

    pub async fn get_repeat_mode(&self) -> RepeatMode {
        let state = self.state.lock().await;
        state.repeat_mode
    }

    pub async fn get_queue(&self) -> Vec<YTVideoInfo> {
        let state = self.state.lock().await;
        state.queue.clone()
    }

    pub async fn get_queue_info(&self) -> String {
        let state = self.state.lock().await;

        if state.queue.is_empty() {
            return "Queue is empty".to_string();
        }

        let track_info = format!("Track {}/{}", state.current_index + 1, state.queue.len());
        let shuffle_info = if state.shuffle_mode { " • Shuffled" } else { "" };
        let repeat_info = match state.repeat_mode {
            RepeatMode::Off => "",
            RepeatMode::All => " • Repeat All",
            RepeatMode::One => " • Repeat One",
        };

        format!("{}{}{}", track_info, shuffle_info, repeat_info)
    }

    pub async fn set_current_index(&self, index: i32) {
        let mut state = self.state.lock().await;
        state.current_index = index;
        drop(state);
        self.emit_queue_update().await;
    }

    pub async fn reorder_queue(
        &self,
        new_queue: Vec<YTVideoInfo>,
        playing_track_id: Option<String>,
    ) -> Result<(), String> {
        let mut state = self.state.lock().await;

        if new_queue.len() != state.queue.len() {
            return Err("New queue length doesn't match current queue".to_string());
        }

        let mut current_counts: HashMap<&str, usize> = HashMap::new();
        for track in &state.queue {
            *current_counts.entry(track.id.as_str()).or_insert(0) += 1;
        }

        let mut new_counts: HashMap<&str, usize> = HashMap::new();
        for track in &new_queue {
            *new_counts.entry(track.id.as_str()).or_insert(0) += 1;
        }

        if current_counts != new_counts {
            return Err("New queue items do not match current queue".to_string());
        }

        // Prefer the audio manager's actual playing track as the source of truth,
        // since current_index can drift if a track was started outside the queue flow.
        let anchor_id = playing_track_id.or_else(|| {
            if state.current_index >= 0 && (state.current_index as usize) < state.queue.len() {
                Some(state.queue[state.current_index as usize].id.clone())
            } else {
                None
            }
        });

        state.queue = new_queue;

        if let Some(id) = anchor_id {
            if let Some(pos) = state.queue.iter().position(|t| t.id == id) {
                state.current_index = pos as i32;
            }
        }

        println!("🔄 Queue reordered (current_index={})", state.current_index);
        drop(state);
        self.emit_queue_update().await;
        Ok(())
    }

    pub async fn sync_current_index_to(&self, track_id: &str) {
        let mut state = self.state.lock().await;
        if let Some(pos) = state.queue.iter().position(|t| t.id == track_id) {
            state.current_index = pos as i32;
        }
        drop(state);
        self.emit_queue_update().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // play_next/play_previous's index math branches on repeat mode and wraps
    // around queue boundaries -- exactly the kind of off-by-one logic that's
    // easy to break silently. No AppHandle needed: emit_queue_update() no-ops
    // when one hasn't been set, so QueueManager::new() alone is testable.

    fn track(id: &str) -> YTVideoInfo {
        YTVideoInfo {
            id: id.to_string(),
            title: id.to_string(),
            uploader: "uploader".to_string(),
            duration: 100,
            thumbnail_url: None,
            audio_url: None,
            description: None,
        }
    }

    async fn queue_of(ids: &[&str]) -> QueueManager {
        let qm = QueueManager::new();
        qm.add_to_queue_batch(ids.iter().map(|id| track(id)).collect())
            .await;
        qm
    }

    #[tokio::test]
    async fn play_next_on_empty_queue_returns_none() {
        let qm = QueueManager::new();
        assert!(qm.play_next().await.is_none());
    }

    #[tokio::test]
    async fn repeat_off_advances_and_stops_at_the_end() {
        let qm = queue_of(&["a", "b", "c"]).await;
        qm.set_current_index(0).await;

        assert_eq!(qm.play_next().await.unwrap().id, "b");
        assert_eq!(qm.play_next().await.unwrap().id, "c");
        // No wraparound under RepeatMode::Off -- past the last track, None.
        assert!(qm.play_next().await.is_none());
    }

    #[tokio::test]
    async fn repeat_all_wraps_around_to_the_start() {
        let qm = queue_of(&["a", "b", "c"]).await;
        qm.set_current_index(2).await; // already on the last track
        qm.cycle_repeat_mode().await; // Off -> All

        assert_eq!(qm.get_repeat_mode().await, RepeatMode::All);
        assert_eq!(qm.play_next().await.unwrap().id, "a");
    }

    #[tokio::test]
    async fn repeat_one_keeps_returning_the_current_track() {
        let qm = queue_of(&["a", "b", "c"]).await;
        qm.set_current_index(1).await;
        qm.cycle_repeat_mode().await; // Off -> All
        qm.cycle_repeat_mode().await; // All -> One

        assert_eq!(qm.get_repeat_mode().await, RepeatMode::One);
        assert_eq!(qm.play_next().await.unwrap().id, "b");
        assert_eq!(qm.play_next().await.unwrap().id, "b");
    }

    #[tokio::test]
    async fn play_previous_off_stops_at_the_start_by_replaying_first_track() {
        let qm = queue_of(&["a", "b", "c"]).await;
        qm.set_current_index(0).await;

        // At index 0 under RepeatMode::Off, "previous" replays the first
        // track rather than returning None or going negative.
        assert_eq!(qm.play_previous().await.unwrap().id, "a");
    }

    #[tokio::test]
    async fn play_previous_all_wraps_to_the_end() {
        let qm = queue_of(&["a", "b", "c"]).await;
        qm.set_current_index(0).await;
        qm.cycle_repeat_mode().await; // Off -> All

        assert_eq!(qm.play_previous().await.unwrap().id, "c");
    }

    #[tokio::test]
    async fn remove_from_queue_rejects_an_out_of_range_index() {
        let qm = queue_of(&["a", "b"]).await;
        assert!(qm.remove_from_queue(5).await.is_err());
    }

    #[tokio::test]
    async fn insert_next_places_track_immediately_after_current() {
        let qm = queue_of(&["a", "b", "c"]).await;
        qm.set_current_index(0).await;

        qm.insert_next(track("x")).await;

        let ids: Vec<String> = qm.get_queue().await.into_iter().map(|t| t.id).collect();
        assert_eq!(ids, vec!["a", "x", "b", "c"]);
    }

    #[tokio::test]
    async fn add_to_queue_rejects_a_duplicate_id() {
        let qm = queue_of(&["a"]).await;
        qm.add_to_queue(track("a")).await;
        assert_eq!(qm.get_queue().await.len(), 1);
    }

    #[tokio::test]
    async fn insert_next_rejects_a_duplicate_id() {
        let qm = queue_of(&["a", "b"]).await;
        qm.insert_next(track("a")).await;
        assert_eq!(qm.get_queue().await.len(), 2);
    }

    #[tokio::test]
    async fn clear_queue_empties_the_queue_and_resets_current_index() {
        let qm = queue_of(&["a", "b"]).await;
        qm.set_current_index(1).await;

        qm.clear_queue().await;

        assert!(qm.get_queue().await.is_empty());
        assert!(qm.play_next().await.is_none());
    }

    #[tokio::test]
    async fn toggle_shuffle_keeps_the_same_set_of_tracks() {
        let qm = queue_of(&["a", "b", "c", "d", "e"]).await;
        qm.set_current_index(0).await;

        let enabled = qm.toggle_shuffle().await;
        assert!(enabled);
        assert!(qm.get_shuffle_mode().await);

        let mut shuffled_ids: Vec<String> =
            qm.get_queue().await.into_iter().map(|t| t.id).collect();
        shuffled_ids.sort();
        assert_eq!(shuffled_ids, vec!["a", "b", "c", "d", "e"]);
    }

    #[tokio::test]
    async fn toggle_shuffle_moves_the_current_track_to_the_front() {
        let qm = queue_of(&["a", "b", "c", "d", "e"]).await;
        qm.set_current_index(2).await; // "c" is current

        qm.toggle_shuffle().await;

        let queue = qm.get_queue().await;
        assert_eq!(queue[0].id, "c");
    }

    #[tokio::test]
    async fn toggling_shuffle_off_restores_original_order() {
        let qm = queue_of(&["a", "b", "c", "d", "e"]).await;
        qm.set_current_index(0).await;

        qm.toggle_shuffle().await; // on
        let disabled = qm.toggle_shuffle().await; // off

        assert!(!disabled);
        assert!(!qm.get_shuffle_mode().await);
        let ids: Vec<String> = qm.get_queue().await.into_iter().map(|t| t.id).collect();
        assert_eq!(ids, vec!["a", "b", "c", "d", "e"]);
    }

    #[tokio::test]
    async fn get_queue_info_reports_empty_queue() {
        let qm = QueueManager::new();
        assert_eq!(qm.get_queue_info().await, "Queue is empty");
    }

    #[tokio::test]
    async fn get_queue_info_reports_position_and_modifiers() {
        let qm = queue_of(&["a", "b", "c"]).await;
        qm.set_current_index(1).await;
        qm.cycle_repeat_mode().await; // Off -> All

        assert_eq!(qm.get_queue_info().await, "Track 2/3 • Repeat All");
    }

    #[tokio::test]
    async fn reorder_queue_rejects_a_length_mismatch() {
        let qm = queue_of(&["a", "b", "c"]).await;
        let result = qm
            .reorder_queue(vec![track("a"), track("b")], None)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn reorder_queue_rejects_a_different_set_of_tracks() {
        let qm = queue_of(&["a", "b", "c"]).await;
        let result = qm
            .reorder_queue(vec![track("a"), track("b"), track("x")], None)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn reorder_queue_accepts_a_valid_permutation_and_tracks_the_anchor() {
        let qm = queue_of(&["a", "b", "c"]).await;
        qm.set_current_index(0).await; // "a" is playing

        qm.reorder_queue(vec![track("c"), track("b"), track("a")], None)
            .await
            .unwrap();

        let ids: Vec<String> = qm.get_queue().await.into_iter().map(|t| t.id).collect();
        assert_eq!(ids, vec!["c", "b", "a"]);
        // "a" is still the anchor even though its position moved to index 2.
        assert_eq!(qm.play_previous().await.unwrap().id, "b");
    }

    #[tokio::test]
    async fn reorder_queue_prefers_explicit_playing_track_id_over_current_index() {
        let qm = queue_of(&["a", "b", "c"]).await;
        qm.set_current_index(0).await; // stale anchor: "a"

        qm.reorder_queue(
            vec![track("c"), track("b"), track("a")],
            Some("c".to_string()),
        )
        .await
        .unwrap();

        // Anchored on "c" (index 0 post-reorder) rather than the stale "a".
        assert_eq!(qm.play_next().await.unwrap().id, "b");
    }
}
