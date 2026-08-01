use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::sync::Mutex;
use std::sync::LazyLock;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use crate::command_utils::{command_no_window, unix_timestamp};

static INSTALL_LOCK: LazyLock<Arc<Mutex<()>>> = LazyLock::new(|| Arc::new(Mutex::new(())));

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
}

#[derive(Serialize, Deserialize)]
struct UpdateCheck {
    last_check: i64,
}

#[derive(Clone, Serialize)]
pub struct DepProgress {
    pub dependency: String,
    pub downloaded: u64,
    pub total: u64,
}

/// Whether `path` is a binary we can actually execute.
///
/// Existence alone isn't enough. A download interrupted before the chmod (app
/// quit, crash, machine sleep) leaves a file that exists but has no execute
/// bit. Since "is it installed?" gated every reinstall, such a file was treated
/// as a working install forever: the installer skipped it and every search
/// failed with "Permission denied", with no way out but deleting it by hand.
fn is_runnable_binary(path: &std::path::Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        match std::fs::metadata(path) {
            Ok(metadata) => metadata.is_file() && metadata.permissions().mode() & 0o111 != 0,
            Err(_) => false,
        }
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

const YTDLP_RELEASE_BASE: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download";

/// Pulls one asset's digest out of a `SHA2-256SUMS` listing.
///
/// Lines look like `<hex>  <filename>`, and the filename must match exactly --
/// a substring match would let `yt-dlp` collide with `yt-dlp_linux`.
fn parse_sha256sums(body: &str, asset_name: &str) -> Option<String> {
    body.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let digest = parts.next()?;
        // Some tools prefix the name with '*' to mark binary mode.
        let name = parts.next()?.trim_start_matches('*');
        (name == asset_name && digest.len() == 64).then(|| digest.to_string())
    })
}

pub struct YTDLPInstaller;

impl YTDLPInstaller {
    pub fn get_ytdlp_dir() -> PathBuf {
        let mut path = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."));
        path.push("ytaudiobar");
        path.push("bin");
        path
    }

    pub fn get_ytdlp_path() -> PathBuf {
        let mut path = Self::get_ytdlp_dir();

        #[cfg(target_os = "windows")]
        path.push("yt-dlp.exe");

        #[cfg(not(target_os = "windows"))]
        path.push("yt-dlp");

        path
    }

    pub async fn is_installed() -> bool {
        is_runnable_binary(&Self::get_ytdlp_path())
    }

