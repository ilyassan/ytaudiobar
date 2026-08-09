use tokio::process::Command as TokioCommand;
use std::process::Command as StdCommand;
use std::time::{SystemTime, UNIX_EPOCH};

/// Current unix timestamp in seconds, as used for created/added/download dates.
pub fn unix_timestamp() -> i64 {
    // A misconfigured/reset RTC (VMs, some embedded boards, or a user
    // manually setting the clock) can genuinely predate the epoch. This is
    // called from many command handlers (adding a track/playlist/favorite),
    // so panicking here would abort whatever async task called it -- fall
    // back to 0 instead, which just means an implausible timestamp on that
    // one record rather than a crash.
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_timestamp_is_a_plausible_recent_value() {
        let ts = unix_timestamp();
        // 1700000000 ~= Nov 2023 -- a sane lower bound that will hold for
        // years without needing updates, while still catching an obviously
        // broken clock (e.g. epoch 0, or a negative/garbage value).
        assert!(ts > 1_700_000_000);
    }

    #[test]
    fn unix_timestamp_does_not_go_backwards() {
        let first = unix_timestamp();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let second = unix_timestamp();
        assert!(second >= first);
    }
}
