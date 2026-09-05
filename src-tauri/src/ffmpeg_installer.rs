use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::sync::Mutex;
use std::sync::LazyLock;
use serde_json::json;
use tauri::{AppHandle, Emitter};
use crate::analytics::{truncate_for_analytics, Analytics};
use crate::ytdlp_installer::DepProgress;

static INSTALL_LOCK: LazyLock<Arc<Mutex<bool>>> = LazyLock::new(|| Arc::new(Mutex::new(false)));

// Bump this tag whenever the Linux ffmpeg source or extraction logic changes, to
// force existing users to redownload the corrected binary. The installer normally
// skips the download when the binary already exists; the marker makes it detect
// stale binaries from older app versions.
#[cfg(target_os = "linux")]
const FFMPEG_BUILD_ID: &str = "btbn-lgpl-r2";

pub struct FfmpegInstaller;

impl FfmpegInstaller {
    pub fn get_ffmpeg_dir() -> PathBuf {
        let mut path = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."));
        path.push("ytaudiobar");
        path.push("bin");
        path
    }

    pub fn get_ffmpeg_path() -> PathBuf {
        let mut path = Self::get_ffmpeg_dir();

        #[cfg(target_os = "windows")]
        path.push("ffmpeg.exe");

        #[cfg(not(target_os = "windows"))]
        path.push("ffmpeg");

        path
    }

    #[cfg(target_os = "linux")]
    fn get_buildid_marker_path() -> PathBuf {
        Self::get_ffmpeg_dir().join("ffmpeg.buildid")
    }

    pub async fn is_local_ffmpeg_installed() -> bool {
        let path = Self::get_ffmpeg_path();
        if !path.exists() {
            return false;
        }

        // On Linux, verify the build-id marker matches what this version of the
        // app expects. A missing or outdated marker means the binary was downloaded
        // by an older build and may be incompatible — delete it so the installer
        // fetches a fresh one.
        #[cfg(target_os = "linux")]
        {
            let marker = Self::get_buildid_marker_path();
            let ok = fs::read_to_string(&marker)
                .await
                .map(|s| s.trim() == FFMPEG_BUILD_ID)
                .unwrap_or(false);
            if !ok {
                eprintln!("⚠️ ffmpeg build-id mismatch or missing — scheduling redownload");
                let _ = fs::remove_file(&path).await;
                return false;
            }
        }

        true
    }

    /// Check if our local ffmpeg is available
    pub async fn is_available() -> bool {
        Self::is_local_ffmpeg_installed().await
    }

