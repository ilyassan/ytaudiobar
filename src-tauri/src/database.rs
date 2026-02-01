use sqlx::{sqlite::SqlitePool, Row};
use std::path::PathBuf;
use crate::models::{AppSettings, Playlist, Track};

pub struct DatabaseManager {
    pool: SqlitePool,
}

impl DatabaseManager {
    pub async fn new() -> Result<Self, sqlx::Error> {
        let db_path = Self::get_db_path();

        // Create directory if it doesn't exist
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let db_url = format!("sqlite:{}?mode=rwc", db_path.display());
        let pool = SqlitePool::connect(&db_url).await?;

        let manager = Self { pool };
        manager.init_database().await?;

        Ok(manager)
    }

    fn get_db_path() -> PathBuf {
        let mut path = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."));
        path.push("ytaudiobar");
        path.push("ytaudiobar.db");
        path
    }

    async fn init_database(&self) -> Result<(), sqlx::Error> {
        // Create tracks table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS tracks (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                author TEXT,
                duration INTEGER,
                thumbnail_url TEXT,
                added_date INTEGER,
                file_path TEXT
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Create playlists table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS playlists (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                created_date INTEGER,
                is_system_playlist BOOLEAN DEFAULT 0
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Create playlist_memberships table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS playlist_memberships (
                id TEXT PRIMARY KEY,
                playlist_id TEXT,
                track_id TEXT,
                added_date INTEGER,
                is_favorite BOOLEAN DEFAULT 0,
                FOREIGN KEY (playlist_id) REFERENCES playlists(id) ON DELETE CASCADE,
                FOREIGN KEY (track_id) REFERENCES tracks(id) ON DELETE CASCADE
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Create app_settings table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS app_settings (
                id TEXT PRIMARY KEY,
                default_download_path TEXT,
                preferred_audio_quality TEXT DEFAULT 'best',
                auto_update_ytdlp BOOLEAN DEFAULT 1
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Create system "All Favorites" playlist if not exists
        self.create_system_playlist().await?;

        Ok(())
    }

    async fn create_system_playlist(&self) -> Result<(), sqlx::Error> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM playlists WHERE is_system_playlist = 1 LIMIT 1)",
        )
        .fetch_one(&self.pool)
        .await?;

        if !exists {
            let now = chrono::Utc::now().timestamp();
            sqlx::query(
                r#"
                INSERT INTO playlists (id, name, created_date, is_system_playlist)
                VALUES ('favorites', 'All Favorites', ?, 1)
                "#,
            )
            .bind(now)
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }

    pub async fn save_track(&self, track: &Track) -> Result<(), sqlx::Error> {
        // Use INSERT OR IGNORE instead of REPLACE to avoid triggering ON DELETE CASCADE
        // which would delete all playlist memberships when track already exists
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO tracks (id, title, author, duration, thumbnail_url, added_date, file_path)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&track.id)
        .bind(&track.title)
        .bind(&track.author)
        .bind(track.duration)
        .bind(&track.thumbnail_url)
        .bind(track.added_date)
        .bind(&track.file_path)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_track(&self, id: &str) -> Result<Option<Track>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT id, title, author, duration, thumbnail_url, added_date, file_path FROM tracks WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| Track {
            id: r.get("id"),
            title: r.get("title"),
            author: r.get("author"),
            duration: r.get("duration"),
            thumbnail_url: r.get("thumbnail_url"),
            added_date: r.get("added_date"),
            file_path: r.get("file_path"),
        }))
    }

    pub async fn delete_track(&self, id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM tracks WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn create_playlist(&self, name: &str) -> Result<String, sqlx::Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();

        sqlx::query(
            "INSERT INTO playlists (id, name, created_date, is_system_playlist) VALUES (?, ?, ?, 0)"
        )
        .bind(&id)
        .bind(name)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(id)
    }

    pub async fn delete_playlist(&self, id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM playlists WHERE id = ? AND is_system_playlist = 0")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn add_track_to_playlist(&self, track_id: &str, playlist_id: &str) -> Result<(), sqlx::Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();

        sqlx::query(
            "INSERT INTO playlist_memberships (id, playlist_id, track_id, added_date, is_favorite) VALUES (?, ?, ?, ?, 0)"
        )
        .bind(&id)
        .bind(playlist_id)
        .bind(track_id)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn remove_track_from_playlist(&self, track_id: &str, playlist_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM playlist_memberships WHERE track_id = ? AND playlist_id = ?")
            .bind(track_id)
            .bind(playlist_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn get_playlist_tracks(&self, playlist_id: &str) -> Result<Vec<Track>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT t.id, t.title, t.author, t.duration, t.thumbnail_url, t.added_date, t.file_path
            FROM tracks t
            INNER JOIN playlist_memberships pm ON t.id = pm.track_id
            WHERE pm.playlist_id = ?
            ORDER BY pm.added_date DESC
            "#
        )
        .bind(playlist_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| Track {
                id: r.get("id"),
                title: r.get("title"),
                author: r.get("author"),
                duration: r.get("duration"),
                thumbnail_url: r.get("thumbnail_url"),
                added_date: r.get("added_date"),
                file_path: r.get("file_path"),
            })
            .collect())
    }

    pub async fn add_to_favorites(&self, track_id: &str) -> Result<(), sqlx::Error> {
        self.add_track_to_playlist(track_id, "favorites").await
    }

    pub async fn remove_from_favorites(&self, track_id: &str) -> Result<(), sqlx::Error> {
        self.remove_track_from_playlist(track_id, "favorites").await
    }

    pub async fn get_favorites(&self) -> Result<Vec<Track>, sqlx::Error> {
        self.get_playlist_tracks("favorites").await
    }

    pub async fn get_all_playlists(&self) -> Result<Vec<Playlist>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, name, created_date, is_system_playlist FROM playlists ORDER BY is_system_playlist DESC, created_date ASC"
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| Playlist {
                id: r.get("id"),
                name: r.get("name"),
                created_date: r.get("created_date"),
                is_system_playlist: r.get("is_system_playlist"),
            })
            .collect())
    }

    pub async fn save_settings(&self, settings: &AppSettings) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO app_settings (id, default_download_path, preferred_audio_quality, auto_update_ytdlp)
            VALUES ('default', ?, ?, ?)
            "#
        )
        .bind(&settings.default_download_path)
        .bind(&settings.preferred_audio_quality)
        .bind(settings.auto_update_ytdlp)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn load_settings(&self) -> Result<AppSettings, sqlx::Error> {
        let row = sqlx::query(
            "SELECT default_download_path, preferred_audio_quality, auto_update_ytdlp FROM app_settings WHERE id = 'default'"
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| AppSettings {
            default_download_path: r.get("default_download_path"),
            preferred_audio_quality: r.get("preferred_audio_quality"),
            auto_update_ytdlp: r.get("auto_update_ytdlp"),
        }).unwrap_or_default())
    }
}
