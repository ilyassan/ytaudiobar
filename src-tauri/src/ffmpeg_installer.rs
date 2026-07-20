use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use std::sync::LazyLock;
use futures_util::StreamExt;
use tauri::{AppHandle, Emitter};
use crate::ytdlp_installer::DepProgress;

static INSTALL_LOCK: LazyLock<Arc<Mutex<bool>>> = LazyLock::new(|| Arc::new(Mutex::new(false)));

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

    pub async fn is_local_ffmpeg_installed() -> bool {
        Self::get_ffmpeg_path().exists()
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

        #[cfg(target_os = "linux")]
        let download_url = "https://github.com/ffbinaries/ffbinaries-prebuilt/releases/download/v6.1/ffmpeg-6.1-linux-64.zip";

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

        let response = reqwest::get(download_url)
            .await
            .map_err(|e| format!("Failed to download ffmpeg: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("Failed to download ffmpeg: HTTP {}", response.status()));
        }

        let total_size = response.content_length().unwrap_or(0);
        let mut downloaded: u64 = 0;
        let mut stream = response.bytes_stream();
        let temp_zip = ffmpeg_dir.join("ffmpeg_temp.zip");

        let mut file = fs::File::create(&temp_zip)
            .await
            .map_err(|e| format!("Failed to create temp file: {}", e))?;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| format!("Download error: {}", e))?;
            file.write_all(&chunk)
                .await
                .map_err(|e| format!("Write error: {}", e))?;

            downloaded += chunk.len() as u64;

            let _ = app_handle.emit("dep-progress", DepProgress {
                dependency: "ffmpeg".to_string(),
                downloaded,
                total: total_size,
            });
        }

        drop(file);

        // Extract zip
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
        .map_err(|e| format!("Extraction task failed: {}", e))??;

        let _ = fs::remove_file(&temp_zip).await;

        Ok(())
    }

    pub async fn install(app_handle: &AppHandle) -> Result<(), String> {
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

        result
    }

    /// Ensure our local ffmpeg is available, downloading if needed
    pub async fn ensure_available(app_handle: &AppHandle) -> Result<(), String> {
        if Self::is_local_ffmpeg_installed().await {
            return Ok(());
        }

        println!("📥 ffmpeg not found, downloading...");
        Self::install(app_handle).await
    }
}