    async fn download_with_progress(app_handle: &AppHandle) -> Result<(), String> {
        let ffmpeg_dir = Self::get_ffmpeg_dir();

        fs::create_dir_all(&ffmpeg_dir)
            .await
            .map_err(|e| format!("Failed to create directory: {}", e))?;

        #[cfg(target_os = "windows")]
        let download_url = "https://github.com/ffbinaries/ffbinaries-prebuilt/releases/download/v6.1/ffmpeg-6.1-win-64.zip";

        // BtbN static build: SSL compiled in statically so ffmpeg can open
        // HTTPS YouTube CDN URLs. The ffbinaries Linux build is dynamically
        // linked and ships without SSL support — any https:// input silently
        // produces no audio, which is why streaming always failed on Linux
        // while yt-dlp downloads (which never hand a URL to ffmpeg) still
        // worked fine.
        #[cfg(target_os = "linux")]
        let download_url = "https://github.com/BtbN/FFmpeg-Builds/releases/latest/download/ffmpeg-master-latest-linux64-lgpl.tar.xz";

        // ffbinaries has no macOS arm64 build, so we use osxexperts.net instead,
        // which publishes separate per-architecture static builds (not a
        // universal/fat binary) -- pick the one matching the actual CPU rather
        // than always grabbing Intel and relying on Rosetta 2.
        #[cfg(target_os = "macos")]
        let download_url = if std::env::consts::ARCH == "aarch64" {
            "https://www.osxexperts.net/ffmpeg81arm.zip"
        } else {
            "https://www.osxexperts.net/ffmpeg80intel.zip"
        };

        println!("📥 Downloading ffmpeg from: {}", download_url);

        // Linux uses .tar.xz (BtbN); every other platform uses .zip.
        #[cfg(target_os = "linux")]
        let temp_zip = ffmpeg_dir.join("ffmpeg_temp.tar.xz");
        #[cfg(not(target_os = "linux"))]
        let temp_zip = ffmpeg_dir.join("ffmpeg_temp.zip");

        // Resumable: a dropped connection part-way through keeps its progress
        // and continues, instead of restarting a ~28MB transfer from zero.
        let handle_for_progress = app_handle.clone();
        crate::downloader::download_resumable(download_url, &temp_zip, move |downloaded, total| {
            let _ = handle_for_progress.emit("dep-progress", DepProgress {
                dependency: "ffmpeg".to_string(),
                downloaded,
                total,
            });
        })
        .await
        .map_err(|e| {
            // A resumed archive that ends up malformed would fail to extract on
            // every subsequent attempt, so don't leave a bad one to resume from.
            format!("Failed to download ffmpeg: {}", e)
        })?;

        // Linux: extract from .tar.xz (BtbN static build).
        #[cfg(target_os = "linux")]
        {
            let temp_zip_clone = temp_zip.clone();
            let outpath = Self::get_ffmpeg_path();
            tokio::task::spawn_blocking(move || {
                Self::extract_tar_xz_ffmpeg(&temp_zip_clone, &outpath)
            })
            .await
            .map_err(|e| format!("Extraction task failed: {}", e))
            .and_then(|inner| inner)
            .inspect_err(|_| { let _ = std::fs::remove_file(&temp_zip); })?;

            let _ = fs::remove_file(&temp_zip).await;
        }

        // Windows / macOS: extract from .zip.
        #[cfg(not(target_os = "linux"))]
        {
        let temp_zip_clone = temp_zip.clone();
        tokio::task::spawn_blocking(move || {
            let file = std::fs::File::open(&temp_zip_clone)
                .map_err(|e| format!("Failed to open zip: {}", e))?;

            let mut archive = zip::ZipArchive::new(file)
                .map_err(|e| format!("Failed to read zip: {}", e))?;

            #[cfg(target_os = "windows")]
            let binary_name = "ffmpeg.exe";

            #[cfg(not(target_os = "windows"))]
            let binary_name = "ffmpeg";

            for i in 0..archive.len() {
                let mut file = archive.by_index(i)
                    .map_err(|e| format!("Failed to access zip entry: {}", e))?;

                // Exact basename match, not just ends_with -- macOS zips from
                // osxexperts.net also contain a "__MACOSX/._ffmpeg" resource-fork
                // junk entry, which "ends_with(binary_name)" would also match.
                let is_target = file
                    .name()
                    .rsplit('/')
                    .next()
                    .map(|basename| basename == binary_name)
                    .unwrap_or(false);

                if is_target {
                    let outpath = Self::get_ffmpeg_path();
                    let mut outfile = std::fs::File::create(&outpath)
                        .map_err(|e| format!("Failed to create output file: {}", e))?;

                    std::io::copy(&mut file, &mut outfile)
                        .map_err(|e| format!("Failed to extract: {}", e))?;

                    #[cfg(not(target_os = "windows"))]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let mut perms = std::fs::metadata(&outpath)
                            .map_err(|e| format!("Failed to get file metadata: {}", e))?
                            .permissions();
                        perms.set_mode(0o755);
                        std::fs::set_permissions(&outpath, perms)
                            .map_err(|e| format!("Failed to set permissions: {}", e))?;
                    }

                    // Apple Silicon's Gatekeeper/AMFI policy refuses to execute
                    // any completely unsigned binary (fails with "killed" or a
                    // security-policy error), even when we spawn it ourselves --
                    // unlike Intel Macs, which are more lenient. Ad-hoc signing
                    // (no Apple Developer account needed, works fully offline)
                    // satisfies that requirement for local execution.
                    #[cfg(target_os = "macos")]
                    {
                        // Strip quarantine xattr to prevent slow per-launch Gatekeeper
                        // network checks -- same fix as yt-dlp.
                        let _ = std::process::Command::new("xattr")
                            .args(["-d", "com.apple.quarantine"])
                            .arg(&outpath)
                            .output();

                        let sign_result = std::process::Command::new("codesign")
                            .args(["--sign", "-", "--force", "--"])
                            .arg(&outpath)
                            .output();
                        match sign_result {
                            Ok(output) if output.status.success() => {
                                println!("✅ ad-hoc signed ffmpeg for local execution");
                            }
                            Ok(output) => {
                                eprintln!(
                                    "⚠️ ad-hoc signing ffmpeg failed: {}",
                                    String::from_utf8_lossy(&output.stderr)
                                );
                            }
                            Err(e) => {
                                eprintln!("⚠️ could not run codesign on ffmpeg: {}", e);
                            }
                        }
                    }

                    println!("✅ ffmpeg installed at: {}", outpath.display());
                    return Ok::<(), String>(());
                }
            }

            Err("ffmpeg binary not found in archive".to_string())
        })
        .await
        .map_err(|e| format!("Extraction task failed: {}", e))
        .and_then(|inner| inner)
        // Whether extraction succeeded or the archive turned out to be
        // unreadable, the temp file has served its purpose. Removing it on
        // failure matters most: a complete-but-corrupt zip would otherwise be
        // "resumed" (i.e. left as-is) and fail to extract on every retry,
        // permanently.
        .inspect_err(|_| {
            let _ = std::fs::remove_file(&temp_zip);
        })?;

        let _ = fs::remove_file(&temp_zip).await;
        } // end #[cfg(not(target_os = "linux"))]

