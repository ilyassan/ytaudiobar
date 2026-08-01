#[tauri::command]
pub async fn resize_window(window: tauri::WebviewWindow, height: f64) -> Result<(), String> {
    use tauri::LogicalSize;
    if height < 500.0 {
        // Mini mode: remove min constraint first, then resize, then lock
        let _ = window.set_min_size(Some(LogicalSize::new(380.0f64, height)));
        let _ = window.set_size(LogicalSize::new(380.0f64, height));
        let _ = window.set_resizable(false);
    } else {
        // Max mode: resize, restore constraints, unlock
        let _ = window.set_resizable(true);
        let _ = window.set_size(LogicalSize::new(380.0f64, height));
        let _ = window.set_min_size(Some(LogicalSize::new(380.0f64, 500.0f64)));
    }
    Ok(())
}

#[tauri::command]
pub async fn reset_window(window: tauri::WebviewWindow, height: f64) -> Result<(), String> {
    use tauri::{PhysicalPosition, LogicalSize};
    if height < 500.0 {
        let _ = window.set_min_size(Some(LogicalSize::new(380.0f64, height)));
        let _ = window.set_size(LogicalSize::new(380.0f64, height));
        let _ = window.set_resizable(false);
    } else {
        let _ = window.set_resizable(true);
        let _ = window.set_size(LogicalSize::new(380.0f64, height));
        let _ = window.set_min_size(Some(LogicalSize::new(380.0f64, 500.0f64)));
    }
    if let Ok(Some(monitor)) = window.current_monitor() {
        let screen = monitor.size();
        // Monitor coordinates are absolute within the virtual desktop, so the
        // monitor's own origin has to be added in. Without it every position is
        // computed as if the monitor started at (0, 0), which puts the window
        // on the primary display no matter which one the app is actually on --
        // and for a monitor positioned to the left (negative origin) it can
        // land off-screen entirely.
        let origin = monitor.position();
        let scale = monitor.scale_factor();
        let win_w = (380.0 * scale) as i32;
        // Margins are authored in logical pixels; scaling them keeps the gap
        // looking the same on a HiDPI display instead of halving it.
        let margin = |logical: f64| (logical * scale) as i32;

        #[cfg(target_os = "windows")]
        {
            let win_h = (height * scale) as i32;
            let x = origin.x + screen.width as i32 - win_w - margin(5.0);
            let y = origin.y + screen.height as i32 - win_h - margin(80.0);
            let _ = window.set_position(PhysicalPosition::new(x, y));
        }
        #[cfg(target_os = "linux")]
        {
            let x = origin.x + screen.width as i32 - win_w - margin(30.0);
            let _ = window.set_position(PhysicalPosition::new(x, origin.y + margin(40.0)));
        }
        // Near the top-right, below the menu bar -- macOS has no taskbar-corner
        // convention like Windows, so this just keeps it near the menu-bar
        // tray icon instead of defaulting to dead center.
        #[cfg(target_os = "macos")]
        {
            let x = origin.x + screen.width as i32 - win_w - margin(20.0);
            let _ = window.set_position(PhysicalPosition::new(x, origin.y + margin(40.0)));
        }
    }
    Ok(())
}
