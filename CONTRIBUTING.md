# Contributing to YTAudioBar

Thank you for your interest in contributing to YTAudioBar! This document provides guidelines and instructions for contributing to the project.

## Code of Conduct

Be respectful and inclusive. We're committed to providing a welcoming and inspiring community — see [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md) for the full policy.

## Getting Started

### 1. Fork and Clone

```bash
# Fork the repository on GitHub
# Clone your fork locally
git clone https://github.com/yourusername/ytaudiobar.git
cd YTAudioBar-tauri

# Add upstream remote
git remote add upstream https://github.com/ilyassan/ytaudiobar.git
```

### 2. Set Up Development Environment

#### Prerequisites

- **Rust** 1.70+: [Install Rust](https://rustup.rs/)
- **Node.js** 16+: [Install Node.js](https://nodejs.org/)
- **Visual Studio Build Tools** (Windows) or C compiler (Linux)

#### Install Dependencies

```bash
# Install Node dependencies
npm install

# Install Rust dependencies
cd src-tauri && cargo fetch && cd ..
```

### 3. Run Development Server

```bash
# Start dev server with hot reload
npm run tauri dev
```

## Making Changes

### Branch Naming

Create a descriptive branch from `main`:

```bash
git checkout -b feature/your-feature-name
# or
git checkout -b fix/your-bug-name
```

`main` is branch-protected: direct pushes are rejected for everyone
(including maintainers), so every change lands via a PR with CI passing.

One exception: **`release/x.y.z` branches never merge back into `main`.**
They exist to cut a patch release from a subset of already-merged commits
(cherry-picked via `git cherry-pick`) without pulling in whatever unreleased
work `main` has moved on to since. Tag the release from that branch, then
either delete the branch or leave it as a record — just never open a PR
from it into `main`, since the two are meant to diverge (see the
`release/2.5.1` branch for a real example: it backported 4 fixes onto the
`v2.5.0` tag while `main` kept moving toward `2.6.0`).

### Code Style

#### Rust

- Use `cargo fmt` for formatting: `cargo fmt --manifest-path src-tauri/Cargo.toml`
- Use `cargo clippy` for linting: `cargo clippy --manifest-path src-tauri/Cargo.toml`
- Follow Rust naming conventions (snake_case for functions/variables)
- Write meaningful variable and function names
- Add comments for non-obvious logic

#### TypeScript/React

- Use `npx prettier --write .` for formatting
- Use `npx eslint --fix .` for linting
- Follow camelCase for variables and functions
- Use PascalCase for components
- Write meaningful comments for complex logic
- Use TypeScript types (avoid `any`)

### Commit Messages

Write clear, concise commit messages:

```bash
# Good
git commit -m "Add media key seek support"
git commit -m "Fix playlist modal height glitch"

# Avoid
git commit -m "stuff"
git commit -m "minor fixes"
```

### Testing

Before submitting a PR:

1. **Build the app**: `npm run tauri build`
2. **Type check**: `npx tsc --noEmit`
3. **Test locally**: Run the app and verify your changes work
4. **Cross-platform**: If possible, test on both Windows and Linux

## Submitting Changes

### 1. Update Your Branch

```bash
# Fetch latest changes from upstream
git fetch upstream main

# Rebase on latest main
git rebase upstream/main

# If there are conflicts, resolve them and:
git add .
git rebase --continue
```

### 2. Push Your Changes

```bash
git push origin feature/your-feature-name
```

### 3. Create a Pull Request

1. Go to the original repository on GitHub
2. Click "New Pull Request"
3. Select your branch as the source
4. Fill in the title and description:
    - **Title**: Clear, short description (e.g., "Add seek support for streamed tracks")
    - **Description**: What does it do? Why? How to test?

GitHub pre-fills the PR description from
[`.github/PULL_REQUEST_TEMPLATE.md`](.github/PULL_REQUEST_TEMPLATE.md) —
fill it in rather than deleting it.

## Areas for Contribution

### 🎵 Audio/Playback

- Audio quality improvements
- New format support
- Seeking optimizations
- Performance improvements

### 🎨 UI/UX

- Design improvements
- Accessibility enhancements
- Mobile responsiveness (future)
- Dark/light theme support (future)

### 🔧 Backend

- Download improvements
- Playlist enhancements
- Database optimizations
- Error handling

### 📚 Documentation

- README improvements
- Code comments
- Architecture documentation
- Setup guides for developers

### 🧪 Quality

- Bug reports and fixes
- Performance optimization
- Platform-specific issues
- Testing improvements

## Debugging Tips

### Frontend Debugging

```bash
# Open dev tools in Tauri
npm run tauri dev
# Press F12 to open DevTools
```

### Backend Debugging

```bash
# Run with RUST_LOG for detailed logs
RUST_LOG=debug npm run tauri dev

# Check console output for backend logs (marked with 🎵 or 🔊)
```

### Common Issues

- **App won't start**: Clear `node_modules` and `src-tauri/target`, reinstall
- **Audio not working**: Ensure `yt-dlp` and `ffmpeg` are downloaded (check in home directory)
- **Playlist modal flickers**: This was fixed in v1.4.10 - update to latest

## Project Structure

```
src/                     # React frontend
├── features/           # Feature modules
├── stores/             # State management (Zustand)
├── lib/tauri.ts       # Tauri IPC bindings
└── components/        # Reusable components

src-tauri/              # Rust backend
├── src/
│   ├── audio_manager.rs    # Core audio playback
│   ├── download_manager.rs # Download & FLAC conversion
│   ├── media_key_manager.rs # OS media controls
│   ├── database.rs         # SQLite
│   └── main.rs            # App setup
└── Cargo.toml          # Rust dependencies
```

## Architecture Overview

### Audio Pipeline

1. YouTube search → yt-dlp extracts stream URL
2. Stream playback: `reqwest::get(url)` → Symphonia decoder → rodio Sink
3. Downloads: ffmpeg converts to FLAC → file-based playback
4. Seeking: FLAC seek tables enable <100ms jumps

### State Management

- **Frontend**: Zustand stores (player, playlist, download state)
- **Backend**: Arc<Mutex<>> for thread-safe state
- **IPC**: Tauri commands for frontend-backend communication

## Release Process

Only maintainers can publish releases. Update the version in `package.json`,
`src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json`, add an entry to
`src/lib/whats-new.ts` and `CHANGELOG.md` if the release has user-facing
changes, then:

```bash
# Tag a new version (any vX.Y.Z tag triggers the release workflow,
# regardless of which branch it points to)
git tag v1.5.0
git push origin v1.5.0

# GitHub Actions automatically builds and creates release
```

For a patch release that should exclude in-progress work on `main`, cut a
`release/x.y.z` branch from the last stable tag and cherry-pick only the
fixes you want — see the branch-naming note above.

## Questions?

- **Issues**: [GitHub Issues](https://github.com/ilyassan/ytaudiobar/issues)
- **Discussions**: [GitHub Discussions](https://github.com/ilyassan/ytaudiobar/discussions)
- **Tauri Docs**: [tauri.app](https://tauri.app)

## License

By contributing to YTAudioBar, you agree that your contributions will be licensed under the MIT License.

---

Thank you for making YTAudioBar better! 🎉
