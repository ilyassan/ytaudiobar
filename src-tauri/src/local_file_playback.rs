use crate::models::YTVideoInfo;
use lofty::file::{AudioFile, TaggedFileExt};
use std::path::Path;

// Audio extensions we advertise as openable (bundle.fileAssociations,
// Linux .desktop MimeType) and recognize in incoming argv/RunEvent::Opened
// paths. Kept as one list so the two stay in sync.
pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    "mp3", "m4a", "m4b", "m4p", "mp4", "flac", "ogg", "oga", "opus", "spx", "wav", "aac",
    "webm", "weba", "wma", "ape", "wv", "tta", "aiff", "aif", "aifc", "amr", "au", "caf",
    "mka", "ac3", "dts", "gsm", "voc", "3ga", "dsf", "dff",
];

pub fn is_supported_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| SUPPORTED_EXTENSIONS.iter().any(|ext| ext.eq_ignore_ascii_case(e)))
        .unwrap_or(false)
}

/// Builds a player-ready track from a local audio file the user opened
/// directly (double-click / "Open with"), never from anything in the
/// library/downloads. Reads title/artist/duration/embedded cover art via
/// `lofty`, falling back to the filename and an unknown duration when a
/// file has no (or unreadable) tags -- a missing tag is routine for a
/// random local file, not a reason to refuse to play it.
pub fn build_track_from_local_file(path: &Path) -> YTVideoInfo {
    let file_stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Unknown".to_string());

    let tagged = lofty::probe::Probe::open(path)
        .and_then(|p| p.read())
        .ok();

    let mut title = file_stem.clone();
    let mut uploader = "Local File".to_string();
    let mut duration = 0i64;
    let mut thumbnail_url = None;

    if let Some(tagged_file) = tagged {
        duration = tagged_file.properties().duration().as_secs() as i64;

        let tag = tagged_file
            .primary_tag()
            .or_else(|| tagged_file.first_tag());

        if let Some(tag) = tag {
            use lofty::tag::Accessor;
            if let Some(t) = tag.title() {
                if !t.trim().is_empty() {
                    title = t.to_string();
                }
            }
            if let Some(a) = tag.artist() {
                if !a.trim().is_empty() {
                    uploader = a.to_string();
                }
            }

            // Embedded cover art, encoded as a data: URI -- there's no HTTP
            // URL to point the frontend's existing <img src={thumbnail_url}>
            // at, since the image lives inside this local file, not on a
            // server. A data URI reuses that same rendering path unchanged.
            if let Some(picture) = tag.pictures().first() {
                use base64::Engine;
                let mime = picture.mime_type().map(|m| m.to_string()).unwrap_or_else(|| "image/jpeg".to_string());
                let encoded = base64::engine::general_purpose::STANDARD.encode(picture.data());
                thumbnail_url = Some(format!("data:{};base64,{}", mime, encoded));
            }
        }
    }

    // Prefixed and hashed rather than using the raw path as-is: this id
    // flows through the exact same fields a real YouTube video's id does
    // (favorites/playlists/downloaded-file lookups all key off it), so it
    // must never collide with a real video id, and the raw path could
    // contain characters those lookups/DB keys don't expect.
    let id = format!("local:{:x}", stable_path_hash(&path.to_string_lossy()));

    YTVideoInfo {
        id,
        title,
        uploader,
        duration,
        thumbnail_url,
        audio_url: None,
        description: None,
    }
}

// A cryptographic hash isn't needed here -- this id only has to be stable
// for the same path within one run and distinct enough not to collide by
// accident; it's never persisted or compared across launches. FNV-1a avoids
// pulling in a hashing crate for that.
fn stable_path_hash(input: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in input.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_supported_extensions_case_insensitively() {
        assert!(is_supported_audio_file(Path::new("song.mp3")));
        assert!(is_supported_audio_file(Path::new("song.MP3")));
        assert!(is_supported_audio_file(Path::new("song.FLAC")));
    }

    #[test]
    fn rejects_unsupported_extensions() {
        assert!(!is_supported_audio_file(Path::new("video.mkv")));
        assert!(!is_supported_audio_file(Path::new("document.pdf")));
        assert!(!is_supported_audio_file(Path::new("no_extension")));
    }

    #[test]
    fn hash_is_stable_for_the_same_input() {
        assert_eq!(stable_path_hash("C:/music/song.mp3"), stable_path_hash("C:/music/song.mp3"));
    }

    #[test]
    fn hash_differs_for_different_paths() {
        assert_ne!(stable_path_hash("C:/music/a.mp3"), stable_path_hash("C:/music/b.mp3"));
    }
}