    async fn download_with_progress(app_handle: &AppHandle) -> Result<(), String> {
        let ytdlp_dir = Self::get_ytdlp_dir();
        let ytdlp_path = Self::get_ytdlp_path();

        fs::create_dir_all(&ytdlp_dir)
            .await
            .map_err(|e| format!("Failed to create directory: {}", e))?;

        #[cfg(target_os = "windows")]
        let asset_name = "yt-dlp.exe";

        #[cfg(target_os = "linux")]
        let asset_name = {
            // Use standalone binary (same approach as macOS - no Python needed)
            println!("📥 Downloading yt-dlp standalone binary for Linux");
            "yt-dlp_linux"
        };

        #[cfg(target_os = "macos")]
        let asset_name = {
            // Standalone universal (x86_64 + arm64) binary, no Python needed.
            println!("📥 Downloading yt-dlp standalone binary for macOS");
            "yt-dlp_macos"
        };

        let download_url = format!("{}/{}", YTDLP_RELEASE_BASE, asset_name);
        let download_url = download_url.as_str();

        println!("📥 Downloading yt-dlp from: {}", download_url);

        // Download to a temp path and only move it into place once the transfer
        // is complete *and* verified. Writing straight to `ytdlp_path` means an
        // interrupted download leaves a truncated file behind, which then has to
        // be detected and repaired after the fact (see `is_runnable_binary` and
        // the reinstall in `check_and_update`). Renaming a finished file into
        // place avoids ever creating that state.
        //
        // The partial file is intentionally kept when a download fails so the
        // retry resumes from it rather than restarting -- on a slow link,
        // restarting a ~38MB download from zero can mean never completing.
        let temp_path = ytdlp_path.with_extension("download");

        let handle_for_progress = app_handle.clone();
        crate::downloader::download_resumable(download_url, &temp_path, move |downloaded, total| {
            let _ = handle_for_progress.emit("dep-progress", DepProgress {
                dependency: "ytdlp".to_string(),
                downloaded,
                total,
            });
        })
        .await
        .map_err(|e| format!("Failed to download yt-dlp: {}", e))?;

        // Verify before installing. A resumed download that spliced together
        // mismatched ranges, or a file corrupted in transit, would otherwise be
        // renamed into place and then fail at every use with a confusing error.
        if let Some(expected) = Self::fetch_expected_sha256(asset_name).await {
            if !crate::downloader::sha256_matches(&temp_path, &expected).await {
                let _ = fs::remove_file(&temp_path).await;
                return Err(
                    "Downloaded yt-dlp failed checksum verification; discarded".to_string()
                );
            }
            println!("✅ yt-dlp checksum verified");
        } else {
            // Upstream didn't publish sums we could read -- fall back to
            // confirming the file isn't obviously truncated.
            let len = fs::metadata(&temp_path).await.map(|m| m.len()).unwrap_or(0);
            if len < 1_000_000 {
                let _ = fs::remove_file(&temp_path).await;
                return Err(format!("Downloaded yt-dlp is implausibly small ({} bytes)", len));
            }
            println!("⚠️ Could not fetch yt-dlp checksums; accepted on size ({} bytes)", len);
        }

        fs::rename(&temp_path, &ytdlp_path)
            .await
            .map_err(|e| format!("Failed to install downloaded yt-dlp: {}", e))?;

        // Make executable on Linux
        #[cfg(not(target_os = "windows"))]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&ytdlp_path)
                .map_err(|e| format!("Failed to get file metadata: {}", e))?
                .permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&ytdlp_path, perms)
                .map_err(|e| format!("Failed to set permissions: {}", e))?;
        }

        // yt-dlp's macOS binary isn't signed/notarized either -- ad-hoc sign it
        // so Apple Silicon's Gatekeeper/AMFI policy will actually execute it.
        // Doesn't require an Apple Developer account, works fully offline.
        #[cfg(target_os = "macos")]
        {
            let sign_result = std::process::Command::new("codesign")
                .args(["--sign", "-", "--force", "--"])
                .arg(&ytdlp_path)
                .output();
            match sign_result {
                Ok(output) if output.status.success() => {
                    println!("✅ ad-hoc signed yt-dlp for local execution");
                }
                Ok(output) => {
                    eprintln!(
                        "⚠️ ad-hoc signing yt-dlp failed: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
                Err(e) => {
                    eprintln!("⚠️ could not run codesign on yt-dlp: {}", e);
                }
            }
        }

        println!("✅ yt-dlp installed at: {}", ytdlp_path.display());
        Ok(())
    }

    /// Looks up the published SHA-256 for a release asset.
    ///
    /// yt-dlp ships a `SHA2-256SUMS` file alongside its binaries, in the usual
    /// `<hex>  <filename>` format. Returns `None` if it can't be fetched or the
    /// asset isn't listed -- verification is then skipped rather than blocking
    /// the install, since a missing sums file shouldn't make the app unusable.
    async fn fetch_expected_sha256(asset_name: &str) -> Option<String> {
        let url = format!("{}/SHA2-256SUMS", YTDLP_RELEASE_BASE);
        let body = reqwest::get(&url).await.ok()?.text().await.ok()?;
        parse_sha256sums(&body, asset_name)
    }

    /// Downloads yt-dlp if it isn't already present. No-op when it is.
    pub async fn install(app_handle: &AppHandle) -> Result<(), String> {
        Self::install_inner(app_handle, false).await
    }

    /// Downloads yt-dlp even if a copy is already present, replacing it.
    ///
    /// The updater needs this: `install()` short-circuits on "already
    /// installed", so using it to apply an update silently did nothing at all
    /// while still reporting success.
    pub async fn reinstall(app_handle: &AppHandle) -> Result<(), String> {
        Self::install_inner(app_handle, true).await
    }

    async fn install_inner(app_handle: &AppHandle, force: bool) -> Result<(), String> {
        // Held for the whole download so a second caller waits for the first to
        // finish instead of racing it to write the same file. (The previous
        // poll-until-the-file-exists approach couldn't express "wait for a
        // *replacement* to finish" -- for an update the file already exists, so
        // the waiter returned immediately and saw the old binary.)
        let _guard = INSTALL_LOCK.lock().await;

        if !force && Self::is_installed().await {
            return Ok(());
        }

        Self::download_with_progress(app_handle).await
    }

    pub async fn get_version() -> Result<String, String> {
        let ytdlp_path = Self::get_ytdlp_path();

        if !ytdlp_path.exists() {
            return Err("yt-dlp not installed".to_string());
        }

        let output = command_no_window(ytdlp_path.to_str().unwrap_or("yt-dlp"))
            .arg("--version")
            .output()
            .await
            .map_err(|e| format!("Failed to get version: {}", e))?;

        if !output.status.success() {
            return Err("Failed to get yt-dlp version".to_string());
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn get_update_check_file() -> PathBuf {
        let mut path = Self::get_ytdlp_dir();
        path.push("last_update_check.json");
        path
    }

    async fn get_last_update_check() -> Option<i64> {
        let check_file = Self::get_update_check_file();
        if !check_file.exists() {
            return None;
        }

        let content = fs::read_to_string(&check_file).await.ok()?;
        let check: UpdateCheck = serde_json::from_str(&content).ok()?;
        Some(check.last_check)
    }

    async fn save_update_check() -> Result<(), String> {
        let check_file = Self::get_update_check_file();
        let check = UpdateCheck {
            last_check: unix_timestamp(),
        };
        let content = serde_json::to_string(&check)
            .map_err(|e| format!("Failed to serialize update check: {}", e))?;
        fs::write(&check_file, content)
            .await
            .map_err(|e| format!("Failed to write update check: {}", e))
    }

    pub async fn should_check_for_update() -> bool {
        match Self::get_last_update_check().await {
            Some(last_check) => {
                let now = unix_timestamp();
                let hours_since_check = (now - last_check) / 3600;
                hours_since_check >= 24
            }
            None => true,
        }
    }

    pub async fn fetch_latest_version() -> Result<String, String> {
        let url = "https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest";

        let client = reqwest::Client::new();
        let response = client
            .get(url)
            .header("User-Agent", "YTAudioBar")
            .send()
            .await
            .map_err(|e| format!("Failed to fetch latest version: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("GitHub API error: HTTP {}", response.status()));
        }

        let release: GitHubRelease = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        Ok(release.tag_name)
    }

    pub async fn check_and_update(app_handle: &AppHandle) -> Result<Option<String>, String> {
        if !Self::is_installed().await {
            return Err("yt-dlp not installed".to_string());
        }

        if !Self::should_check_for_update().await {
            return Ok(None);
        }

        println!("🔍 Checking for yt-dlp updates...");

        // A binary that won't report its version is broken, not merely out of
        // date -- a truncated download will happily pass the "is it installed?"
        // check but fail to execute. Replacing it here is the difference
        // between the app repairing itself on the next launch and every search
        // failing until the user manually deletes the file.
        let current_version = match Self::get_version().await {
            Ok(version) => version,
            Err(e) => {
                println!("⚠️ yt-dlp present but unusable ({}), reinstalling", e);
                Self::reinstall(app_handle).await?;
                let repaired = Self::get_version().await?;
                let _ = Self::save_update_check().await;
                println!("✅ yt-dlp repaired ({})", repaired);
                return Ok(Some(repaired));
            }
        };
        let latest_version = Self::fetch_latest_version().await?;

        if current_version == latest_version {
            println!("✅ yt-dlp is up to date ({})", current_version);
            let _ = Self::save_update_check().await;
            return Ok(None);
        }

        println!("📦 Updating yt-dlp: {} → {}", current_version, latest_version);
        // Must be reinstall(), not install() -- yt-dlp is already present here
        // by definition, so install() would return Ok without downloading and
        // we'd report a successful update that never happened.
        Self::reinstall(app_handle).await?;

        // Only recorded once the update actually landed. Stamping it before the
        // download meant a failed update was suppressed for another 24h, so a
        // broken yt-dlp could stay broken while the app kept reporting success.
        let _ = Self::save_update_check().await;

        println!("✅ yt-dlp updated to {}", latest_version);
        Ok(Some(latest_version))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_is_not_runnable() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_runnable_binary(&dir.path().join("yt-dlp")));
    }

    #[test]
    fn a_directory_is_not_runnable() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_runnable_binary(dir.path()));
    }

    #[cfg(unix)]
    #[test]
    fn a_file_without_the_execute_bit_is_not_runnable() {
        use std::os::unix::fs::PermissionsExt;

        // Exactly the state an interrupted download leaves behind: the file is
        // present, so an existence check would call it installed.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("yt-dlp");
        std::fs::write(&path, b"partial").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

        assert!(path.exists(), "precondition: the file is present");
        assert!(!is_runnable_binary(&path));
    }

    #[cfg(unix)]
    #[test]
    fn a_file_with_the_execute_bit_is_runnable() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("yt-dlp");
        std::fs::write(&path, b"binary").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();

        assert!(is_runnable_binary(&path));
    }
}

