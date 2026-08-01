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
    // On Linux, show() + set_focus() does NOT raise the window: neither call
    // carries a user-interaction timestamp, so GNOME/Mutter treats it as an
    // app trying to steal focus on its own and declines (see
    // `present_with_user_interaction_time` below). This used to be worked
    // around with a hide()/show() remap plus an always-on-top toggle, which
    // did force the window up -- but the remap also mapped the window with no
    // timestamp attached, which is exactly what makes GNOME post the stray
    // "YTAudioBar is ready" notification. Attributing the request to the
    // click that triggered it gets the window raised *and* keeps the
    // notification away, so the old dance is no longer needed.
    #[cfg(target_os = "linux")]
    {
        // Read the timestamp first, while the click that triggered us is still
        // the current GTK event -- the calls below can dispatch and clear it.
        let timestamp = gtk::current_event_time();
        let _ = window.unminimize();
        present_with_user_interaction_time(window, timestamp);
    }
}

/// Marks the window as having been summoned by the user interaction at
/// `timestamp`, then shows and raises it.
///
/// Two separate things have to carry that timestamp, which is why this isn't
/// just a `present_with_time()` call:
///
/// 1. The *map* -- `_NET_WM_USER_TIME` has to already be on the window when it
///    gets mapped, otherwise Mutter decides at map time that an unattended app
///    popped a window up and posts the "YTAudioBar is ready" notification.
///    Setting it after the fact (or letting `show()` map an unstamped window)
///    is too late: the notification has already been queued.
/// 2. The *raise/focus* -- `present_with_time()` is the WM-sanctioned request
///    for "put this in front", and passes the same attribution along.
///
/// The window is mapped via Tauri's `show()` rather than by `present()` alone
/// so Tauri's own visibility bookkeeping stays in sync -- presenting the GTK
/// window behind Tauri's back leaves it convinced the window is still hidden.
#[cfg(target_os = "linux")]
fn present_with_user_interaction_time(window: &tauri::WebviewWindow, timestamp: u32) {
    use gtk::glib::object::Cast;
    use gtk::prelude::{GtkWindowExt, WidgetExt};

    let Ok(gtk_window) = window.gtk_window() else {
        // No GTK handle (shouldn't happen on Linux) -- fall back to Tauri's
        // own show/focus so the window at least still appears.
        let _ = window.show();
        let _ = window.set_focus();
        return;
    };

    if timestamp != 0 {
        // The GDK window only exists once realized; it normally already is
        // (the window is created up front and merely hidden), and realizing
        // again is a no-op, but a hidden-and-never-shown window may not be.
        gtk_window.realize();

        // X11-only: under a Wayland-native session there's no X11 window to
        // stamp and the downcast simply fails. This app forces GDK_BACKEND=x11
        // (see main), so in practice this is always the X11 path.
        if let Some(x11_window) = gtk_window
            .window()
            .and_then(|gdk_window| gdk_window.downcast::<gdkx11::X11Window>().ok())
        {
            x11_window.set_user_time(timestamp);
        }
    }

    let _ = window.show();

    match timestamp {
        0 => gtk_window.present(),
        timestamp => gtk_window.present_with_time(timestamp),
    }
}

