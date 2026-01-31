# YTAudioBar

A lightweight, cross-platform YouTube audio player for Windows and Linux with system tray integration.

## Features

- 🎵 Stream YouTube audio directly
- 📥 Download tracks for offline listening
- 🎼 Queue management with shuffle and repeat modes
- ❤️ Favorites and playlist management
- 🎨 Clean, modern interface
- 💾 Lightweight and fast
- 🖥️ System tray integration
- 🌓 Dark mode support

## Technology Stack

- **Frontend**: HTML, CSS, JavaScript
- **Backend**: Rust + Tauri
- **Audio**: yt-dlp integration (planned)
- **Database**: SQLite (planned)

## Installation

### Prerequisites

- [Node.js](https://nodejs.org/) (v16 or higher)
- [Rust](https://www.rust-lang.org/tools/install)

### Setup

1. Clone the repository
2. Install dependencies:
```bash
npm install
```

3. Run in development mode:
```bash
npm run dev
```

4. Build for production:
```bash
npm run build
```

## Usage

- The app runs in the system tray
- Left-click the tray icon to show/hide the window
- The window automatically hides when clicking outside
- Right-click the tray icon for menu options (Show/Quit)

## Project Structure

```
YTAudioBar-tauri/
├── src/                    # Frontend files
│   ├── index.html         # Main HTML
│   ├── styles.css         # Application styles
│   └── main.js            # Frontend logic
├── src-tauri/             # Tauri backend
│   ├── src/
│   │   └── main.rs        # Rust main file
│   ├── Cargo.toml         # Rust dependencies
│   └── tauri.conf.json    # Tauri configuration
└── package.json
```

## Planned Features

- YouTube search and streaming
- Audio playback engine
- Download manager with progress tracking
- Playlist persistence
- Media key support
- Cross-platform support (Windows, Linux)

## License

MIT

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