#[cfg(test)]
mod sha256sums_tests {
    use super::parse_sha256sums;

    const SUMS: &str = "\
1111111111111111111111111111111111111111111111111111111111111111  yt-dlp
2222222222222222222222222222222222222222222222222222222222222222  yt-dlp_linux
3333333333333333333333333333333333333333333333333333333333333333  yt-dlp.exe
";

    #[test]
    fn picks_the_digest_for_the_requested_asset() {
        assert_eq!(
            parse_sha256sums(SUMS, "yt-dlp_linux").as_deref(),
            Some("2222222222222222222222222222222222222222222222222222222222222222")
        );
        assert_eq!(
            parse_sha256sums(SUMS, "yt-dlp.exe").as_deref(),
            Some("3333333333333333333333333333333333333333333333333333333333333333")
        );
    }

    #[test]
    fn matches_the_filename_exactly_rather_than_by_prefix() {
        // "yt-dlp" is a prefix of "yt-dlp_linux"; a sloppy match would return
        // the wrong digest and reject a perfectly good download.
        assert_eq!(
            parse_sha256sums(SUMS, "yt-dlp").as_deref(),
            Some("1111111111111111111111111111111111111111111111111111111111111111")
        );
    }

    #[test]
    fn tolerates_the_binary_mode_star_prefix() {
        let sums = "4444444444444444444444444444444444444444444444444444444444444444 *yt-dlp_linux";
        assert_eq!(
            parse_sha256sums(sums, "yt-dlp_linux").as_deref(),
            Some("4444444444444444444444444444444444444444444444444444444444444444")
        );
    }

    #[test]
    fn returns_none_for_an_asset_that_is_not_listed() {
        assert!(parse_sha256sums(SUMS, "yt-dlp_macos").is_none());
    }

    #[test]
    fn ignores_lines_whose_digest_is_not_a_sha256() {
        // Guards against parsing a header or a stray note as a digest.
        let sums = "# SHA2-256SUMS for release\nabc  yt-dlp_linux";
        assert!(parse_sha256sums(sums, "yt-dlp_linux").is_none());
    }

    #[test]
    fn returns_none_for_empty_input() {
        assert!(parse_sha256sums("", "yt-dlp_linux").is_none());
    }
}