// The AppImage bundles its own webkit2gtk/gtk3/glib stack (built against an
// older Ubuntu base) so it can run on hosts that don't have those libraries
// installed. That bundled stack ships its own libwayland-client.so, and on
// some hosts that specific library fails to negotiate with the compositor,
// which makes WebKitGTK's EGL display init fail with `EGL_BAD_PARAMETER` and
// hard-abort the whole process -- even though this app never uses Wayland
// directly (GDK_BACKEND is forced to x11 above). Preloading the *system's*
// libwayland-client.so instead of the bundled one fixes it. LD_PRELOAD only
// takes effect at exec time, so we have to re-exec ourselves once with it set.
#[cfg(target_os = "linux")]
fn relaunch_appimage_with_system_wayland_client_preload() {
    use std::os::unix::process::CommandExt;

    if std::env::var_os("APPIMAGE").is_none() || std::env::var_os("YTAUDIOBAR_RELAUNCHED").is_some() {
        return;
    }

    let candidates = [
        "/usr/lib/x86_64-linux-gnu/libwayland-client.so.0",
        "/usr/lib/aarch64-linux-gnu/libwayland-client.so.0",
        "/usr/lib64/libwayland-client.so.0",
        "/usr/lib/libwayland-client.so.0",
        "/lib/x86_64-linux-gnu/libwayland-client.so.0",
    ];
    let Some(preload) = candidates.iter().find(|p| std::path::Path::new(p).exists()) else {
        return;
    };

    let Ok(exe) = std::env::current_exe() else {
        return;
    };

    let err = std::process::Command::new(exe)
        .args(std::env::args_os().skip(1))
        .env("YTAUDIOBAR_RELAUNCHED", "1")
        .env("LD_PRELOAD", preload)
        .exec(); // replaces this process on success; only returns on failure

    eprintln!("⚠️ Failed to relaunch with system libwayland-client.so preloaded: {}", err);
}

/// Identifies the most recent geometry-save request, so a burst of window
/// events results in exactly one write, of the final value.
static GEOMETRY_SAVE_GENERATION: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

/// Persists window geometry, debounced.
///
/// Dragging or resizing emits a configure event per frame. Each one used to
/// spawn its own DB write, and since those tasks were unordered the value that
/// ended up stored wasn't necessarily the window's final position.
fn schedule_window_geometry_save(window: &tauri::Window, x: i32, y: i32, width: u32, height: u32) {
    use std::sync::atomic::Ordering;

    // A minimized window reports meaningless geometry -- Windows parks it at
    // (-32000, -32000) with a ~zero size, and minimizing is exactly what the
    // tray icon does there. Saving that means the next launch rejects the
    // position as off-screen and falls back to the default corner, so
    // "remember where I put the window" never works for tray users.
    if width == 0 || height == 0 || window.is_minimized().unwrap_or(false) {
        return;
    }

    let generation = GEOMETRY_SAVE_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let db = window.app_handle().state::<AppState>().db.clone();

    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;

        // Superseded by a later event in the same burst -- that one will write.
        if GEOMETRY_SAVE_GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }

        println!("📐 Saving geometry: pos=({}, {}), size={}x{}", x, y, width, height);
        let _ = db.save_window_geometry(x, y, width, height).await;
    });
}

/// Marker recording that first-run autostart setup already happened.
///
/// Without it there is no way to tell "the user has never been opted in" from
/// "the user deliberately opted out", and enabling autostart whenever it looks
/// disabled silently undoes the Settings toggle on the next launch.
fn autostart_initialized_marker() -> std::path::PathBuf {
    let mut path = dirs::data_local_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    path.push("ytaudiobar");
    path.push("autostart_initialized");
    path
}

/// Opts new installs into autostart once, then leaves the user's choice alone.
///
/// Also repairs a stale autostart entry: the registered command line is the
/// absolute path of the running binary, and AppImages carry their version in
/// the filename, so upgrading by downloading a new AppImage leaves the entry
/// pointing at a file that no longer exists. `is_enabled()` only checks that
/// the entry exists -- it never validates the path -- so autostart keeps
/// reporting "on" while silently doing nothing at boot.
fn sync_autostart(app: &tauri::AppHandle) {
    let manager = app.autolaunch();
    let marker = autostart_initialized_marker();

    if !marker.exists() {
        println!("🚀 Enabling autostart on system boot (first run)...");
        match manager.enable() {
            Ok(()) => println!("✅ Autostart enabled successfully"),
            Err(e) => eprintln!("⚠️ Failed to enable autostart: {}", e),
        }
        if let Some(parent) = marker.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        // Written even if enable() failed: this records that the one-time
        // opt-in was attempted, so a persistent failure doesn't turn into
        // "silently re-enable on every launch" -- which is the behaviour that
        // made the Settings toggle impossible to turn off.
        let _ = std::fs::write(&marker, "1");
        return;
    }

    if !manager.is_enabled().unwrap_or(false) {
        // Disabled by the user -- leave it alone.
        return;
    }

    #[cfg(target_os = "linux")]
    refresh_stale_autostart_path(&manager);
}

