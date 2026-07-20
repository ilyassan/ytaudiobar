use crate::AppState;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt;

#[tauri::command]
pub async fn get_app_version() -> Result<String, String> {
    Ok(env!("CARGO_PKG_VERSION").to_string())
}

#[tauri::command]
pub async fn get_autostart_enabled(app: AppHandle) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_autostart_enabled(enabled: bool, app: AppHandle) -> Result<(), String> {
    app.state::<AppState>()
        .analytics
        .track_with_data("autostart_toggled", serde_json::json!({ "enabled": enabled }));
    let manager = app.autolaunch();
    if enabled {
        manager.enable().map_err(|e| e.to_string())
    } else {
        manager.disable().map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub async fn check_for_updates_manual(app: AppHandle) -> Result<bool, String> {
    check_for_updates_silently(app).await;
    Ok(true)
}

#[tauri::command]
pub async fn set_mini_mode(is_mini: bool, state: State<'_, AppState>) -> Result<(), String> {
    state.db.save_mini_mode(is_mini).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_mini_mode(state: State<'_, AppState>) -> Result<bool, String> {
    state.db.load_mini_mode().await.map_err(|e| e.to_string())
}

// Silent auto-update function (like macOS Sparkle) -- called both from the manual
// "Check for Updates" command above and from the delayed background check in
// main.rs's setup().
pub async fn check_for_updates_silently(app: AppHandle) {
    use tauri_plugin_updater::UpdaterExt;

    // On Linux, auto-update only works with AppImage, not .deb
    #[cfg(target_os = "linux")]
    {
        // Check for APPIMAGE environment variable (set by AppImage runtime)
        // This variable contains the path to the .AppImage file and is only set when running from AppImage
        if std::env::var("APPIMAGE").is_err() {
            println!("ℹ️ Skipping auto-update: .deb installations cannot be updated automatically.");
            println!("   To get auto-updates, use the AppImage version instead.");
            return;
        }
    }

    println!("🔍 Starting update check...");
    println!("📍 Update endpoint: https://github.com/ilyassan/ytaudiobar/releases/latest/download/latest.json");

    match app.updater() {
        Ok(updater) => {
            println!("✅ Updater initialized successfully");
            match updater.check().await {
                Ok(Some(update)) => {
                    println!("🔄 Update available!");
                    println!("   Current version: {}", update.current_version);
                    println!("   New version: {}", update.version);
                    println!("   Download URL: {}", update.download_url);

                    // Download silently in background
                    println!("📥 Downloading update in background...");
                    match update.download(
                        |chunk_len, content_len| {
                            if let Some(total) = content_len {
                                let progress = (chunk_len as f64 / total as f64) * 100.0;
                                if progress as u32 % 10 == 0 {
                                    println!("   Download progress: {:.0}%", progress);
                                }
                            }
                        },
                        || {
                            println!("📦 Download complete!");
                        }
                    ).await {
                        Ok(bytes) => {
                            println!("✅ Update downloaded successfully!");

                            // Finalize the update (instant, non-blocking)
                            match update.install(bytes) {
                                Ok(_) => {
                                    println!("✅ Update ready! Will be applied on next app launch");
                                }
                                Err(e) => {
                                    eprintln!("⚠️ Failed to finalize update: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("❌ Failed to download update: {}", e);
                        }
                    }
                }
                Ok(None) => {
                    println!("✅ App is already up to date");
                }
                Err(e) => {
                    eprintln!("⚠️ Failed to check for updates: {}", e);
                    eprintln!("   This could be due to:");
                    eprintln!("   - Network connectivity issues");
                    eprintln!("   - latest.json not found on server");
                    eprintln!("   - Invalid JSON format");
                }
            }
        }
        Err(e) => {
            eprintln!("⚠️ Updater not available: {}", e);
        }
    }
}
