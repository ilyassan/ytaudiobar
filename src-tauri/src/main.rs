// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::{
    Manager, WindowEvent, tray::{TrayIconBuilder, TrayIconEvent, MouseButton, MouseButtonState},
    menu::{Menu, MenuItem}
};

fn main() {
    tauri::Builder::default()
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
        .invoke_handler(tauri::generate_handler![])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