/// Rewrites the autostart entry when its `Exec=` no longer matches this build.
#[cfg(target_os = "linux")]
fn refresh_stale_autostart_path(manager: &tauri_plugin_autostart::AutoLaunchManager) {
    // The plugin registers the AppImage path when running from one, and the
    // real executable path otherwise -- mirror that to compare like for like.
    let Some(current_path) = std::env::var("APPIMAGE")
        .ok()
        .or_else(|| std::env::current_exe().ok()?.to_str().map(str::to_owned))
    else {
        return;
    };

    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let entry = std::path::Path::new(&home)
        .join(".config/autostart")
        .join(format!("{}.desktop", "YTAudioBar"));

    let Ok(contents) = std::fs::read_to_string(&entry) else {
        return;
    };

    let still_valid = contents
        .lines()
        .filter_map(|line| line.strip_prefix("Exec="))
        // The plugin appends launch args after the path, so compare the prefix.
        .any(|exec| exec.trim_end() == current_path || exec.starts_with(&format!("{} ", current_path)));

    if still_valid {
        return;
    }

    println!("🔧 Autostart entry points at an old path -- rewriting it");
    // enable() overwrites the entry with the current path.
    if let Err(e) = manager.enable() {
        eprintln!("⚠️ Failed to refresh autostart entry: {}", e);
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

        // Re-integrate whenever the entry is missing *or* points somewhere
        // else. AppImages are versioned by filename, so a plain "already
        // exists, skip" check leaves the entry pointing at whichever build
        // happened to run first -- and it keeps pointing there after that file
        // is replaced or deleted, which breaks both launching from the app
        // menu and the dock's ability to match the window to this entry.
        let icon_dir = format!("{}/.local/share/icons/hicolor/128x128/apps", home);
        let icon_dest = format!("{}/ytaudiobar.png", icon_dir);

        let exec_is_current = std::fs::read_to_string(&desktop_file)
            .map(|existing| {
                existing
                    .lines()
                    .any(|line| line == format!("Exec={}", appimage_path))
            })
            .unwrap_or(false);
        // The icon is checked separately: if the very first integration hit a
        // transient extraction failure the entry was still written (falling
        // back to Icon=<appimage path>), and keying only off Exec= meant we'd
        // never retry -- leaving a wrong icon forever. Same for a user who
        // clears ~/.local/share/icons.
        if exec_is_current && std::path::Path::new(&icon_dest).exists() {
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

        // Extract into a directory of our own rather than the shared, fixed
        // /tmp/squashfs-root: that name is what *every* AppImage extracts to,
        // so a leftover from another app could be picked up as our icon (or
        // block extraction), and cleaning it up would delete a directory we
        // didn't create.
        let extract_dir = std::env::temp_dir().join(format!("ytaudiobar-icon-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&extract_dir);

        // Invoked directly instead of through `sh -c`: the path is interpolated
        // straight into the command line there, so a perfectly ordinary
        // download name like "YTAudioBar (1).AppImage" -- or any directory with
        // a space in it -- would be a shell syntax error.
        let extract_result = std::fs::create_dir_all(&extract_dir).ok().and_then(|_| {
            std::process::Command::new(&appimage_path)
                .arg("--appimage-extract")
                .arg("YTAudioBar.png")
                .current_dir(&extract_dir)
                .output()
                .ok()
        });

        if let Some(output) = extract_result {
            if output.status.success() {
                let extracted_icon = extract_dir.join("squashfs-root/YTAudioBar.png");
                if extracted_icon.exists()
                    && std::fs::create_dir_all(&icon_dir).is_ok()
                    && std::fs::copy(&extracted_icon, &icon_dest).is_ok()
                {
                    println!("✅ Icon extracted and installed successfully to {}", icon_dest);
                    icon_installed = true;
                }
            }
        }
        let _ = std::fs::remove_dir_all(&extract_dir);

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

        // Create desktop entry.
        //
        // StartupWMClass has to match the window's actual WM_CLASS *exactly* --
        // the shell compares these case-sensitively, and a mismatch means the
        // dock can't tell which app the window belongs to, so it falls back to
        // a generic placeholder icon instead of ours. The window reports
        // WM_CLASS = "ytaudiobar", "Ytaudiobar" (tao derives it from the crate
        // name), so this must stay lowercase -- "YTAudioBar" matches neither.
        let desktop_content = format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Name=YTAudioBar\n\
             Comment=YouTube Audio Player\n\
             Exec={}\n\
             Icon={}\n\
             Categories=AudioVideo;Audio;Player;\n\
             Terminal=false\n\
             StartupWMClass=ytaudiobar\n\
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
        relaunch_appimage_with_system_wayland_client_preload();

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
            // Launching the app again while it's already running should bring
            // the existing window up. set_focus() alone can't: it's a no-op on a
            // hidden window, and closing this app hides rather than exits it --
            // so the common case (close, then relaunch from the dock/app menu)
            // would silently do nothing at all.
            if let Some(window) = app.get_webview_window("main") {
                show_and_focus_window(&window);
            }
        }))
        // Registered so the "Restart" button on the crash screen actually works
        // -- the frontend imports `relaunch` from @tauri-apps/plugin-process
        // (see features/errors/app-error.tsx), which throws without this.
        .plugin(tauri_plugin_process::init())
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
                sync_autostart(&handle);
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
                            // Monitor coordinates are absolute within the virtual
                            // desktop -- see the same adjustment in
                            // commands::window::resize_window. Omitting the origin
                            // pins the window to the primary monitor regardless of
                            // where the app actually is.
                            let origin = monitor.position();
                            let scale = monitor.scale_factor();
                            let margin = |logical: f64| (logical * scale) as i32;
                            if let Ok(window_size) = window.outer_size() {
                                #[cfg(target_os = "windows")]
                                {
                                    let x = origin.x + screen_size.width as i32 - window_size.width as i32 - margin(5.0);
                                    let y = origin.y + screen_size.height as i32 - window_size.height as i32 - margin(80.0);
                                    let _ = window.set_position(PhysicalPosition::new(x, y));
                                }
                                #[cfg(target_os = "linux")]
                                {
                                    let x = origin.x + screen_size.width as i32 - window_size.width as i32 - margin(30.0);
                                    let y = origin.y + margin(40.0);
                                    let _ = window.set_position(PhysicalPosition::new(x, y));
                                }
                                #[cfg(target_os = "macos")]
                                {
                                    let x = origin.x + screen_size.width as i32 - window_size.width as i32 - margin(20.0);
                                    let y = origin.y + margin(40.0);
                                    let _ = window.set_position(PhysicalPosition::new(x, y));
                                }
                            }
                        }
                    }

                    // Applied whether or not the saved geometry was usable.
                    // This used to sit in the restore branch only, so when the
                    // saved position failed the on-screen check the window came
                    // up at its full default height while the UI rendered the
                    // mini player -- and the saved position failing is the
                    // normal case for anyone who minimizes to the tray.
                    let is_mini = db.load_mini_mode().await.unwrap_or(false);
                    if is_mini {
                        use tauri::LogicalSize;
                        let _ = window.set_min_size(Some(LogicalSize::new(380.0f64, 100.0f64)));
                        let _ = window.set_size(LogicalSize::new(380.0f64, 100.0f64));
                        let _ = window.set_resizable(false);
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
                    schedule_window_geometry_save(window, pos.x, pos.y, size.width, size.height);
                }
            }
            WindowEvent::Resized(size) => {
                if let Ok(pos) = window.outer_position() {
                    schedule_window_geometry_save(window, pos.x, pos.y, size.width, size.height);
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
