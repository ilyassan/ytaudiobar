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
pub struct AudioState {
    pub is_playing: bool,
    pub current_position: f64,
    pub duration: f64,
    pub volume: f32,
    pub playback_rate: f32,
    pub current_track: Option<YTVideoInfo>,
    pub is_loading: bool,
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
pub struct DownloadProgress {
    pub video_id: String,
    pub progress: f64,
    pub speed: String,
    pub eta: String,
    pub file_size: String,
    pub is_completed: bool,
    pub error: Option<String>,
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
