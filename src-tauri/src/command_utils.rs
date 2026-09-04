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
        // The AppImage runtime overwrites LD_LIBRARY_PATH with its own bundled
        // .so paths before the app starts. yt-dlp is also a PyInstaller bundle
        // and inherits this LD_LIBRARY_PATH, which makes it load incompatible
        // versions of libssl/libcrypto from the AppImage instead of the system
        // ones — causing silent HTTPS failures where yt-dlp exits with no URL
        // and no error output, every bypass method fails, and the app shows
        // "not available in your region". The AppImage runtime saves the
        // original value in APPIMAGE_ORIGINAL_LD_LIBRARY_PATH — restore that,
        // or clear the var entirely when not running from an AppImage.
        match std::env::var("APPIMAGE_ORIGINAL_LD_LIBRARY_PATH") {
            Ok(original) if !original.is_empty() => {
                cmd.env("LD_LIBRARY_PATH", original);
            }
            _ => {
                cmd.env_remove("LD_LIBRARY_PATH");
            }
        }
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
        // Same LD_LIBRARY_PATH fix as command_no_window above.
        match std::env::var("APPIMAGE_ORIGINAL_LD_LIBRARY_PATH") {
            Ok(original) if !original.is_empty() => {
                cmd.env("LD_LIBRARY_PATH", original);
            }
            _ => {
                cmd.env_remove("LD_LIBRARY_PATH");
            }
        }
    }

    cmd
}

/// Converts raw yt-dlp stderr into a short, user-friendly error message.
/// Never expose internal yt-dlp flags, URLs, or technical jargon to users.
pub fn friendly_ytdlp_error(raw: &str) -> String {
    let lower = raw.to_lowercase();

    // Bot / rate-limit detection
    if lower.contains("sign in to confirm")
        || lower.contains("confirm you're not a bot")
        || lower.contains("not a robot")
    {
        return "YouTube is temporarily blocking requests. Try again in a few minutes.".to_string();
    }
    if lower.contains("429") || lower.contains("too many requests") {
        return "Too many requests to YouTube. Wait a moment and try again.".to_string();
    }

    // Network / connection
    if lower.contains("unable to download webpage")
        || lower.contains("unable to connect")
        || lower.contains("connection refused")
        || lower.contains("connection timed out")
        || lower.contains("no route to host")
        || lower.contains("name or service not known")
        || lower.contains("network is unreachable")
        || (lower.contains("network") && lower.contains("error"))
    {
        return "Connection failed. Check your internet connection and try again.".to_string();
    }

    // Video access / availability
    if lower.contains("private video") || lower.contains("this is a private video") {
        return "This video is private.".to_string();
    }
    if lower.contains("members-only") || lower.contains("members only") {
        return "This video is for channel members only.".to_string();
    }
    if lower.contains("age")
        && (lower.contains("restrict") || lower.contains("gated") || lower.contains("limit"))
    {
        return "This video is age-restricted.".to_string();
    }
    if lower.contains("not available in your country")
        || lower.contains("not available in your region")
        || lower.contains("geo-restricted")
    {
        return "This video is not available in your region.".to_string();
    }
    if lower.contains("copyright") {
        return "This video is unavailable due to a copyright claim.".to_string();
    }
    if lower.contains("video unavailable")
        || lower.contains("this video is unavailable")
        || lower.contains("has been removed")
        || lower.contains("is no longer available")
    {
        return "This video is no longer available.".to_string();
    }
    if lower.contains("404") || lower.contains("not found") {
        return "Video not found.".to_string();
    }
    if lower.contains("403") {
        return "Access denied by YouTube. Try again later.".to_string();
    }

    "An error occurred. Please try again.".to_string()
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
