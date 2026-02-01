// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod models;
mod database;
mod ytdlp_manager;
mod ytdlp_installer;

use std::sync::Arc;
use tokio::sync::Mutex;
use tauri::{
    Manager, State, WindowEvent, tray::{TrayIconBuilder, TrayIconEvent, MouseButton, MouseButtonState},
    menu::{Menu, MenuItem}
};

use crate::database::DatabaseManager;
use crate::models::{AudioState, QueueState, YTVideoInfo};
use crate::ytdlp_manager::YTDLPManager;
use crate::ytdlp_installer::YTDLPInstaller;

pub struct AppState {
    audio_state: Arc<Mutex<AudioState>>,
    queue_state: Arc<Mutex<QueueState>>,
    db: Arc<DatabaseManager>,
    ytdlp: Arc<YTDLPManager>,
}

#[tauri::command]
async fn search_youtube(
    query: String,
    music_mode: bool,
    state: State<'_, AppState>,
) -> Result<Vec<YTVideoInfo>, String> {
    state.ytdlp.search(query, music_mode).await
}

#[tauri::command]
async fn check_ytdlp_installed() -> Result<bool, String> {
    Ok(YTDLPInstaller::is_installed().await)
}

#[tauri::command]
async fn install_ytdlp() -> Result<(), String> {
    YTDLPInstaller::install().await
}

#[tauri::command]
async fn get_ytdlp_version() -> Result<String, String> {
    YTDLPInstaller::get_version().await
}

#[tokio::main]
async fn main() {
    // Initialize database
    let db = DatabaseManager::new()
        .await
        .expect("Failed to initialize database");

    // Create app state
    let app_state = AppState {
        audio_state: Arc::new(Mutex::new(AudioState::default())),
        queue_state: Arc::new(Mutex::new(QueueState::default())),
        db: Arc::new(db),
        ytdlp: Arc::new(YTDLPManager::new()),
    };

    tauri::Builder::default()
        .manage(app_state)
        .setup(|app| {
            // Create tray menu
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let show_item = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

            // Create tray icon
            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .menu_on_left_click(false)
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            if window.is_visible().unwrap_or(false) {
                                let _ = window.hide();
                            } else {
                                // Position window in bottom-right before showing
                                #[cfg(target_os = "windows")]
                                {
                                    use tauri::PhysicalPosition;
                                    if let Ok(Some(monitor)) = window.current_monitor() {
                                        let screen_size = monitor.size();
                                        if let Ok(window_size) = window.outer_size() {
                                            let x = screen_size.width as i32 - window_size.width as i32 - 5;
                                            let y = screen_size.height as i32 - window_size.height as i32 - 80;
                                            let _ = window.set_position(PhysicalPosition::new(x, y));
                                        }
                                    }
                                }
                                let _ = window.show().and_then(|_| window.set_focus());
                            }
                        }
                    }
                })
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        app.exit(0);
                    }
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show().and_then(|_| window.set_focus());
                        }
                    }
                    _ => {}
                })
                .build(app)?;

            // Get the main window
            let window = app.get_webview_window("main").unwrap();

            // Position window near system tray (bottom-right on Windows, top-right on Linux)
            #[cfg(target_os = "windows")]
            {
                use tauri::PhysicalPosition;
                if let Some(monitor) = window.current_monitor()? {
                    let screen_size = monitor.size();
                    if let Ok(window_size) = window.outer_size() {
                        // Position in bottom-right corner - close to right edge, more space from taskbar
                        let x = screen_size.width as i32 - window_size.width as i32 - 5;
                        let y = screen_size.height as i32 - window_size.height as i32 - 80;

                        let _ = window.set_position(PhysicalPosition::new(x, y));
                    }
                }
            }

            #[cfg(target_os = "linux")]
            {
                use tauri::PhysicalPosition;
                if let Some(monitor) = window.current_monitor()? {
                    let screen_size = monitor.size();
                    if let Ok(window_size) = window.outer_size() {
                        // Position in top-right corner with some padding
                        let x = screen_size.width as i32 - window_size.width as i32 - 10;
                        let y = 50;

                        let _ = window.set_position(PhysicalPosition::new(x, y));
                    }
                }
            }

            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                // Hide window instead of closing
                let _ = window.hide();
                api.prevent_close();
            }
            WindowEvent::Focused(false) => {
                // Auto-hide when clicking outside (loses focus)
                let _ = window.hide();
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            search_youtube,
            check_ytdlp_installed,
            install_ytdlp,
            get_ytdlp_version
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
