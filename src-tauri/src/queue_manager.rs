use crate::models::{QueueState, RepeatMode, YTVideoInfo};
use rand::seq::SliceRandom;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct QueueManager {
    state: Arc<Mutex<QueueState>>,
}

impl QueueManager {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(QueueState::default())),
        }
    }

    pub async fn add_to_queue(&self, track: YTVideoInfo) {
        let mut state = self.state.lock().await;
        state.queue.push(track);
        println!("➕ Added to queue. Total tracks: {}", state.queue.len());
    }

    pub async fn add_to_queue_batch(&self, tracks: Vec<YTVideoInfo>) {
        let mut state = self.state.lock().await;
        state.queue.extend(tracks);
        println!("➕ Added batch to queue. Total tracks: {}", state.queue.len());
    }

    pub async fn insert_next(&self, track: YTVideoInfo) {
        let mut state = self.state.lock().await;
        let insert_index = (state.current_index + 1).max(0) as usize;

        if insert_index >= state.queue.len() {
            state.queue.push(track);
        } else {
            state.queue.insert(insert_index, track);
        }

        println!("⏭️ Inserted track to play next");
    }

    pub async fn remove_from_queue(&self, index: usize) -> Result<(), String> {
        let mut state = self.state.lock().await;

        if index >= state.queue.len() {
            return Err("Invalid queue index".to_string());
        }

        state.queue.remove(index);

        // Adjust current index if needed
        if state.current_index >= index as i32 && state.current_index > 0 {
            state.current_index -= 1;
        }

        println!("🗑️ Removed track from queue. Remaining: {}", state.queue.len());
        Ok(())
    }

    pub async fn clear_queue(&self) {
        let mut state = self.state.lock().await;
        state.queue.clear();
        state.current_index = -1;
        println!("🧹 Queue cleared");
    }

    pub async fn play_track_at(&self, index: usize) -> Option<YTVideoInfo> {
        let mut state = self.state.lock().await;

        if index >= state.queue.len() {
            return None;
        }

        state.current_index = index as i32;
        state.queue.get(index).cloned()
    }

    pub async fn play_next(&self) -> Option<YTVideoInfo> {
        let mut state = self.state.lock().await;

        if state.queue.is_empty() {
            return None;
        }

        match state.repeat_mode {
            RepeatMode::One => {
                // Repeat current track
                if state.current_index >= 0 && (state.current_index as usize) < state.queue.len() {
                    state.queue.get(state.current_index as usize).cloned()
                } else {
                    None
                }
            }
            RepeatMode::All => {
                // Move to next track, loop back to start
                state.current_index = (state.current_index + 1) % state.queue.len() as i32;
                state.queue.get(state.current_index as usize).cloned()
            }
            RepeatMode::Off => {
                // Move to next track, stop at end
                let next_index = state.current_index + 1;
                if (next_index as usize) < state.queue.len() {
                    state.current_index = next_index;
                    state.queue.get(state.current_index as usize).cloned()
                } else {
                    None
                }
            }
        }
    }

    pub async fn play_previous(&self) -> Option<YTVideoInfo> {
        let mut state = self.state.lock().await;

        if state.queue.is_empty() {
            return None;
        }

        match state.repeat_mode {
            RepeatMode::One => {
                // Repeat current track
                if state.current_index >= 0 && (state.current_index as usize) < state.queue.len() {
                    state.queue.get(state.current_index as usize).cloned()
                } else {
                    None
                }
            }
            RepeatMode::All => {
                // Move to previous track, loop back to end
                state.current_index = if state.current_index <= 0 {
                    state.queue.len() as i32 - 1
                } else {
                    state.current_index - 1
                };
                state.queue.get(state.current_index as usize).cloned()
            }
            RepeatMode::Off => {
                // Move to previous track, stop at beginning
                if state.current_index > 0 {
                    state.current_index -= 1;
                    state.queue.get(state.current_index as usize).cloned()
                } else {
                    state.queue.get(0).cloned()
                }
            }
        }
    }

    pub async fn has_next(&self) -> bool {
        let state = self.state.lock().await;

        if state.queue.is_empty() {
            return false;
        }

        match state.repeat_mode {
            RepeatMode::One | RepeatMode::All => true,
            RepeatMode::Off => (state.current_index + 1) < state.queue.len() as i32,
        }
    }

    pub async fn has_previous(&self) -> bool {
        let state = self.state.lock().await;
        !state.queue.is_empty() && state.current_index >= 0
    }

    pub async fn toggle_shuffle(&self) -> bool {
        let mut state = self.state.lock().await;

        state.shuffle_mode = !state.shuffle_mode;

        if state.shuffle_mode {
            // Save original order
            state.original_queue = state.queue.clone();

            // Get current track before shuffle
            let current_track = if state.current_index >= 0 && (state.current_index as usize) < state.queue.len() {
                Some(state.queue[state.current_index as usize].clone())
            } else {
                None
            };

            // Shuffle the queue
            let mut rng = rand::thread_rng();
            state.queue.shuffle(&mut rng);

            // Move current track to the front if it exists
            if let Some(track) = current_track {
                if let Some(pos) = state.queue.iter().position(|t| t.id == track.id) {
                    state.queue.swap(0, pos);
                    state.current_index = 0;
                }
            }

            println!("🔀 Shuffle enabled");
        } else {
            // Restore original order
            if !state.original_queue.is_empty() {
                let current_track = if state.current_index >= 0 && (state.current_index as usize) < state.queue.len() {
                    Some(state.queue[state.current_index as usize].clone())
                } else {
                    None
                };

                state.queue = state.original_queue.clone();

                // Find current track in original order
                if let Some(track) = current_track {
                    if let Some(pos) = state.queue.iter().position(|t| t.id == track.id) {
                        state.current_index = pos as i32;
                    }
                }
            }

            println!("🔀 Shuffle disabled");
        }

        state.shuffle_mode
    }

    pub async fn cycle_repeat_mode(&self) -> RepeatMode {
        let mut state = self.state.lock().await;
        state.repeat_mode = state.repeat_mode.cycle();

        println!("🔁 Repeat mode: {}", state.repeat_mode.as_str());
        state.repeat_mode
    }

    pub async fn get_queue(&self) -> Vec<YTVideoInfo> {
        let state = self.state.lock().await;
        state.queue.clone()
    }

    pub async fn get_current_index(&self) -> i32 {
        let state = self.state.lock().await;
        state.current_index
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
    }

    pub async fn reorder_queue(&self, new_queue: Vec<YTVideoInfo>) -> Result<(), String> {
        let mut state = self.state.lock().await;

        if new_queue.len() != state.queue.len() {
            return Err("New queue length doesn't match current queue".to_string());
        }

        // Find current track to preserve playback position
        let current_track = if state.current_index >= 0 && (state.current_index as usize) < state.queue.len() {
            Some(state.queue[state.current_index as usize].clone())
        } else {
            None
        };

        // Update queue with new order
        state.queue = new_queue;

        // Find new index of current track
        if let Some(track) = current_track {
            if let Some(pos) = state.queue.iter().position(|t| t.id == track.id) {
                state.current_index = pos as i32;
            }
        }

        println!("🔄 Queue reordered");
        Ok(())
    }
}
