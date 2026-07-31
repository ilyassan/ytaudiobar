// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod models;
mod database;
mod ytdlp_manager;
mod ytdlp_installer;
mod ffmpeg_installer;
mod audio_manager;
mod queue_manager;
mod download_manager;
mod media_key_manager;
mod command_utils;
mod analytics;
mod commands;

use std::sync::Arc;
use tauri::{
    Manager, WindowEvent, tray::{TrayIconBuilder, TrayIconEvent, MouseButton, MouseButtonState},
    menu::{Menu, MenuItem}
};
use tauri_plugin_autostart::ManagerExt;

use crate::database::DatabaseManager;
use crate::ytdlp_manager::YTDLPManager;
use crate::ytdlp_installer::YTDLPInstaller;
use crate::audio_manager::AudioManager;
use crate::queue_manager::QueueManager;
use crate::download_manager::DownloadManager;
use crate::media_key_manager::MediaKeyManager;
use crate::analytics::Analytics;
use crate::commands::search::*;
use crate::commands::playback::*;
use crate::commands::queue::*;
use crate::commands::library::*;
use crate::commands::downloads::*;
use crate::commands::settings::*;
use crate::commands::window::*;
use crate::commands::media_keys::*;

#[derive(Clone)]
pub struct AppState {
    audio: Arc<AudioManager>,
    queue: Arc<QueueManager>,
    db: Arc<DatabaseManager>,
    ytdlp: Arc<YTDLPManager>,
    downloads: Arc<DownloadManager>,
    media_keys: Arc<MediaKeyManager>,
    analytics: Arc<Analytics>,
}

// macOS menu-bar apps anchor their popover directly under the status item
// (see the native Swift app's `popover.show(relativeTo: button.bounds, ...)`)
// rather than living at a fixed screen position -- the icon's on-screen spot
// shifts depending on how many other apps' menu-bar icons are present, so a
// static corner offset (like the Windows/Linux branches below use) would
// often miss it. TrayIconEvent::Click carries the icon's actual on-screen
// rect, which is what makes this possible.
#[cfg(target_os = "macos")]
fn position_window_under_tray_icon(window: &tauri::WebviewWindow, icon_rect: &tauri::Rect) {
    let scale = window.scale_factor().unwrap_or(1.0);
    let icon_pos = icon_rect.position.to_physical::<i32>(scale);
    let icon_size = icon_rect.size.to_physical::<u32>(scale);

    if let Ok(window_size) = window.outer_size() {
        // Centered under the icon horizontally, top edge just below it.
        let x = icon_pos.x + (icon_size.width as i32 / 2) - (window_size.width as i32 / 2);
        let y = icon_pos.y + icon_size.height as i32 + 4;
        let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
    }
}

