# YTAudioBar

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![GitHub Release](https://img.shields.io/github/release/ilyassan/ytaudiobar.svg)](https://github.com/ilyassan/ytaudiobar/releases)
[![Downloads](https://img.shields.io/github/downloads/ilyassan/ytaudiobar/total.svg)](https://github.com/ilyassan/ytaudiobar/releases)

<div align="center">
  <img src="app-icon.png" alt="YTAudioBar Logo" width="128" height="128">
</div>

A feature-rich desktop application for streaming and downloading YouTube audio on Windows and Linux. Extract audio from YouTube videos, stream them directly, or download for offline listening with a Spotify-inspired interface.

**For the native macOS version, see [YTAudioBar-macos](https://github.com/ilyassan/YTAudioBar-macos)**

## Contents

- [Features](#features)
- [Installation](#installation)
- [Usage](#usage)
- [System Requirements](#system-requirements)
- [Development](#development)
- [Architecture](#architecture)
- [Contributing](#contributing)
- [License](#license)

## Features

- **Stream YouTube Audio** — Play high-quality audio directly from YouTube with intuitive playback controls
- **Download for Offline** — Download tracks locally in FLAC format with automatic metadata
- **Queue Management** — Build and manage playback queues on the fly
- **Unlimited Playlists** — Create custom playlists and organize your music collection
- **Fast Seeking** — Near-instant seeking in downloaded tracks using FLAC seek tables
- **OS Media Controls** — Full integration with Windows SMTC and Linux media controls
- **Media Key Support** — Control playback with media keys (Play, Pause, Next, Previous, Seek)
- **Search Modes** — Toggle between general search and music-optimized search
- **System Tray** — Minimize to system tray for always-on access
- **Auto-start** — Optional automatic startup with your system

## Installation

### Windows

Download the latest `.exe` installer from [GitHub Releases](https://github.com/ilyassan/ytaudiobar/releases) or the [official website](https://ytaudiobar.vercel.app/download).

1. Download `YTAudioBar_x64-setup.exe`
2. Run the installer
3. On first launch, the app will automatically download `yt-dlp` and `ffmpeg` (~15 MB)

Minimum requirements: Windows 10 or later

### Linux

#### Arch Linux (AUR)
The easiest way to install on Arch Linux is via the AUR:
```bash
# Using an AUR helper like yay
yay -S ytaudiobar-git
```

#### Flatpak (Universal)
Run anywhere (Arch, Fedora, Ubuntu, etc.) with sandboxed security:
```bash
# Build and install locally
flatpak-builder --user --install --force-clean build-dir com.ytaudiobar.app.yml
```

#### AppImage
1. Download `YTAudioBar_*.AppImage` from [GitHub Releases](https://github.com/ilyassan/ytaudiobar/releases)
2. Make it executable: `chmod +x YTAudioBar_*.AppImage`
3. Run: `./YTAudioBar_*.AppImage`

#### Debian/Ubuntu (.deb)
Download the `.deb` package from the releases page and install:
```bash
sudo apt install ./YTAudioBar_*.deb
```

Minimum requirements: Ubuntu 22.04+, Arch Linux, or any distribution with WebKit2GTK support.

## Usage

### Basic Playback

1. Use the **Search** tab to find YouTube videos
2. Click a result to start playback
3. Use playback controls or media keys
4. Adjust volume and seek through the track

### Downloads

Switch to the **Downloads** tab to:

- View download progress
- Download tracks for offline playback
- Downloaded tracks support full seeking and faster playback

### Playlists

1. Click the **Playlists** tab
2. Create new playlists with the `+` button
3. Add tracks via the playlist icon during playback
4. Organize your music collection

### Settings

Access **Settings** tab to:

- Enable/disable auto-start on system boot
- Configure output device (if available)
- Adjust UI preferences

### Windows

- **OS:** Windows 10 or later (tested on Windows 11)
- **RAM:** 256 MB minimum
- **Disk Space:** ~5 MB for installation + 15 MB for runtime dependencies (downloaded on first launch)

### Linux

- **OS:** Ubuntu 22.04+ or equivalent distribution
- **RAM:** 256 MB minimum
- **Disk Space:** ~70 MB for AppImage + 15 MB for runtime dependencies (downloaded on first launch)
- **Dependencies:** libssl, libxcb (automatically handled by AppImage)

## Development

### Prerequisites

- Rust 1.70+ ([Install Rust](https://rustup.rs/))
- Node.js 16+ ([Install Node.js](https://nodejs.org/))
- Visual Studio Build Tools (Windows) or standard C compiler (Linux)

### Setup

```bash
# Clone the repository
git clone https://github.com/ilyassan/ytaudiobar.git
cd YTAudioBar-tauri

# Install dependencies
npm install

# Install Rust dependencies
cd src-tauri && cargo fetch && cd ..
```

### Development Build

```bash
# Run in development mode with hot reload
npm run tauri dev
```

### Production Build

```bash
# Build optimized release
npm run tauri build
```

### Type Checking

```bash
npx tsc --noEmit
```

### Technology Stack

**Frontend:**

- React
- TypeScript
- Tauri IPC
- TailwindCSS

**Backend:**

- Tauri 2.x
- Symphonia (audio decoding)
- rodio (audio output)
- SQLite
- reqwest
- yt-dlp
- FFmpeg

### Audio Pipeline

```
YouTube URL
    ↓
yt-dlp (extract stream)
    ↓
Symphonia (decode)
    ↓
rodio Sink (output)
```

### Playback Modes

- **Downloaded Tracks:** File-based playback with fast seeking (<100ms) using FLAC seek tables
- **Streamed Tracks:** Memory-buffered playback with seeking support (loads audio into memory)

## Project Structure

```
src/                               Frontend (React)
├── features/
│   ├── player/                   Player UI components
│   ├── search/                   Search functionality
│   ├── queue/                    Queue management
│   ├── playlists/                Playlist UI
│   ├── downloads/                Downloads UI
│   └── settings/                 Settings UI
├── stores/                       State management (Zustand)
├── lib/tauri.ts                  IPC bindings
└── app/routes/home.tsx           Main page

src-tauri/                        Backend (Rust)
├── src/
│   ├── main.rs                   App setup
│   ├── audio_manager.rs          Audio playback
│   ├── download_manager.rs       Downloads & FLAC conversion
│   ├── media_key_manager.rs      OS media controls
│   ├── database.rs               SQLite management
│   └── models.rs                 Data structures
└── Cargo.toml                    Rust dependencies
```

## Contributing

Contributions are welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.

## Credits

- **[ilyassan](https://github.com/ilyassan)** — Original creator and lead developer.
- **[A-007481D](https://github.com/A-007481D)** — Linux contributor (Universal Linux support, Arch Linux/AUR packaging, and Flatpak implementation).

## Related Projects

- [YTAudioBar-macos](https://github.com/ilyassan/YTAudioBar-macos) — Native macOS version
- [yt-dlp](https://github.com/yt-dlp/yt-dlp) — YouTube audio extraction
- [Tauri](https://tauri.app/) — Desktop app framework
- [Symphonia](https://github.com/pdeljanov/symphonia) — Audio decoding library
- [rodio](https://github.com/pdeljanov/rodio) — Audio output library

---

Made by Ilyass for the open source community
