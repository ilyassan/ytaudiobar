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
pub async fn reset_window(
    window: tauri::WebviewWindow,
    height: f64,
    tray_pos: tauri::State<'_, crate::LastTrayWindowPos>,
) -> Result<(), String> {
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

    // On macOS snap back to the exact position the tray-icon click last used.
    // This is always directly under the menu-bar icon, which is the only
    // sensible "home" position for a menu-bar app.
    #[cfg(target_os = "macos")]
    {
        if let Ok(stored) = tray_pos.0.lock() {
            if let Some(pos) = *stored {
                let _ = window.set_position(pos);
            }
        }
        // Fallback: user hasn't clicked the tray icon yet this session —
        // just leave the window where it is (size was already reset above).
        return Ok(());
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = &tray_pos; // only used on macOS; state is always managed
        if let Ok(Some(monitor)) = window.current_monitor() {
            let screen = monitor.size();
            let origin = monitor.position();
            let scale = monitor.scale_factor();
            let win_w = (380.0 * scale) as i32;
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
        }
    }
    Ok(())
}
