use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YTVideoInfo {
    pub id: String,
    pub title: String,
    pub uploader: String,
    pub duration: i64,
    pub thumbnail_url: Option<String>,
    pub audio_url: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YTPlaylistInfo {
    pub id: String,
    pub title: String,
    pub thumbnail_url: Option<String>,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YTPlaylistPreview {
    pub id: String,
    pub title: String,
    pub uploader: String,
    pub track_count: i64,
    pub tracks: Vec<YTVideoInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioState {
    pub is_playing: bool,
    pub current_position: f64,
    pub duration: f64,
    pub volume: f32,
    pub playback_rate: f32,
    pub current_track: Option<YTVideoInfo>,
    pub is_loading: bool,
    pub download_progress: f64, // 0.0 to 1.0, for progressive seeking on streamed tracks
    pub playback_error: Option<String>, // set when playback permanently fails; cleared on next play
}

impl Default for AudioState {
    fn default() -> Self {
        Self {
            is_playing: false,
            current_position: 0.0,
            duration: 0.0,
            volume: 1.0,
            playback_rate: 1.0,
            current_track: None,
            is_loading: false,
            download_progress: 1.0, // 1.0 = fully available (default for downloaded tracks)
            playback_error: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum RepeatMode {
    Off,
    All,
    One,
}

impl RepeatMode {
    pub fn cycle(&self) -> Self {
        match self {
            RepeatMode::Off => RepeatMode::All,
            RepeatMode::All => RepeatMode::One,
            RepeatMode::One => RepeatMode::Off,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            RepeatMode::Off => "Off",
            RepeatMode::All => "All",
            RepeatMode::One => "One",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueState {
    pub queue: Vec<YTVideoInfo>,
    pub current_index: i32,
    pub shuffle_mode: bool,
    pub repeat_mode: RepeatMode,
    pub original_queue: Vec<YTVideoInfo>,
}

impl Default for QueueState {
    fn default() -> Self {
        Self {
            queue: Vec::new(),
            current_index: -1,
            shuffle_mode: false,
            repeat_mode: RepeatMode::Off,
            original_queue: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub default_download_path: String,
    pub preferred_audio_quality: String,
    pub auto_update_ytdlp: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            default_download_path: String::new(),
            preferred_audio_quality: "best".to_string(),
            auto_update_ytdlp: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub author: Option<String>,
    pub duration: i64,
    pub thumbnail_url: Option<String>,
    pub added_date: i64,
    pub file_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub created_date: i64,
    pub is_system_playlist: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistWithCount {
    pub id: String,
    pub name: String,
    pub created_date: i64,
    pub is_system_playlist: bool,
    pub track_count: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeat_mode_cycles_off_all_one_and_back_to_off() {
        assert_eq!(RepeatMode::Off.cycle(), RepeatMode::All);
        assert_eq!(RepeatMode::All.cycle(), RepeatMode::One);
        assert_eq!(RepeatMode::One.cycle(), RepeatMode::Off);
    }

    #[test]
    fn repeat_mode_cycle_is_a_closed_loop_of_three() {
        let mut mode = RepeatMode::Off;
        for _ in 0..3 {
            mode = mode.cycle();
        }
        assert_eq!(mode, RepeatMode::Off);
    }

    #[test]
    fn repeat_mode_as_str_matches_each_variant() {
        assert_eq!(RepeatMode::Off.as_str(), "Off");
        assert_eq!(RepeatMode::All.as_str(), "All");
        assert_eq!(RepeatMode::One.as_str(), "One");
    }

    #[test]
    fn audio_state_default_is_a_stopped_fully_available_state() {
        let state = AudioState::default();
        assert!(!state.is_playing);
        assert_eq!(state.current_position, 0.0);
        assert_eq!(state.volume, 1.0);
        assert_eq!(state.playback_rate, 1.0);
        assert!(state.current_track.is_none());
        assert!(!state.is_loading);
        assert_eq!(state.download_progress, 1.0);
        assert!(state.playback_error.is_none());
    }

    #[test]
    fn queue_state_default_is_empty_with_no_current_track() {
        let state = QueueState::default();
        assert!(state.queue.is_empty());
        assert_eq!(state.current_index, -1);
        assert!(!state.shuffle_mode);
        assert_eq!(state.repeat_mode, RepeatMode::Off);
        assert!(state.original_queue.is_empty());
    }

    #[test]
    fn app_settings_default_prefers_best_quality_and_auto_update_on() {
        let settings = AppSettings::default();
        assert_eq!(settings.default_download_path, "");
        assert_eq!(settings.preferred_audio_quality, "best");
        assert!(settings.auto_update_ytdlp);
    }
}