        // Record which build we just installed so future startups can detect a stale binary.
        #[cfg(target_os = "linux")]
        {
            let marker = Self::get_buildid_marker_path();
            let _ = fs::write(&marker, FFMPEG_BUILD_ID).await;
        }

        Ok(())
    }

    pub async fn install(app_handle: &AppHandle, analytics: &Analytics) -> Result<(), String> {
        let mut installing = INSTALL_LOCK.lock().await;

        if Self::is_available().await {
            return Ok(());
        }

        if *installing {
            drop(installing);
            for _ in 0..120 {
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                if Self::is_local_ffmpeg_installed().await {
                    return Ok(());
                }
            }
            return Err("ffmpeg installation timeout".to_string());
        }

        *installing = true;
        let result = Self::download_with_progress(app_handle).await;
        *installing = false;

        match &result {
            Ok(()) => {
                analytics.track_with_data(
                    "dependency_installed",
                    json!({ "dependency": "ffmpeg" }),
                );
            }
            Err(e) => {
                analytics.track_with_data(
                    "dependency_install_failed",
                    json!({ "dependency": "ffmpeg", "reason": truncate_for_analytics(e) }),
                );
            }
        }

        result
    }

    /// Ensure our local ffmpeg is available, downloading if needed
    pub async fn ensure_available(app_handle: &AppHandle, analytics: &Analytics) -> Result<(), String> {
        if Self::is_local_ffmpeg_installed().await {
            return Ok(());
        }

        println!("📥 ffmpeg not found, downloading...");
        Self::install(app_handle, analytics).await
    }

    /// Decompresses a `.tar.xz` archive and extracts the `ffmpeg` binary
    /// (located at `*/bin/ffmpeg` inside the archive) to `out_path`.
    /// Used on Linux where we download BtbN static builds instead of the
    /// ffbinaries zip, because the BtbN build has SSL compiled in statically.
    #[cfg(target_os = "linux")]
    fn extract_tar_xz_ffmpeg(
        archive_path: &std::path::Path,
        out_path: &std::path::Path,
    ) -> Result<(), String> {
        // Step 1: XZ-decompress into a sibling temp .tar file to avoid loading
        // the full ~200 MB decompressed content into memory all at once.
        let temp_tar = archive_path.with_file_name("ffmpeg_temp.tar");

        {
            let xz_file = std::fs::File::open(archive_path)
                .map_err(|e| format!("Failed to open tar.xz: {}", e))?;
            let mut tar_file = std::fs::File::create(&temp_tar)
                .map_err(|e| format!("Failed to create temp tar: {}", e))?;
            lzma_rs::xz_decompress(&mut std::io::BufReader::new(xz_file), &mut tar_file)
                .map_err(|e| format!("Failed to decompress xz: {}", e))?;
        }

        // Step 2: Walk the tar and extract the ffmpeg binary.
        let result = (|| -> Result<(), String> {
            let tar_file = std::fs::File::open(&temp_tar)
                .map_err(|e| format!("Failed to open temp tar: {}", e))?;
            let mut archive = tar::Archive::new(tar_file);

            for entry in archive.entries().map_err(|e| format!("Failed to read tar entries: {}", e))? {
                let mut entry = entry.map_err(|e| format!("Failed to read tar entry: {}", e))?;
                let entry_path = entry.path().map_err(|e| format!("Failed to read entry path: {}", e))?;
                let entry_str = entry_path.to_string_lossy();

                // Match `*/bin/ffmpeg` — the exact binary, not ffprobe/ffplay.
                if entry_str.ends_with("/bin/ffmpeg") || entry_str == "bin/ffmpeg" {
                    entry.unpack(out_path)
                        .map_err(|e| format!("Failed to extract ffmpeg: {}", e))?;

                    use std::os::unix::fs::PermissionsExt;
                    let mut perms = std::fs::metadata(out_path)
                        .map_err(|e| format!("Failed to read permissions: {}", e))?
                        .permissions();
                    perms.set_mode(0o755);
                    std::fs::set_permissions(out_path, perms)
                        .map_err(|e| format!("Failed to set permissions: {}", e))?;

                    println!("✅ ffmpeg installed at: {}", out_path.display());
                    return Ok(());
                }
            }

            Err("ffmpeg binary not found in tar archive".to_string())
        })();

        let _ = std::fs::remove_file(&temp_tar);
        result
    }
}
