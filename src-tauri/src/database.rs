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

        // WAL lets readers and writers proceed concurrently instead of taking a whole-file
        // lock per write, and NORMAL sync (safe under WAL) skips an fsync most writers don't
        // need on a single-user desktop app — this speeds up every save (window geometry
        // while dragging/resizing, playlist edits) without any durability risk that matters
        // here (worst case on an actual crash is losing the last unflushed write, not corruption).
        sqlx::query("PRAGMA journal_mode = WAL").execute(&pool).await?;
        sqlx::query("PRAGMA synchronous = NORMAL").execute(&pool).await?;

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

        // Column-add migrations and the position backfill are only ever needed once;
        // gate them behind the schema version so every subsequent startup does a
        // single cheap PRAGMA read instead of 6 ALTER TABLE attempts + a full scan.
        const SCHEMA_VERSION: i64 = 1;
        let current_version: i64 = sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(&self.pool)
            .await
            .unwrap_or(0);

        if current_version < SCHEMA_VERSION {
            // Migrate: add window geometry columns if they don't exist yet
            let _ = sqlx::query("ALTER TABLE app_settings ADD COLUMN window_x INTEGER").execute(&self.pool).await;
            let _ = sqlx::query("ALTER TABLE app_settings ADD COLUMN window_y INTEGER").execute(&self.pool).await;
            let _ = sqlx::query("ALTER TABLE app_settings ADD COLUMN window_width INTEGER").execute(&self.pool).await;
            let _ = sqlx::query("ALTER TABLE app_settings ADD COLUMN window_height INTEGER").execute(&self.pool).await;
            let _ = sqlx::query("ALTER TABLE app_settings ADD COLUMN is_mini_mode INTEGER DEFAULT 0").execute(&self.pool).await;

            // Migrate: add position column for custom playlist track ordering
            let _ = sqlx::query("ALTER TABLE playlist_memberships ADD COLUMN position INTEGER").execute(&self.pool).await;
            self.backfill_membership_positions().await?;

            sqlx::query(&format!("PRAGMA user_version = {}", SCHEMA_VERSION))
                .execute(&self.pool)
                .await?;
        }

        // Create system "All Favorites" playlist if not exists
        self.create_system_playlist().await?;

        Ok(())
    }

    async fn backfill_membership_positions(&self) -> Result<(), sqlx::Error> {
        let playlist_ids: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT playlist_id FROM playlist_memberships WHERE position IS NULL"
        )
        .fetch_all(&self.pool)
        .await?;

        for playlist_id in playlist_ids {
            let membership_ids: Vec<String> = sqlx::query_scalar(
                "SELECT id FROM playlist_memberships WHERE playlist_id = ? AND position IS NULL ORDER BY added_date ASC"
            )
            .bind(&playlist_id)
            .fetch_all(&self.pool)
            .await?;

            for (index, membership_id) in membership_ids.into_iter().enumerate() {
                sqlx::query("UPDATE playlist_memberships SET position = ? WHERE id = ?")
                    .bind(index as i64)
                    .bind(membership_id)
                    .execute(&self.pool)
                    .await?;
            }
        }

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

    pub async fn update_playlist_name(&self, id: &str, name: &str) -> Result<(), sqlx::Error> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Ok(());
        }

        sqlx::query("UPDATE playlists SET name = ? WHERE id = ? AND is_system_playlist = 0")
            .bind(trimmed)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn add_track_to_playlist(&self, track_id: &str, playlist_id: &str) -> Result<(), sqlx::Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();

        let next_position: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM playlist_memberships WHERE playlist_id = ?"
        )
        .bind(playlist_id)
        .fetch_one(&self.pool)
        .await?;

        sqlx::query(
            "INSERT INTO playlist_memberships (id, playlist_id, track_id, added_date, is_favorite, position) VALUES (?, ?, ?, ?, 0, ?)"
        )
        .bind(&id)
        .bind(playlist_id)
        .bind(track_id)
        .bind(now)
        .bind(next_position)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn reorder_playlist_tracks(&self, playlist_id: &str, track_ids: &[String]) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        for (index, track_id) in track_ids.iter().enumerate() {
            sqlx::query(
                "UPDATE playlist_memberships SET position = ? WHERE playlist_id = ? AND track_id = ?"
            )
            .bind(index as i64)
            .bind(playlist_id)
            .bind(track_id)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
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
            ORDER BY pm.position ASC
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

    pub async fn save_window_geometry(&self, x: i32, y: i32, width: u32, height: u32) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO app_settings (id, window_x, window_y, window_width, window_height)
            VALUES ('default', ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                window_x = excluded.window_x,
                window_y = excluded.window_y,
                window_width = excluded.window_width,
                window_height = excluded.window_height
            "#
        )
        .bind(x)
        .bind(y)
        .bind(width as i64)
        .bind(height as i64)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn load_window_geometry(&self) -> Result<Option<(i32, i32, u32, u32)>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT window_x, window_y, window_width, window_height FROM app_settings WHERE id = 'default'"
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.and_then(|r| {
            let x: Option<i64> = r.get("window_x");
            let y: Option<i64> = r.get("window_y");
            let w: Option<i64> = r.get("window_width");
            let h: Option<i64> = r.get("window_height");
            match (x, y, w, h) {
                (Some(x), Some(y), Some(w), Some(h)) => Some((x as i32, y as i32, w as u32, h as u32)),
                _ => None,
            }
        }))
    }

    pub async fn save_mini_mode(&self, is_mini: bool) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO app_settings (id, is_mini_mode)
            VALUES ('default', ?)
            ON CONFLICT(id) DO UPDATE SET is_mini_mode = excluded.is_mini_mode
            "#
        )
        .bind(is_mini as i64)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn load_mini_mode(&self) -> Result<bool, sqlx::Error> {
        let row = sqlx::query(
            "SELECT is_mini_mode FROM app_settings WHERE id = 'default'"
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.and_then(|r| {
            let v: Option<i64> = r.get("is_mini_mode");
            v
        }).unwrap_or(0) != 0)
    }
}
