use futures_util::StreamExt;
use std::path::Path;
use tokio::io::AsyncWriteExt;

// A dropped connection part-way through a ~38MB dependency download used to
// mean starting again from zero, which on a slow or flaky link can mean never
// finishing at all. GitHub's release assets advertise `accept-ranges: bytes`,
// so a partial file can be continued instead.
const MAX_ATTEMPTS: usize = 4;

/// Downloads `url` into `temp_path`, resuming a partial file if one is present,
/// and retrying on network errors.
///
/// The caller is expected to download to a temporary path and only move the
/// result into place once this returns `Ok` -- a partial file is deliberately
/// left behind on failure so the next attempt can continue from it.
///
/// `on_progress` receives `(downloaded_bytes, total_bytes)`; `total_bytes` is 0
/// when the server doesn't report a length.
pub async fn download_resumable<F>(
    url: &str,
    temp_path: &Path,
    mut on_progress: F,
) -> Result<(), String>
where
    F: FnMut(u64, u64),
{
    let mut last_error = String::new();

    for attempt in 1..=MAX_ATTEMPTS {
        match try_download(url, temp_path, &mut on_progress).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                last_error = e;
                if attempt < MAX_ATTEMPTS {
                    let resume_from = partial_len(temp_path).await;
                    println!(
                        "⚠️ Download attempt {}/{} failed ({}), resuming from {} bytes",
                        attempt, MAX_ATTEMPTS, last_error, resume_from
                    );
                    // Back off a little so an immediate retry doesn't just hit
                    // the same transient failure.
                    tokio::time::sleep(std::time::Duration::from_secs(2 * attempt as u64)).await;
                }
            }
        }
    }

    Err(format!(
        "Download failed after {} attempts: {}",
        MAX_ATTEMPTS, last_error
    ))
}

async fn partial_len(path: &Path) -> u64 {
    tokio::fs::metadata(path).await.map(|m| m.len()).unwrap_or(0)
}

async fn try_download<F>(url: &str, temp_path: &Path, on_progress: &mut F) -> Result<(), String>
where
    F: FnMut(u64, u64),
{
    let already_have = partial_len(temp_path).await;

    let client = reqwest::Client::new();
    let mut request = client.get(url);
    if already_have > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={}-", already_have));
    }

    let response = request
        .send()
        .await
        .map_err(|e| format!("request failed: {}", e))?;

    // 416 means we already hold at least as many bytes as the server has, which
    // happens if a previous attempt wrote everything but failed before the
    // caller could verify it. Treat it as "nothing left to fetch" and let the
    // caller's size check decide whether the file is actually complete.
    if response.status() == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
        return Ok(());
    }

    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }

    // Only 206 means the server honoured our range. A plain 200 means it's
    // sending the whole file again, so the partial data has to be discarded --
    // appending to it would splice two copies together.
    let resuming = response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    let start = if resuming { already_have } else { 0 };
    let total = response.content_length().unwrap_or(0) + start;

    if resuming {
        println!("↩️ Resuming download from {} bytes", start);
    }

    let mut file = if resuming {
        tokio::fs::OpenOptions::new()
            .append(true)
            .open(temp_path)
            .await
    } else {
        if let Some(parent) = temp_path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        tokio::fs::File::create(temp_path).await
    }
    .map_err(|e| format!("could not open {}: {}", temp_path.display(), e))?;

    let mut downloaded = start;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("stream error: {}", e))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("write error: {}", e))?;
        downloaded += chunk.len() as u64;
        on_progress(downloaded, total);
    }

    // Without this, buffered tail bytes can still be unwritten when the caller
    // renames the file into place.
    file.flush()
        .await
        .map_err(|e| format!("flush error: {}", e))?;

    if total > 0 && downloaded < total {
        return Err(format!(
            "connection closed early ({} of {} bytes)",
            downloaded, total
        ));
    }

    Ok(())
}

/// Checks a downloaded file against an expected SHA-256, in hex.
///
/// Reads incrementally rather than loading the whole file, since these are
/// tens of megabytes.
pub async fn sha256_matches(path: &Path, expected_hex: &str) -> bool {
    use sha2::{Digest, Sha256};
    use tokio::io::AsyncReadExt;

    let Ok(mut file) = tokio::fs::File::open(path).await else {
        return false;
    };

    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        match file.read(&mut buffer).await {
            Ok(0) => break,
            Ok(n) => hasher.update(&buffer[..n]),
            Err(_) => return false,
        }
    }

    let actual = hex::encode(hasher.finalize());
    actual.eq_ignore_ascii_case(expected_hex.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sha256_matches_accepts_the_correct_digest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f");
        tokio::fs::write(&path, b"hello").await.unwrap();

        // Well-known SHA-256 of "hello".
        assert!(
            sha256_matches(
                &path,
                "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
            )
            .await
        );
    }

    #[tokio::test]
    async fn sha256_matches_is_case_insensitive() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f");
        tokio::fs::write(&path, b"hello").await.unwrap();

        assert!(
            sha256_matches(
                &path,
                "2CF24DBA5FB0A30E26E83B2AC5B9E29E1B161E5C1FA7425E73043362938B9824"
            )
            .await
        );
    }

    #[tokio::test]
    async fn sha256_matches_rejects_a_truncated_file() {
        // The exact scenario this guards: a partial download whose digest can't
        // match the real one.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f");
        tokio::fs::write(&path, b"hell").await.unwrap();

        assert!(
            !sha256_matches(
                &path,
                "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
            )
            .await
        );
    }

    #[tokio::test]
    async fn sha256_matches_reports_false_for_a_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        assert!(sha256_matches(&dir.path().join("nope"), "00").await == false);
    }
}
