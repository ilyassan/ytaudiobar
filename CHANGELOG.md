# Changelog

All notable user-facing changes to YTAudioBar are documented here, in the
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) style. See
[`src/lib/whats-new.ts`](src/lib/whats-new.ts) for the same highlights as
shown in-app; add an entry in both places when cutting a release.

## [2.6.0-beta.4] - 2026-08-11

### Fixed

- Displayed position jumping ahead a few seconds when resuming a paused track.

## [2.6.0-beta.3] - 2026-08-11

### Added

- Local playback now recognizes many more audio formats (m4b, wma, ape,
  aiff, amr, and more), not just the most common ones.

## [2.6.0-beta.2] - 2026-08-11

### Fixed

- YTAudioBar not showing up in "Open with" for audio files on Windows,
  Linux, and macOS.

## [2.6.0-beta.1] - 2026-08-11

### Added

- Open local audio files directly with YTAudioBar (double-click, or "Open
  with") — title, duration, and cover art are read from the file itself.

## [2.5.1] - 2026-08-11

Backport of the non-feature fixes below onto 2.5.0, cut from
`release/2.5.1` while the local-file-playback work above continues on
`main` toward 2.6.0.

### Fixed

- Displayed position jumping ahead a few seconds when resuming a paused track.
- Playback not retrying when a stream fails to start, giving up immediately
  instead.
- The "cookies from browser" bypass failing for Chrome/Edge users on
  Windows.

### Added

- Dependency install/update outcomes now logged to analytics for visibility
  into failures in the field.

## [2.5.0] - 2026-08-09

### Added

- Auto-retry for downloads when YouTube blocks a request.

### Fixed

- Playback recovering when network drops mid-stream, with an accurate
  position.
- New installs now save downloads to the Music folder instead of Downloads,
  so a Downloads cleanup can't take them with it.
