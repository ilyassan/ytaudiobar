use sqlx::{sqlite::SqlitePool, Row};
use std::path::PathBuf;
use crate::models::{AppSettings, Playlist, PlaylistWithCount, Track};
use crate::command_utils::unix_timestamp;

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

        Self::from_pool(pool).await
    }

    // Shared by the real on-disk connection above and the in-memory pool the
    // test suite below uses -- same schema/migration path either way, so
    // tests exercise the exact same init_database() logic production does.
    async fn from_pool(pool: SqlitePool) -> Result<Self, sqlx::Error> {
        let manager = Self { pool };
        manager.init_database().await?;
        Ok(manager)
    }

    #[cfg(test)]
    async fn new_in_memory() -> Result<Self, sqlx::Error> {
        // A pool with more than one connection would give each connection its
        // own private ":memory:" database in SQLite -- capping at 1 connection
        // keeps every query in a test on the same, consistent in-memory DB.
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        Self::from_pool(pool).await
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
        const SCHEMA_VERSION: i64 = 5;
        let current_version: i64 = sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(&self.pool)
            .await
            .unwrap_or(0);

        if current_version < SCHEMA_VERSION {
            // Whether app_settings already had a row *before* any migration below
            // runs -- i.e. whether this is a real existing install, not a brand
            // new one whose row hasn't been created yet. Needed for the
            // last_seen_version backfill further down.
            let had_existing_settings_row: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM app_settings")
                .fetch_one(&self.pool)
                .await
                .unwrap_or(0);

            // Migrate: add window geometry columns if they don't exist yet
            let _ = sqlx::query("ALTER TABLE app_settings ADD COLUMN window_x INTEGER").execute(&self.pool).await;
            let _ = sqlx::query("ALTER TABLE app_settings ADD COLUMN window_y INTEGER").execute(&self.pool).await;
            let _ = sqlx::query("ALTER TABLE app_settings ADD COLUMN window_width INTEGER").execute(&self.pool).await;
            let _ = sqlx::query("ALTER TABLE app_settings ADD COLUMN window_height INTEGER").execute(&self.pool).await;
            let _ = sqlx::query("ALTER TABLE app_settings ADD COLUMN is_mini_mode INTEGER DEFAULT 0").execute(&self.pool).await;

            // Migrate: add position column for custom playlist track ordering
            let _ = sqlx::query("ALTER TABLE playlist_memberships ADD COLUMN position INTEGER").execute(&self.pool).await;
            self.backfill_membership_positions().await?;

            // Migrate: add a random anonymous install id for analytics -- generated
            // once and persisted, never tied to any account or content.
            let _ = sqlx::query("ALTER TABLE app_settings ADD COLUMN analytics_id TEXT").execute(&self.pool).await;

            // Migrate: track which version's "what's new" the user has already
            // seen, so an auto-update to a new version can show it once without
            // showing anything on a fresh install (see save/load_last_seen_version).
            let _ = sqlx::query("ALTER TABLE app_settings ADD COLUMN last_seen_version TEXT").execute(&self.pool).await;

            // The column above is new, so anyone upgrading through this exact
            // migration has never had a chance to write to it -- their read
            // back is NULL, identical to what a brand new install looks like,
            // so "what's new" would never show for them even though they
            // really did just upgrade. Backfill a sentinel (guaranteed to
            // differ from any real version string) for rows that already
            // existed before this migration, so the *next* release's
            // "what's new" correctly detects them as upgraders. Rows that
            // didn't exist yet (a genuinely fresh install) are left NULL, as
            // originally intended.
            if had_existing_settings_row > 0 {
                let _ = sqlx::query(
                    "UPDATE app_settings SET last_seen_version = COALESCE(last_seen_version, '0.0.0')"
                ).execute(&self.pool).await;
            }

            // Migrate: a track could be added to the same playlist more than
            // once, showing up twice and inflating track_count. Collapse any
            // existing duplicates (keeping the earliest membership) before
            // adding the constraint that prevents new ones.
            sqlx::query(
                r#"
                DELETE FROM playlist_memberships
                WHERE rowid NOT IN (
                    SELECT MIN(rowid) FROM playlist_memberships
                    GROUP BY playlist_id, track_id
                )
                "#,
            )
            .execute(&self.pool)
            .await?;

            sqlx::query(
                "CREATE UNIQUE INDEX IF NOT EXISTS idx_playlist_memberships_unique
                 ON playlist_memberships (playlist_id, track_id)",
            )
            .execute(&self.pool)
            .await?;

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
            let now = unix_timestamp();
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

    pub async fn create_playlist(&self, name: &str) -> Result<String, sqlx::Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = unix_timestamp();

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
        let now = unix_timestamp();

        let next_position: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM playlist_memberships WHERE playlist_id = ?"
        )
        .bind(playlist_id)
        .fetch_one(&self.pool)
        .await?;

        // Idempotent: adding a track that's already in the playlist is a no-op
        // rather than a second row. Backed by the unique index added in the
        // schema v3 migration.
        sqlx::query(
            "INSERT INTO playlist_memberships (id, playlist_id, track_id, added_date, is_favorite, position) VALUES (?, ?, ?, ?, 0, ?)
             ON CONFLICT(playlist_id, track_id) DO NOTHING"
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

    // One grouped query instead of the caller fetching every playlist's full track
    // list just to read its length (an N+1 pattern that used to run on every
    // Playlists-tab load and every "add to playlist" modal open).
    pub async fn get_all_playlists_with_counts(&self) -> Result<Vec<PlaylistWithCount>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT p.id, p.name, p.created_date, p.is_system_playlist, COUNT(pm.track_id) as track_count
            FROM playlists p
            LEFT JOIN playlist_memberships pm ON pm.playlist_id = p.id
            GROUP BY p.id
            ORDER BY p.is_system_playlist DESC, p.created_date ASC
            "#
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| PlaylistWithCount {
                id: r.get("id"),
                name: r.get("name"),
                created_date: r.get("created_date"),
                is_system_playlist: r.get("is_system_playlist"),
                track_count: r.get("track_count"),
            })
            .collect())
    }

    // Which playlists already contain a given track, in one query -- used by the
    // "add to playlist" modal instead of fetching every playlist's full track list
    // to check membership one at a time.
    pub async fn get_playlist_ids_containing_track(&self, track_id: &str) -> Result<Vec<String>, sqlx::Error> {
        sqlx::query_scalar("SELECT playlist_id FROM playlist_memberships WHERE track_id = ?")
            .bind(track_id)
            .fetch_all(&self.pool)
            .await
    }

    pub async fn save_settings(&self, settings: &AppSettings) -> Result<(), sqlx::Error> {
        // Must be ON CONFLICT DO UPDATE, not INSERT OR REPLACE: the latter
        // deletes the existing row and inserts a fresh one, so every column not
        // named here (window_x/y/width/height, is_mini_mode, analytics_id) is
        // reset to NULL. Changing the download folder or audio quality would
        // therefore throw away the saved window geometry and mini-mode, and
        // re-roll the analytics install id as if it were a new install.
        sqlx::query(
            r#"
            INSERT INTO app_settings (id, default_download_path, preferred_audio_quality, auto_update_ytdlp)
            VALUES ('default', ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                default_download_path = excluded.default_download_path,
                preferred_audio_quality = excluded.preferred_audio_quality,
                auto_update_ytdlp = excluded.auto_update_ytdlp
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

    pub async fn save_last_seen_version(&self, version: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO app_settings (id, last_seen_version)
            VALUES ('default', ?)
            ON CONFLICT(id) DO UPDATE SET last_seen_version = excluded.last_seen_version
            "#
        )
        .bind(version)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // None means "never recorded" -- a fresh install, not a version bump -- so
    // the caller can tell that case apart from an upgrade and skip showing
    // "what's new" for it.
    pub async fn load_last_seen_version(&self) -> Result<Option<String>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT last_seen_version FROM app_settings WHERE id = 'default'"
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.and_then(|r| r.get("last_seen_version")))
    }

    // Generates a random anonymous id on first call and persists it -- an atomic
    // "set only if not already set" upsert, so a race between two calls (there
    // shouldn't be any, but this is cheap insurance) can't produce two different
    // ids depending on which one ran last.
    pub async fn get_or_create_analytics_id(&self) -> Result<String, sqlx::Error> {
        let candidate_id = uuid::Uuid::new_v4().to_string();

        sqlx::query(
            r#"
            INSERT INTO app_settings (id, analytics_id)
            VALUES ('default', ?)
            ON CONFLICT(id) DO UPDATE SET
                analytics_id = COALESCE(app_settings.analytics_id, excluded.analytics_id)
            "#
        )
        .bind(&candidate_id)
        .execute(&self.pool)
        .await?;

        sqlx::query_scalar("SELECT analytics_id FROM app_settings WHERE id = 'default'")
            .fetch_one(&self.pool)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: &str) -> Track {
        Track {
            id: id.to_string(),
            title: format!("Title {}", id),
            author: Some("Author".to_string()),
            duration: 120,
            thumbnail_url: None,
            added_date: unix_timestamp(),
            file_path: None,
        }
    }

    #[tokio::test]
    async fn init_creates_the_system_favorites_playlist() {
        let db = DatabaseManager::new_in_memory().await.unwrap();
        let playlists = db.get_all_playlists().await.unwrap();

        assert_eq!(playlists.len(), 1);
        assert_eq!(playlists[0].id, "favorites");
        assert_eq!(playlists[0].name, "All Favorites");
        assert!(playlists[0].is_system_playlist);
    }

    #[tokio::test]
    async fn init_is_idempotent_and_does_not_duplicate_the_system_playlist() {
        let db = DatabaseManager::new_in_memory().await.unwrap();
        // Simulates a second startup against the same (already-initialized) DB.
        db.init_database().await.unwrap();

        let playlists = db.get_all_playlists().await.unwrap();
        assert_eq!(playlists.len(), 1);
    }

    #[tokio::test]
    async fn create_playlist_returns_a_usable_id_and_appears_in_get_all_playlists() {
        let db = DatabaseManager::new_in_memory().await.unwrap();
        let id = db.create_playlist("My Mix").await.unwrap();

        let playlists = db.get_all_playlists().await.unwrap();
        let created = playlists.iter().find(|p| p.id == id).unwrap();
        assert_eq!(created.name, "My Mix");
        assert!(!created.is_system_playlist);
    }

    #[tokio::test]
    async fn get_all_playlists_orders_system_playlists_first() {
        let db = DatabaseManager::new_in_memory().await.unwrap();
        db.create_playlist("User Playlist").await.unwrap();

        let playlists = db.get_all_playlists().await.unwrap();
        assert!(playlists[0].is_system_playlist);
    }

    #[tokio::test]
    async fn delete_playlist_removes_a_user_playlist() {
        let db = DatabaseManager::new_in_memory().await.unwrap();
        let id = db.create_playlist("Temp").await.unwrap();

        db.delete_playlist(&id).await.unwrap();

        let playlists = db.get_all_playlists().await.unwrap();
        assert!(playlists.iter().all(|p| p.id != id));
    }

    #[tokio::test]
    async fn delete_playlist_refuses_to_delete_the_system_playlist() {
        let db = DatabaseManager::new_in_memory().await.unwrap();
        db.delete_playlist("favorites").await.unwrap();

        let playlists = db.get_all_playlists().await.unwrap();
        assert!(playlists.iter().any(|p| p.id == "favorites"));
    }

    #[tokio::test]
    async fn update_playlist_name_renames_a_user_playlist() {
        let db = DatabaseManager::new_in_memory().await.unwrap();
        let id = db.create_playlist("Old Name").await.unwrap();

        db.update_playlist_name(&id, "New Name").await.unwrap();

        let playlists = db.get_all_playlists().await.unwrap();
        assert_eq!(playlists.iter().find(|p| p.id == id).unwrap().name, "New Name");
    }

    #[tokio::test]
    async fn update_playlist_name_trims_whitespace() {
        let db = DatabaseManager::new_in_memory().await.unwrap();
        let id = db.create_playlist("Old Name").await.unwrap();

        db.update_playlist_name(&id, "  Padded  ").await.unwrap();

        let playlists = db.get_all_playlists().await.unwrap();
        assert_eq!(playlists.iter().find(|p| p.id == id).unwrap().name, "Padded");
    }

    #[tokio::test]
    async fn update_playlist_name_ignores_a_blank_name() {
        let db = DatabaseManager::new_in_memory().await.unwrap();
        let id = db.create_playlist("Keep Me").await.unwrap();

        db.update_playlist_name(&id, "   ").await.unwrap();

        let playlists = db.get_all_playlists().await.unwrap();
        assert_eq!(playlists.iter().find(|p| p.id == id).unwrap().name, "Keep Me");
    }

    #[tokio::test]
    async fn update_playlist_name_refuses_to_rename_the_system_playlist() {
        let db = DatabaseManager::new_in_memory().await.unwrap();
        db.update_playlist_name("favorites", "Renamed").await.unwrap();

        let playlists = db.get_all_playlists().await.unwrap();
        assert_eq!(
            playlists.iter().find(|p| p.id == "favorites").unwrap().name,
            "All Favorites"
        );
    }

    #[tokio::test]
    async fn add_track_to_playlist_makes_it_show_up_in_get_playlist_tracks() {
        let db = DatabaseManager::new_in_memory().await.unwrap();
        let playlist_id = db.create_playlist("Mix").await.unwrap();
        db.save_track(&track("t1")).await.unwrap();

        db.add_track_to_playlist("t1", &playlist_id).await.unwrap();

        let tracks = db.get_playlist_tracks(&playlist_id).await.unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].id, "t1");
    }

    #[tokio::test]
    async fn add_track_to_playlist_assigns_increasing_positions_in_insertion_order() {
        let db = DatabaseManager::new_in_memory().await.unwrap();
        let playlist_id = db.create_playlist("Mix").await.unwrap();
        for id in ["t1", "t2", "t3"] {
            db.save_track(&track(id)).await.unwrap();
            db.add_track_to_playlist(id, &playlist_id).await.unwrap();
        }

        let tracks = db.get_playlist_tracks(&playlist_id).await.unwrap();
        let ids: Vec<&str> = tracks.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec!["t1", "t2", "t3"]);
    }

    #[tokio::test]
    async fn reorder_playlist_tracks_changes_the_returned_order() {
        let db = DatabaseManager::new_in_memory().await.unwrap();
        let playlist_id = db.create_playlist("Mix").await.unwrap();
        for id in ["t1", "t2", "t3"] {
            db.save_track(&track(id)).await.unwrap();
            db.add_track_to_playlist(id, &playlist_id).await.unwrap();
        }

        db.reorder_playlist_tracks(
            &playlist_id,
            &["t3".to_string(), "t1".to_string(), "t2".to_string()],
        )
        .await
        .unwrap();

        let tracks = db.get_playlist_tracks(&playlist_id).await.unwrap();
        let ids: Vec<&str> = tracks.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec!["t3", "t1", "t2"]);
    }

    #[tokio::test]
    async fn remove_track_from_playlist_removes_only_that_membership() {
        let db = DatabaseManager::new_in_memory().await.unwrap();
        let playlist_id = db.create_playlist("Mix").await.unwrap();
        for id in ["t1", "t2"] {
            db.save_track(&track(id)).await.unwrap();
            db.add_track_to_playlist(id, &playlist_id).await.unwrap();
        }

        db.remove_track_from_playlist("t1", &playlist_id).await.unwrap();

        let tracks = db.get_playlist_tracks(&playlist_id).await.unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].id, "t2");
    }

    #[tokio::test]
    async fn add_to_favorites_and_remove_from_favorites_round_trip() {
        let db = DatabaseManager::new_in_memory().await.unwrap();
        db.save_track(&track("t1")).await.unwrap();

        db.add_to_favorites("t1").await.unwrap();
        let favorites = db.get_playlist_tracks("favorites").await.unwrap();
        assert_eq!(favorites.len(), 1);

        db.remove_from_favorites("t1").await.unwrap();
        let favorites = db.get_playlist_tracks("favorites").await.unwrap();
        assert!(favorites.is_empty());
    }

    #[tokio::test]
    async fn get_all_playlists_with_counts_matches_actual_track_counts() {
        let db = DatabaseManager::new_in_memory().await.unwrap();
        let playlist_id = db.create_playlist("Mix").await.unwrap();
        db.save_track(&track("t1")).await.unwrap();
        db.save_track(&track("t2")).await.unwrap();
        db.add_track_to_playlist("t1", &playlist_id).await.unwrap();
        db.add_track_to_playlist("t2", &playlist_id).await.unwrap();

        let counts = db.get_all_playlists_with_counts().await.unwrap();
        let mix = counts.iter().find(|p| p.id == playlist_id).unwrap();
        assert_eq!(mix.track_count, 2);

        // The system playlist has no tracks yet -- the LEFT JOIN must still
        // return it with a count of 0, not silently drop it.
        let favorites = counts.iter().find(|p| p.id == "favorites").unwrap();
        assert_eq!(favorites.track_count, 0);
    }

    #[tokio::test]
    async fn get_playlist_ids_containing_track_reflects_membership_across_playlists() {
        let db = DatabaseManager::new_in_memory().await.unwrap();
        let p1 = db.create_playlist("P1").await.unwrap();
        let p2 = db.create_playlist("P2").await.unwrap();
        db.save_track(&track("t1")).await.unwrap();
        db.add_track_to_playlist("t1", &p1).await.unwrap();
        db.add_track_to_playlist("t1", &p2).await.unwrap();

        let mut ids = db.get_playlist_ids_containing_track("t1").await.unwrap();
        ids.sort();
        let mut expected = vec![p1, p2];
        expected.sort();
        assert_eq!(ids, expected);

        assert!(db
            .get_playlist_ids_containing_track("nonexistent")
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn load_settings_returns_defaults_when_nothing_saved() {
        let db = DatabaseManager::new_in_memory().await.unwrap();
        let settings = db.load_settings().await.unwrap();
        assert_eq!(settings.default_download_path, "");
        assert_eq!(settings.preferred_audio_quality, "best");
    }

    #[tokio::test]
    async fn save_settings_then_load_settings_round_trips() {
        let db = DatabaseManager::new_in_memory().await.unwrap();
        let settings = AppSettings {
            default_download_path: "/tmp/downloads".to_string(),
            preferred_audio_quality: "320".to_string(),
            auto_update_ytdlp: false,
        };
        db.save_settings(&settings).await.unwrap();

        let loaded = db.load_settings().await.unwrap();
        assert_eq!(loaded.default_download_path, "/tmp/downloads");
        assert_eq!(loaded.preferred_audio_quality, "320");
        assert!(!loaded.auto_update_ytdlp);
    }

    #[tokio::test]
    async fn load_window_geometry_is_none_when_never_saved() {
        let db = DatabaseManager::new_in_memory().await.unwrap();
        assert!(db.load_window_geometry().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn save_window_geometry_then_load_round_trips() {
        let db = DatabaseManager::new_in_memory().await.unwrap();
        db.save_window_geometry(10, 20, 380, 500).await.unwrap();

        let (x, y, w, h) = db.load_window_geometry().await.unwrap().unwrap();
        assert_eq!((x, y, w, h), (10, 20, 380, 500));
    }

    #[tokio::test]
    async fn save_window_geometry_overwrites_the_previous_value() {
        let db = DatabaseManager::new_in_memory().await.unwrap();
        db.save_window_geometry(10, 20, 380, 500).await.unwrap();
        db.save_window_geometry(99, 88, 400, 600).await.unwrap();

        let (x, y, w, h) = db.load_window_geometry().await.unwrap().unwrap();
        assert_eq!((x, y, w, h), (99, 88, 400, 600));
    }

    #[tokio::test]
    async fn mini_mode_defaults_to_false_and_round_trips() {
        let db = DatabaseManager::new_in_memory().await.unwrap();
        assert!(!db.load_mini_mode().await.unwrap());

        db.save_mini_mode(true).await.unwrap();
        assert!(db.load_mini_mode().await.unwrap());

        db.save_mini_mode(false).await.unwrap();
        assert!(!db.load_mini_mode().await.unwrap());
    }

    #[tokio::test]
    async fn get_or_create_analytics_id_is_stable_across_calls() {
        let db = DatabaseManager::new_in_memory().await.unwrap();
        let first = db.get_or_create_analytics_id().await.unwrap();
        let second = db.get_or_create_analytics_id().await.unwrap();
        assert_eq!(first, second);
        assert!(!first.is_empty());
    }

    #[tokio::test]
    async fn save_track_is_idempotent_via_insert_or_ignore() {
        let db = DatabaseManager::new_in_memory().await.unwrap();
        let t = track("t1");
        db.save_track(&t).await.unwrap();
        // Saving the same id again must not error (INSERT OR IGNORE) and must
        // not disturb existing playlist memberships for it.
        db.save_track(&t).await.unwrap();

        let playlist_id = db.create_playlist("Mix").await.unwrap();
        db.add_track_to_playlist("t1", &playlist_id).await.unwrap();
        db.save_track(&t).await.unwrap();

        let tracks = db.get_playlist_tracks(&playlist_id).await.unwrap();
        assert_eq!(tracks.len(), 1);
    }

    // save_settings used to be an INSERT OR REPLACE naming only its own three
    // columns, which deletes the row and re-inserts it -- silently clearing
    // every other column on the shared 'default' row.

    #[tokio::test]
    async fn save_settings_preserves_window_geometry() {
        let db = DatabaseManager::new_in_memory().await.unwrap();
        db.save_window_geometry(10, 20, 380, 500).await.unwrap();

        db.save_settings(&AppSettings {
            default_download_path: "/tmp/music".to_string(),
            preferred_audio_quality: "best".to_string(),
            auto_update_ytdlp: true,
        })
        .await
        .unwrap();

        assert_eq!(
            db.load_window_geometry().await.unwrap(),
            Some((10, 20, 380, 500))
        );
    }

    #[tokio::test]
    async fn save_settings_preserves_mini_mode_and_analytics_id() {
        let db = DatabaseManager::new_in_memory().await.unwrap();
        db.save_mini_mode(true).await.unwrap();
        let analytics_id = db.get_or_create_analytics_id().await.unwrap();

        db.save_settings(&AppSettings {
            default_download_path: "/tmp/music".to_string(),
            preferred_audio_quality: "worst".to_string(),
            auto_update_ytdlp: false,
        })
        .await
        .unwrap();

        assert!(db.load_mini_mode().await.unwrap());
        // A cleared id would silently re-register the install as a new one.
        assert_eq!(
            db.get_or_create_analytics_id().await.unwrap(),
            analytics_id
        );
    }

    #[tokio::test]
    async fn save_settings_round_trips_its_own_values() {
        let db = DatabaseManager::new_in_memory().await.unwrap();
        let settings = AppSettings {
            default_download_path: "/tmp/music".to_string(),
            preferred_audio_quality: "worst".to_string(),
            auto_update_ytdlp: false,
        };

        db.save_settings(&settings).await.unwrap();

        let loaded = db.load_settings().await.unwrap();
        assert_eq!(loaded.default_download_path, "/tmp/music");
        assert_eq!(loaded.preferred_audio_quality, "worst");
        assert!(!loaded.auto_update_ytdlp);
    }

    #[tokio::test]
    async fn add_track_to_playlist_twice_does_not_duplicate_the_membership() {
        let db = DatabaseManager::new_in_memory().await.unwrap();
        db.save_track(&track("t1")).await.unwrap();
        let playlist_id = db.create_playlist("Mix").await.unwrap();

        db.add_track_to_playlist("t1", &playlist_id).await.unwrap();
        db.add_track_to_playlist("t1", &playlist_id).await.unwrap();

        let tracks = db.get_playlist_tracks(&playlist_id).await.unwrap();
        assert_eq!(tracks.len(), 1);

        let playlists = db.get_all_playlists_with_counts().await.unwrap();
        let mix = playlists.iter().find(|p| p.id == playlist_id).unwrap();
        assert_eq!(mix.track_count, 1);
    }

    #[tokio::test]
    async fn the_same_track_can_still_live_in_two_different_playlists() {
        // The uniqueness constraint is per (playlist, track) -- guard against
        // it being written as a bare unique index on track_id.
        let db = DatabaseManager::new_in_memory().await.unwrap();
        db.save_track(&track("t1")).await.unwrap();
        let first = db.create_playlist("First").await.unwrap();
        let second = db.create_playlist("Second").await.unwrap();

        db.add_track_to_playlist("t1", &first).await.unwrap();
        db.add_track_to_playlist("t1", &second).await.unwrap();

        assert_eq!(db.get_playlist_tracks(&first).await.unwrap().len(), 1);
        assert_eq!(db.get_playlist_tracks(&second).await.unwrap().len(), 1);
    }
}