fn show_and_focus_window(window: &tauri::WebviewWindow) {
    #[cfg(not(target_os = "linux"))]
    {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
    // On Linux, show() + set_focus() does NOT raise the window — the WM just
    // adds it to the taskbar/dock without bringing it to the front.
    // The only reliable workaround: hide() first to reset the WM state,
    // then unminimize() → set_focus() → show() so the WM treats it as a
    // fresh window appearance and raises it properly.
    // We save and restore the position because hide() causes the WM to forget it.
    // Finally, set_always_on_top(true/false) forces the window above any
    // currently focused or fullscreen app.
    #[cfg(target_os = "linux")]
    {
        let pos = window.outer_position().ok();
        let _ = window.hide();
        let _ = window.unminimize();
        let _ = window.set_focus();
        let _ = window.show();
        if let Some(pos) = pos {
            let _ = window.set_position(tauri::PhysicalPosition::new(pos.x, pos.y));
        }
        let _ = window.set_always_on_top(true);
        let _ = window.set_focus();
        let _ = window.set_always_on_top(false);
    }
}

#[cfg(target_os = "linux")]
fn integrate_appimage_to_system() {
    // Only integrate if running from AppImage
    if let Ok(appimage_path) = std::env::var("APPIMAGE") {
        let home = match std::env::var("HOME") {
            Ok(h) => h,
            Err(_) => return,
        };

        let desktop_file = format!("{}/.local/share/applications/ytaudiobar.desktop", home);

        // Check if already integrated
        if std::path::Path::new(&desktop_file).exists() {
            return;
        }

        println!("📦 Integrating YTAudioBar to system app menu...");

        // Create .local/share/applications directory if it doesn't exist
        let apps_dir = format!("{}/.local/share/applications", home);
        if let Err(e) = std::fs::create_dir_all(&apps_dir) {
            eprintln!("⚠️ Failed to create applications directory: {}", e);
            return;
        }

        // Install icon - extract from AppImage and copy to user icons
        // Extract YTAudioBar.png from AppImage (not .DirIcon which is a broken symlink)
        let mut icon_installed = false;
        let icon_dir = format!("{}/.local/share/icons/hicolor/128x128/apps", home);
        let icon_dest = format!("{}/ytaudiobar.png", icon_dir);

        let extract_result = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("{} --appimage-extract YTAudioBar.png 2>/dev/null", appimage_path))
            .current_dir("/tmp")
            .output();

        if let Ok(output) = extract_result {
            if output.status.success() {
                let extracted_icon = "/tmp/squashfs-root/YTAudioBar.png";
                if std::path::Path::new(extracted_icon).exists() {
                    if std::fs::create_dir_all(&icon_dir).is_ok() {
                        if std::fs::copy(extracted_icon, &icon_dest).is_ok() {
                            println!("✅ Icon extracted and installed successfully to {}", icon_dest);
                            icon_installed = true;
                        }
                    }
                    // Clean up extracted files
                    let _ = std::fs::remove_dir_all("/tmp/squashfs-root");
                }
            }
        }

        if !icon_installed {
            eprintln!("⚠️ Could not extract icon from AppImage, using AppImage path as fallback");
        }

        // Determine icon value - use name if we installed it, otherwise use AppImage path
        let icon_value = if icon_installed {
            "ytaudiobar".to_string()
        } else {
            println!("⚠️ Could not extract icon from AppImage, using AppImage path as fallback");
            appimage_path.clone()
        };

        // Create desktop entry
        let desktop_content = format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Name=YTAudioBar\n\
             Comment=YouTube Audio Player\n\
             Exec={}\n\
             Icon={}\n\
             Categories=AudioVideo;Audio;Player;\n\
             Terminal=false\n\
             StartupWMClass=YTAudioBar\n\
             X-AppImage-Version={}\n",
            appimage_path,
            icon_value,
            env!("CARGO_PKG_VERSION")
        );

        if let Err(e) = std::fs::write(&desktop_file, desktop_content) {
            eprintln!("⚠️ Failed to create desktop entry: {}", e);
            return;
        }

        // Update icon cache
        if icon_installed {
            let _ = std::process::Command::new("gtk-update-icon-cache")
                .arg(format!("{}/.local/share/icons/hicolor", home))
                .arg("-f")
                .arg("-t")
                .output();
        }

        // Update desktop database
        let _ = std::process::Command::new("update-desktop-database")
            .arg(apps_dir)
            .output();

        println!("✅ YTAudioBar integrated! You can now find it in your app menu.");
    }
}

