# OrangeDL

OrangeDL is a modern Tauri v2 desktop download manager with a Rust backend, React frontend, TailwindCSS styling, Tokio async downloads, Reqwest networking, SQLite persistence, `.part` temporary files, HTTP Range resume support, and a system tray workflow.

## Features

- Add downloads from HTTP or HTTPS URLs
- Run multiple downloads concurrently with Tokio tasks
- Stream file data through Reqwest and async file writes
- Emit real-time progress, speed, status, and ETA updates to React
- Pause, resume, cancel, retry, and persist downloads
- Store download state in SQLite under the app data directory
- Use `.part` files until a transfer completes
- Resume partial downloads with HTTP `Range` headers
- Keep transfers running when the main window is hidden
- Minimize to tray on window close and reopen from the tray
- Search, filter, and view download history
- Optional per-download speed limit
- Drag and drop URLs into the downloads page

## Project Structure

```text
.
├── package.json
├── src/
│   ├── App.tsx
│   ├── main.tsx
│   ├── index.css
│   ├── components/
│   ├── hooks/
│   ├── lib/
│   └── pages/
└── src-tauri/
    ├── Cargo.toml
    ├── tauri.conf.json
    ├── capabilities/
    └── src/
        ├── main.rs
        ├── lib.rs
        ├── commands/
        ├── database/
        ├── downloader/
        ├── models/
        └── tray/
```

## Major Files

- `src-tauri/src/downloader/mod.rs`: `DownloadManager`, Tokio task spawning, pause/resume/cancel control, retry logic, speed limiting, `.part` writes, and HTTP Range resume.
- `src-tauri/src/database/mod.rs`: SQLite connection, schema creation, download CRUD helpers, status updates, and app download-directory resolution.
- `src-tauri/src/models/mod.rs`: Shared Rust data models for downloads, status values, start requests, database rows, and summaries.
- `src-tauri/src/commands/mod.rs`: Tauri command boundary used by React through `invoke`.
- `src-tauri/src/tray/mod.rs`: System tray menu, left-click reopen behavior, and quit action.
- `src-tauri/src/lib.rs`: Tauri app setup, backend state injection, tray setup, close-to-tray handling, and command registration.
- `src-tauri/src/main.rs`: Native entry point.
- `src/lib/tauri.ts`: Typed frontend wrapper for Tauri commands and events.
- `src/hooks/useDownloads.ts`: React state manager for persisted downloads, event updates, and command actions.
- `src/hooks/useToasts.ts`: Toast notification state.
- `src/pages/DownloadsPage.tsx`: Main downloads view with search, filters, drag/drop URL handling, cards, and the add modal.
- `src/pages/SettingsPage.tsx`: Runtime settings and backend capability overview.
- `src/components/DownloadCard.tsx`: Download row card with progress, speed, ETA, status, and controls.
- `src/components/AddDownloadModal.tsx`: URL entry modal with filename override and speed-limit input.
- `src/index.css`: Dark futuristic shell, grid background, animation, scrollbar, and card polish.

## Requirements

- Node.js 18 or newer
- Rust stable toolchain
- Tauri v2 system prerequisites for your OS
- On Windows, Microsoft WebView2 runtime and Visual Studio Build Tools are typically required
- Docker, if you want to build Linux release artifacts from the provided container

## Setup

```bash
npm ci
npm run tauri dev
```

Build a production desktop app:

```bash
npm run tauri build
```

Run frontend-only checks:

```bash
npm run build
```

Run Rust checks:

```bash
cd src-tauri
cargo fmt --all -- --check
cargo check --locked
cargo clippy --all-targets --locked -- -D warnings
```

## Docker Build

The Dockerfile is a Linux release builder for OrangeDL. It installs Node.js, Rust, and the native Tauri/WebKit/GTK libraries required to compile the desktop app, then exports the built bundles.

```bash
docker build --target artifacts --output type=local,dest=release/docker .
```

The same command is available as:

```bash
npm run build:docker
```

The exported files are written under `release/docker/`:

- `bundle/`: Tauri Linux bundle output such as AppImage/deb/rpm files
- `orangedl-linux`: the raw Linux executable
- `bootstrapper/orangedl-bootstrap-linux`: the Linux release bootstrapper

Use build args when you need to advance toolchains later:

```bash
docker build --build-arg RUST_VERSION=1.82 --build-arg NODE_VERSION=20-bookworm --target artifacts --output type=local,dest=release/docker .
```

## v0.1 Release Flow

The app version is already set to `0.1.0` in `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json`.

1. Commit the release changes.
2. Create and push the semver tag:

```bash
git tag v0.1.0
git push origin v0.1.0
```

3. GitHub Actions runs `.github/workflows/release.yml`.
4. The workflow builds native Tauri bundles on Windows, macOS, and Linux.
5. The workflow also uploads small bootstrapper binaries:
   - `orangedl-bootstrap-windows-x64.exe`
   - `orangedl-bootstrap-macos`
   - `orangedl-bootstrap-linux-x64`
6. Review the generated draft release, then publish it.

For end users, the bootstrapper is the small file they download first. When run, it finds the matching OrangeDL installer on the `v0.1.0` GitHub release, downloads it to the temp directory, and launches it.

Advanced bootstrapper usage:

```bash
npm run build:bootstrapper
orangedl-bootstrap --tag v0.1.0
orangedl-bootstrap --download-only --output ./OrangeDL-installer
orangedl-bootstrap --asset-url https://github.com/MisplacedOrange/OrangeDL/releases/download/v0.1.0/<asset-name>
```

## Backend Notes

The backend creates a `DownloadManager` during Tauri setup and stores it as managed app state. Each download is saved in SQLite before it starts, then a Tokio task streams the response body to a `.part` file. Pause and cancel requests signal the task with a `CancellationToken`; resume starts a new task and uses the existing `.part` size as the next HTTP `Range` offset.

Progress updates are persisted and sent through an internal Tokio channel. A background event pump emits Tauri events:

- `download-progress`
- `download-finished`
- `download-status`

The React frontend listens to those events and updates cards without polling.

## Dependencies

Rust backend dependencies are declared in `src-tauri/Cargo.toml`:

- `tauri` v2 with `tray-icon`
- `tokio`
- `reqwest`
- `sqlx` with SQLite
- `tokio-util`
- `dashmap`
- `futures-util`
- `serde`
- `uuid`
- `url`
- `chrono`
- `thiserror`

Frontend dependencies are declared in `package.json`:

- React
- Vite
- TailwindCSS
- Tauri JS API
- clsx

## Behavior

Closing the main window hides it instead of quitting the process. Downloads continue because the Rust process and Tokio tasks remain alive. Use the tray icon or tray menu to reopen OrangeDL. Use the tray menu quit action to exit the app.
