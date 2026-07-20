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
        let scale = monitor.scale_factor();
        let win_w = (380.0 * scale) as i32;
        let win_h = (height * scale) as i32;
        #[cfg(target_os = "windows")]
        {
            let x = screen.width as i32 - win_w - 5;
            let y = screen.height as i32 - win_h - 80;
            let _ = window.set_position(PhysicalPosition::new(x, y));
        }
        #[cfg(target_os = "linux")]
        {
            let x = screen.width as i32 - win_w - 30;
            let _ = window.set_position(PhysicalPosition::new(x, 40i32));
        }
    }
    Ok(())
}