#[tokio::main]
async fn main() {
    // Force X11 backend on Linux - Wayland doesn't support:
    // - Window transparency (needed for rounded corners)
    // - Programmatic window positioning
    // - data-tauri-drag-region (custom titlebar dragging)
    // XWayland provides full compatibility for all these features.
    #[cfg(target_os = "linux")]
    {
        std::env::set_var("GDK_BACKEND", "x11");

        // Work around a known webkit2gtk failure mode where a `transparent`
        // window's alpha-compositing silently breaks -- the window manager
        // still reports the window as open (dock preview, focus, etc.) but
        // nothing ever actually paints on screen. Disabling WebKit's
        // compositing mode is the standard community workaround.
        std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");

        // Integrate AppImage to system on first run
        integrate_appimage_to_system();
    }

    // Initialize database
    let db = DatabaseManager::new()
        .await
        .expect("Failed to initialize database");

    // Anonymous, randomly-generated install id -- persisted so events can be
    // grouped per-install without identifying anyone or any account.
    let analytics_id = db
        .get_or_create_analytics_id()
        .await
        .unwrap_or_else(|_| uuid::Uuid::new_v4().to_string());
    let analytics = Arc::new(Analytics::new(analytics_id));

    // Create app state
    let audio_manager = Arc::new(AudioManager::new(Arc::clone(&analytics)));
    let download_manager = Arc::new(DownloadManager::new(Arc::clone(&analytics)));
    let queue_manager = Arc::new(QueueManager::new());

    // Apply persisted settings (downloads dir, audio quality)
    if let Ok(settings) = db.load_settings().await {
        if !settings.default_download_path.is_empty() {
            let path = std::path::PathBuf::from(&settings.default_download_path);
            if path.exists() {
                download_manager.set_downloads_dir_silent(path).await;
            }
        }
        if !settings.preferred_audio_quality.is_empty() {
            let _ = download_manager.set_audio_quality(settings.preferred_audio_quality).await;
        }
    }
    let media_key_manager = Arc::new(MediaKeyManager::new());
    let app_state = AppState {
        audio: Arc::clone(&audio_manager),
        queue: Arc::clone(&queue_manager),
        db: Arc::new(db),
        ytdlp: Arc::new(YTDLPManager::new()),
        downloads: Arc::clone(&download_manager),
        media_keys: Arc::clone(&media_key_manager),
        analytics: Arc::clone(&analytics),
    };

    analytics.track("app_started");

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // When a second instance tries to open, focus the existing main window
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec![])))
        .manage(app_state)
        .setup(move |app| {
            // Window positioning is handled later in setup with manual calculations
            // for better compatibility across different environments

            // Pure menu-bar app on macOS: no Dock icon, no Cmd+Tab entry --
            // matches the native Swift app's NSApp.setActivationPolicy(.accessory).
            #[cfg(target_os = "macos")]
            let _ = app.handle().set_activation_policy(tauri::ActivationPolicy::Accessory);

            // Set app handle in audio manager for events
            let handle = app.handle().clone();
            let audio_clone = Arc::clone(&audio_manager);
            tauri::async_runtime::spawn(async move {
                audio_clone.set_app_handle(handle).await;
            });

            // Check for yt-dlp updates in background (max once per 24h)
            let update_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = YTDLPInstaller::check_and_update(&update_handle).await {
                    eprintln!("⚠️ Failed to check for yt-dlp updates: {}", e);
                }
            });

            // Set app handle in download manager and initialize existing downloads
            let handle = app.handle().clone();
            let download_clone = Arc::clone(&download_manager);
            tauri::async_runtime::spawn(async move {
                download_clone.set_app_handle(handle).await;
                download_clone.initialize().await;
            });

            // Set app handle in queue manager so it can emit queue-updated events
            let handle = app.handle().clone();
            let queue_clone = Arc::clone(&queue_manager);
            tauri::async_runtime::spawn(async move {
                queue_clone.set_app_handle(handle).await;
            });

            // Initialize media key manager
            let handle = app.handle().clone();
            let media_key_clone = Arc::clone(&media_key_manager);
            tauri::async_runtime::spawn(async move {
                if let Err(e) = media_key_clone.initialize(handle).await {
                    eprintln!("Failed to initialize media keys: {}", e);
                }
            });

            // Check for updates silently in background (disabled in dev mode)
            #[cfg(not(debug_assertions))]
            {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    use std::time::Duration;
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    println!("🔍 Checking for updates in background...");
                    check_for_updates_silently(handle).await;
                });
            }

            // Enable autostart on first run
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let manager = handle.autolaunch();
                match manager.is_enabled() {
                    Ok(is_enabled) => {
                        if !is_enabled {
                            println!("🚀 Enabling autostart on system boot...");
                            if let Err(e) = manager.enable() {
                                eprintln!("⚠️ Failed to enable autostart: {}", e);
                            } else {
                                println!("✅ Autostart enabled successfully");
                            }
                        } else {
                            println!("✅ Autostart already enabled");
                        }
                    }
                    Err(e) => {
                        eprintln!("⚠️ Failed to check autostart status: {}", e);
                    }
                }
            });

            // Listen for track-ended events and auto-play next track
            let handle_clone = app.handle().clone();
            let state_clone = app.state::<AppState>().inner().clone();
            tauri::async_runtime::spawn(async move {
                use tauri::Listener;
                handle_clone.listen("track-ended", move |_event| {
                    let state = state_clone.clone();
                    tauri::async_runtime::spawn(async move {
                        println!("🎵 Track ended, attempting to play next...");
                        if let Some(track) = state.queue.play_next().await {
                            println!("▶️ Auto-playing next track: {}", track.title);
                            if let Some(file_path) = state.downloads.get_downloaded_file_path(&track.id).await {
                                println!("🎵 Auto-playing from local file: {}", file_path);
                                let _ = state.audio.play_from_file(track, file_path).await;
                            } else {
                                let _ = state.audio.play(track).await;
                            }
                        } else {
                            println!("⏹️ No more tracks in queue");
                        }
                    });
                });
            });

            let app = app;

            // Get the main window and show it immediately — the WM maps it and the
            // WebView can start painting right away. Tray icon registration below is a
            // real OS call that can take tens to hundreds of ms; running it after the
            // window is shown means it no longer delays first paint.
            //
            // On macOS this app behaves as a pure menu-bar utility (like the native
            // Swift version) -- it stays hidden until the tray icon is clicked, at
            // which point it's positioned under the icon (see on_tray_icon_event
            // below), rather than appearing at a fixed spot on launch.
            let window = app.get_webview_window("main").unwrap();
            #[cfg(not(target_os = "macos"))]
            show_and_focus_window(&window);

            // Create tray menu
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let show_item = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

            // Create tray icon with Linux fallback
            let tray_icon = if cfg!(target_os = "linux") {
                // On Linux, use the PNG icon explicitly
                match app.default_window_icon() {
                    Some(icon) => icon.clone(),
                    None => {
                        // Fallback: load icon from file
                        let icon_path = app.path().resolve("icons/128x128.png", tauri::path::BaseDirectory::Resource)
                            .expect("Failed to resolve icon path");
                        tauri::image::Image::from_path(icon_path)
                            .expect("Failed to load tray icon")
                    }
                }
            } else {
                app.default_window_icon().unwrap().clone()
            };

            let _tray = TrayIconBuilder::new()
                .icon(tray_icon)
                .menu(&menu)
                // On Linux (AppIndicator protocol), show_menu_on_left_click is ignored
                // and the menu always appears on any click. We still set it false so that
                // on DEs using StatusNotifierItem (KDE, etc.) left-click shows the window.
                .show_menu_on_left_click(false)
                .tooltip("YTAudioBar")
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        #[cfg(target_os = "macos")]
                        rect,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            #[cfg(target_os = "macos")]
                            {
                                // Menu-bar popover behavior: toggle visibility, and
                                // reposition under the icon every time it's shown --
                                // its screen position isn't fixed (depends on how
                                // many other apps' icons are present).
                                if window.is_visible().unwrap_or(false) {
                                    let _ = window.hide();
                                } else {
                                    position_window_under_tray_icon(&window, &rect);
                                    show_and_focus_window(&window);
                                }
                            }
                            #[cfg(not(target_os = "macos"))]
                            {
                                let is_minimized = window.is_minimized().unwrap_or(false);
                                let is_visible = window.is_visible().unwrap_or(false);
                                if is_visible && !is_minimized {
                                    #[cfg(target_os = "windows")]
                                    let _ = window.minimize();
                                    // On Linux don't minimize — left click should always show
                                    #[cfg(target_os = "linux")]
                                    show_and_focus_window(&window);
                                } else {
                                    show_and_focus_window(&window);
                                }
                            }
                        }
                    }
                })
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        // Save window geometry before quitting
                        if let Some(window) = app.get_webview_window("main") {
                            if let (Ok(pos), Ok(size)) = (window.outer_position(), window.outer_size()) {
                                println!("📐 [QUIT] Saving geometry before exit: pos=({}, {}), size={}x{}", pos.x, pos.y, size.width, size.height);
                                let db = app.state::<AppState>().db.clone();
                                match tauri::async_runtime::block_on(
                                    db.save_window_geometry(pos.x, pos.y, size.width, size.height)
                                ) {
                                    Ok(_) => println!("📐 [QUIT] Geometry saved successfully"),
                                    Err(e) => println!("📐 [QUIT] ERROR saving geometry: {}", e),
                                }
                            } else {
                                println!("📐 [QUIT] ERROR: could not get window position/size");
                            }
                        }
                        app.exit(0);
                    }
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            show_and_focus_window(&window);
                        }
                    }
                    _ => {}
                })
                .build(app)?;

            // Restore last saved geometry off the main thread — this used to block the
            // event loop with a synchronous DB round trip right after the window was
            // shown, stalling the WebView's message pump and delaying first paint.
            {
                let window = window.clone();
                let db = app.state::<AppState>().db.clone();

                tauri::async_runtime::spawn(async move {
                    use tauri::{PhysicalPosition, PhysicalSize};

                    let saved = db.load_window_geometry().await;
                    let mut restored = false;

                    if let Ok(Some((x, y, width, height))) = saved {
                        // Check the saved position is on at least one available monitor
                        let monitors = window.available_monitors().unwrap_or_default();
                        let on_screen = monitors.iter().any(|m| {
                            let mp = m.position();
                            let ms = m.size();
                            x >= mp.x && y >= mp.y
                                && x < mp.x + ms.width as i32
                                && y < mp.y + ms.height as i32
                        });

                        if on_screen {
                            let _ = window.set_size(PhysicalSize::new(width, height));
                            let _ = window.set_position(PhysicalPosition::new(x, y));
                            restored = true;
                        }
                    }

                    if !restored {
                        // First launch or off-screen: use default 500px max mode positioning
                        if let Ok(Some(monitor)) = window.current_monitor() {
                            let screen_size = monitor.size();
                            if let Ok(window_size) = window.outer_size() {
                                #[cfg(target_os = "windows")]
                                {
                                    let x = screen_size.width as i32 - window_size.width as i32 - 5;
                                    let y = screen_size.height as i32 - window_size.height as i32 - 80;
                                    let _ = window.set_position(PhysicalPosition::new(x, y));
                                }
                                #[cfg(target_os = "linux")]
                                {
                                    let x = screen_size.width as i32 - window_size.width as i32 - 30;
                                    let y = 40;
                                    let _ = window.set_position(PhysicalPosition::new(x, y));
                                }
                                #[cfg(target_os = "macos")]
                                {
                                    let x = screen_size.width as i32 - window_size.width as i32 - 20;
                                    let y = 40;
                                    let _ = window.set_position(PhysicalPosition::new(x, y));
                                }
                            }
                        }
                    } else {
                        // Geometry was restored — now apply mini mode if needed
                        let is_mini = db.load_mini_mode().await.unwrap_or(false);
                        if is_mini {
                            use tauri::LogicalSize;
                            let _ = window.set_min_size(Some(LogicalSize::new(380.0f64, 100.0f64)));
                            let _ = window.set_size(LogicalSize::new(380.0f64, 100.0f64));
                            let _ = window.set_resizable(false);
                        }
                    }
                });
            }

            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                // Save window geometry before hiding
                if let (Ok(pos), Ok(size)) = (window.outer_position(), window.outer_size()) {
                    println!("📐 [CLOSE] Saving geometry: pos=({}, {}), size={}x{}", pos.x, pos.y, size.width, size.height);
                    let db = window.app_handle().state::<AppState>().db.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = db.save_window_geometry(pos.x, pos.y, size.width, size.height).await;
                    });
                }
                let _ = window.hide();
                api.prevent_close();
            }
            WindowEvent::Moved(pos) => {
                if let Ok(size) = window.outer_size() {
                    println!("📐 [MOVED] pos=({}, {}), size={}x{}", pos.x, pos.y, size.width, size.height);
                    let db = window.app_handle().state::<AppState>().db.clone();
                    let px = pos.x;
                    let py = pos.y;
                    let sw = size.width;
                    let sh = size.height;
                    tauri::async_runtime::spawn(async move {
                        let _ = db.save_window_geometry(px, py, sw, sh).await;
                    });
                }
            }
            WindowEvent::Resized(size) => {
                if let Ok(pos) = window.outer_position() {
                    println!("📐 [RESIZED] pos=({}, {}), size={}x{}", pos.x, pos.y, size.width, size.height);
                    let db = window.app_handle().state::<AppState>().db.clone();
                    let px = pos.x;
                    let py = pos.y;
                    let sw = size.width;
                    let sh = size.height;
                    tauri::async_runtime::spawn(async move {
                        let _ = db.save_window_geometry(px, py, sw, sh).await;
                    });
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            search_youtube,
            search_playlists,
            get_playlist_preview,
            cancel_search,
            get_video_details,
            check_ytdlp_installed,
            install_ytdlp,
            get_ytdlp_version,
            check_ytdlp_update,
            check_ffmpeg_available,
            install_ffmpeg,
            play_track,
            toggle_play_pause,
            pause_playback,
            stop_playback,
            seek_to,
            set_volume,
            set_playback_speed,
            play_next,
            play_previous,
            get_audio_state,
            add_to_queue,
            add_to_queue_next,
            get_queue,
            clear_queue,
            toggle_shuffle,
            get_shuffle_mode,
            cycle_repeat_mode,
            get_repeat_mode,
            get_queue_info,
            reorder_queue,
            remove_from_queue,
            // Playlist commands
            get_all_playlists,
            get_all_playlists_with_counts,
            get_playlist_ids_containing_track,
            create_playlist,
            delete_playlist,
            update_playlist_name,
            get_playlist_tracks,
            reorder_playlist_tracks,
            add_track_to_playlist,
            remove_track_from_playlist,
            add_to_favorites,
            remove_from_favorites,
            play_playlist,
            play_track_list,
            import_playlist,
            // Download commands
            download_track,
            get_active_downloads,
            get_downloaded_tracks,
            get_storage_used,
            delete_download,
            cancel_download,
            // Settings commands
            get_downloads_directory,
            set_downloads_directory,
            get_audio_quality,
            set_audio_quality,
            get_app_version,
            // Media key commands
            update_media_metadata,
            update_media_playback_state,
            clear_media_info,
            // Window commands
            set_mini_mode,
            get_mini_mode,
            resize_window,
            reset_window,
            reinit_audio,
            // Updater commands
            check_for_updates_manual,
            // Autostart commands
            get_autostart_enabled,
            set_autostart_enabled,
            // Fast video info
            get_video_info_fast
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
