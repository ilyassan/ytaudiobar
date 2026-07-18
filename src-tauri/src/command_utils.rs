use tokio::process::Command as TokioCommand;
use std::process::Command as StdCommand;
use std::time::{SystemTime, UNIX_EPOCH};

/// Current unix timestamp in seconds, as used for created/added/download dates.
pub fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before unix epoch")
        .as_secs() as i64
}

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

/// Creates a new async Command with CREATE_NO_WINDOW flag automatically applied on Windows.
/// On Linux, clears AppImage Python env vars so yt-dlp's embedded Python isn't poisoned.
pub fn command_no_window(program: &str) -> TokioCommand {
    let mut cmd = TokioCommand::new(program);

    #[cfg(target_os = "windows")]
    {
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    #[cfg(target_os = "linux")]
    {
        cmd.env_remove("PYTHONHOME");
        cmd.env_remove("PYTHONPATH");
    }

    cmd
}

/// Creates a new blocking Command with CREATE_NO_WINDOW flag automatically applied on Windows.
/// On Linux, clears AppImage Python env vars so yt-dlp's embedded Python isn't poisoned.
pub fn command_no_window_blocking(program: &str) -> StdCommand {
    let mut cmd = StdCommand::new(program);

    #[cfg(target_os = "windows")]
    {
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    #[cfg(target_os = "linux")]
    {
        cmd.env_remove("PYTHONHOME");
        cmd.env_remove("PYTHONPATH");
    }

    cmd
}
