# OrangeDL — Product Requirements Document

**Version:** 1.0  
**Author:** Roy Lu (MisplacedOrange)  
**License:** GPL-3.0-only  
**Status:** Released (v0.1.0)  
**Last updated:** 2026-06-11

---

## 1. Overview

OrangeDL is an open-source desktop download manager built with [Tauri v2](https://tauri.app) (Rust backend, React/TypeScript frontend). It provides reliable, resumable HTTP/HTTPS file transfers with an executor-managed queue, per-download throttling, SHA-256 integrity verification, and a multi-theme UI.

### Goals

- Provide a fast, native-feeling download manager for Windows with a polished UI.
- Support concurrent, prioritised, resumable HTTP/HTTPS downloads out of the box.
- Be fully open source (GPL-3.0) with no telemetry, no accounts, and no cloud dependency.
- Remain lightweight — single binary, SQLite-backed state, no Electron overhead.

### Non-Goals

- FTP, BitTorrent, or Magnet-link support.
- Browser extension integration (handled via the `orangedl://` deep-link scheme).
- Multi-account or cloud-sync features.
- Automatic silent updates (update checks inform only; installation is always user-initiated).

---

## 2. Architecture

### 2.1 Technology Stack

| Layer | Technology |
|-------|-----------|
| Frontend | React 18, TypeScript, Vite, Tailwind CSS, clsx |
| Backend | Rust 2021 edition, Tauri v2 |
| Storage | SQLite (via `sqlx` with WAL journal mode) |
| HTTP client | `reqwest` with `rustls-tls`, brotli/gzip/deflate |
| Async runtime | Tokio (multi-thread) |
| IPC | Tauri typed commands + event emitter |
| Packaging | NSIS installer (per-machine, Windows) |

### 2.2 Backend Modules

| Module | Responsibility |
|--------|---------------|
| `lib.rs` | App bootstrap, Tauri plugin registration, single-instance guard, deep-link handler, graceful close |
| `models/mod.rs` | Shared data types: `Download`, `AppSettings`, `DownloadStatus`, request/result DTOs |
| `database/mod.rs` | SQLite schema migrations, all CRUD operations, settings key-value store |
| `downloader/mod.rs` | `DownloadManager` — HTTP download engine, ETag-based resume validation, speed throttling, SHA-256 hashing, retry with exponential backoff |
| `executor/mod.rs` | `DownloadExecutor` — bounded concurrency loop, queue draining, tray tooltip updates |
| `commands/mod.rs` | Tauri command handlers (thin wrappers over `DownloadManager`) |
| `tray/mod.rs` | System tray icon, context menu (show, add, pause all, resume all, quit) |

### 2.3 Frontend Modules

| Module | Responsibility |
|--------|---------------|
| `App.tsx` | Root layout, page routing (Downloads / Settings), theme application |
| `pages/DownloadsPage.tsx` | Download list, toolbar, queue controls, virtual-list rendering, drag-and-drop reorder, import/export |
| `pages/SettingsPage.tsx` | Settings form, theme picker, update check, settings import/export |
| `components/AddDownloadModal.tsx` | New download form with preflight check, multi-URL paste, queue-position and priority controls |
| `components/DownloadCard.tsx` | Single download row with expandable detail, inline options (speed limit, priority), checksum panel |
| `components/FirstRunSetup.tsx` | First-launch wizard |
| `components/ToastViewport.tsx` | Notification toast stack |
| `hooks/useDownloads.ts` | All download state, event listeners, API calls |
| `hooks/useToasts.ts` | Toast queue management |
| `lib/tauri.ts` | Typed wrappers for `invoke` and `listen` |
| `lib/types.ts` | TypeScript mirrors of Rust DTOs |
| `lib/themes.ts` | Theme definitions, `applyTheme`, `applyStoredTheme` |
| `lib/format.ts` | `formatBytes`, `formatSpeed`, `formatEta`, `statusLabel`, `progressValue` |

---

## 3. Feature Inventory

### 3.1 Download Engine

| Feature | Status | Notes |
|---------|--------|-------|
| HTTP/HTTPS downloads | Shipped | `reqwest` with `rustls-tls` |
| Range-request resume | Shipped | ETag + Last-Modified validator invalidation on resource change |
| Exponential backoff retry | Shipped | 3 retries, 2–120 s delay with nanosecond jitter |
| Per-download speed cap | Shipped | Token-bucket throttle at chunk loop |
| Global speed cap | Shipped | Takes minimum of per-download and global limits |
| SHA-256 integrity check | Shipped | On-demand from expanded row |
| HTTPS→HTTP redirect guard | Shipped | Refuses downgrades; enforced in custom `reqwest` policy |
| Content-Disposition filename | Shipped | `filename*=UTF-8''…` (RFC 5987) and plain `filename=` |
| Concurrent queue executor | Shipped | Configurable 1–N slots, notified on slot change |
| Priority queue ordering | Shipped | `priority DESC, queue_position ASC, created_at ASC` |
| Drag-and-drop queue reorder | Shipped | Queued items only |
| File-name deduplication | Shipped | `reserve_unique_destination` with TOCTOU guard via `.part` claim |
| Windows reserved-name guard | Shipped | `CON`, `PRN`, `NUL`, `COM1–9`, `LPT1–9` get `_` prefix |
| Compound extension preservation | Shipped | `.tar.gz`, `.tar.bz2`, `.tar.xz`, etc. |
| Max filename length | Shipped | 180 characters, extension preserved on truncation |
| Move completed download | Shipped | Backend command; no dedicated UI button yet |
| Rename completed download | Shipped | Backend command; no dedicated UI button yet |
| Deep-link (`orangedl://`) | Shipped | `?url=` query param opens Add modal |
| Single-instance guard | Shipped | Raises existing window on second launch |
| Graceful shutdown | Shipped | Pauses active downloads before `app.exit(0)` |
| Close-to-tray | Shipped | Configurable; defaults on |
| Auto-resume on launch | Shipped | Re-queues interrupted downloads if enabled |

### 3.2 UI / UX

| Feature | Status |
|---------|--------|
| Multi-theme support (5 themes) | Shipped |
| Theme flash-free cold start | Shipped (localStorage pre-apply) |
| Virtual list (>300 rows) | Shipped |
| Deferred search | Shipped (`useDeferredValue`) |
| All / Active / History filter | Shipped |
| Drag-and-drop URL onto window | Shipped |
| Clipboard URL auto-detect | Shipped (modal open) |
| Preflight server check | Shipped (HEAD request, debounced 800 ms) |
| Multi-URL batch add | Shipped |
| JSON/TXT import | Shipped |
| JSON/CSV export | Shipped |
| Settings import/export | Shipped |
| Keyboard shortcuts (Ctrl+N, Ctrl+F, Ctrl+,) | Shipped |
| First-run setup wizard | Shipped |
| Offline status indicator | Shipped |
| Native notifications (complete/fail) | Shipped |
| System tray with queue controls | Shipped |
| Update check (GitHub Releases API) | Shipped |

### 3.3 Known Gaps / Backlog

| Item | Priority | Notes |
|------|----------|-------|
| Move/Rename UI in DownloadCard | High | Backend commands exist; UI buttons missing |
| `backgroundUpdateNotifications` active polling | Medium | Setting is stored but executor never polls GitHub in background |
| macOS / Linux builds | Medium | Tauri supports; not yet tested or bundled |
| Browser extension / native messaging | Low | Deep-link scheme is the current integration path |
| Segment / multi-connection downloads | Low | Would require significant downloader changes |
| Proxy support | Low | `reqwest` supports; no UI or setting exists |
| Download scheduling (start-at time) | Low | Not planned for v1 |
| FTP support | Out of scope | |

---

## 4. Data Model

### 4.1 `downloads` Table

| Column | Type | Notes |
|--------|------|-------|
| `id` | TEXT PK | UUID v4 |
| `url` | TEXT | Original request URL |
| `file_name` | TEXT | Display name (may differ from URL path) |
| `destination` | TEXT | Final absolute path |
| `temp_path` | TEXT | `destination + ".part"` while downloading |
| `total_bytes` | INTEGER? | `NULL` if server did not send `Content-Length` |
| `downloaded_bytes` | INTEGER | Bytes written to temp file |
| `status` | TEXT | `queued \| downloading \| paused \| completed \| failed \| cancelled` |
| `speed_bps` | REAL | Last-reported bytes/second (0 when not active) |
| `error` | TEXT? | Last error message |
| `created_at` | TEXT | RFC 3339 UTC |
| `updated_at` | TEXT | RFC 3339 UTC |
| `speed_limit_bps` | INTEGER? | Per-download cap; `NULL` = no cap |
| `priority` | INTEGER | Higher = first in queue |
| `queue_position` | INTEGER? | User-set manual position |
| `queued_at` | TEXT? | When first queued |
| `started_at` | TEXT? | When first started |
| `completed_at` | TEXT? | When completed |
| `retry_count` | INTEGER | Number of retry attempts so far |
| `max_retries` | INTEGER | Default 3 |
| `next_retry_at` | TEXT? | RFC 3339 of scheduled retry |
| `last_error_kind` | TEXT? | Reserved |
| `source_host` | TEXT? | Reserved |
| `etag` | TEXT? | ETag stored for resume validation |
| `last_modified` | TEXT? | Last-Modified stored for resume validation |
| `checksum_sha256` | TEXT? | Expected or last-computed SHA-256 |

### 4.2 `settings` Table

Key-value store. All settings are stored as text and parsed at read time.

| Key | Default | Notes |
|-----|---------|-------|
| `default_download_directory` | System Downloads | Created on save if absent |
| `default_speed_limit_bps` | none | Per-download default |
| `global_speed_limit_bps` | none | Applies to all active downloads |
| `max_concurrent_downloads` | 3 | Minimum 1 |
| `auto_resume_interrupted_downloads` | false | Re-queues on launch |
| `close_to_tray` | true | Hides window instead of quitting |
| `notifications_enabled` | true | Native OS notifications |
| `notification_sound` | false | Sound with notifications |
| `background_update_notifications` | false | Not yet implemented |
| `auto_open_folder_on_completion` | false | Reveals file in Explorer |
| `history_retention_days` | unlimited | Auto-cleanup on settings save |
| `history_max_rows` | unlimited | Auto-cleanup on settings save |
| `first_run_completed` | false | Controls first-run wizard |
| `theme` | creamsicle | Validated against known theme slugs |

---

## 5. Security Model

### 5.1 URL Validation

All URLs are validated before enqueue and before each download attempt:
- Only `http` and `https` schemes accepted.
- Host must be present.
- Embedded credentials (`user:pass@host`) are rejected.
- Redirects are followed up to 10 hops; HTTPS→HTTP downgrades are refused.

### 5.2 Content Security Policy

```
default-src 'self';
base-uri 'none';
object-src 'none';
frame-ancestors 'none';
script-src 'self';
style-src 'self' 'unsafe-inline';
img-src 'self' asset: http://asset.localhost;
connect-src ipc: http://ipc.localhost https://ipc.localhost http://127.0.0.1:1420 ws://127.0.0.1:1420
```

`unsafe-inline` for styles is required by Tailwind CSS utility classes.

### 5.3 File System

- Downloads land in a user-selected directory; no access to arbitrary FS paths from the frontend.
- Temporary `.part` files are created atomically to guard against concurrent starts claiming the same name.
- File names are sanitised (allowed: `A-Za-z0-9 ._-`; everything else → `_`).
- Windows reserved device names (`CON`, `NUL`, `COM1-9`, `LPT1-9`) are prefixed with `_`.
- File names are capped at 180 characters.

### 5.4 Update Check

The update check makes a single `GET` request to the GitHub Releases API from the Rust backend. No data about the user or system is sent. The response is parsed for `tag_name` and `html_url` only. No automatic installation occurs.

---

## 6. Licensing

OrangeDL is distributed under the **GNU General Public License v3.0 only** (GPL-3.0-only).

All first-party source files carry the SPDX identifier:
```
// SPDX-FileCopyrightText: 2025 MisplacedOrange
// SPDX-License-Identifier: GPL-3.0-only
```

Third-party Rust dependencies are declared in `Cargo.toml` with their respective SPDX identifiers as published to crates.io. All npm frontend dependencies use MIT or Apache-2.0 compatible licenses. Full dependency license inventory can be generated with `cargo license` and `license-checker`.

---

## 7. Quality Assurance

### 7.1 Automated Tests

| Suite | Location | Coverage |
|-------|----------|---------|
| DB migration idempotency | `database/mod.rs #[cfg(test)]` | SQLite in-memory |
| Download CRUD round-trips | `database/mod.rs #[cfg(test)]` | Insert, get, set_status, priority ordering, bulk pause, settings |
| File-name sanitisation | `downloader/mod.rs #[cfg(test)]` | 20 cases: reserved names, truncation, compound extensions, percent-decode |
| URL validation | `downloader/mod.rs #[cfg(test)]` | Scheme rejection, credential rejection |
| ETA computation | `downloader/mod.rs #[cfg(test)]` | Correct, done, slow, unknown total |
| Content-Disposition parsing | `downloader/mod.rs #[cfg(test)]` | RFC 5987 star, plain, missing |
| Backoff delay bounds | `downloader/mod.rs #[cfg(test)]` | 10 attempt sweep, 2 s min, 120 s cap |
| Theme normalisation | `downloader/mod.rs #[cfg(test)]` | Valid slugs, invalid fallback |
| Concurrent destination claim | `downloader/mod.rs #[cfg(test)]` | Two goroutines race for same filename |

### 7.2 Manual QA Checklist

See `docs/windows-vm-qa.md` for the Windows-specific manual test matrix.

---

## 8. Build & Release

| Step | Command |
|------|---------|
| Frontend dev server | `npm run dev` |
| TypeScript type-check | `npx tsc --noEmit` |
| Production build | `npm run build` |
| Tauri dev mode | `npm run tauri dev` |
| Tauri release build | `npm run tauri build` |
| Output | `src-tauri/target/release/bundle/nsis/OrangeDL_*.exe` |

Release profile settings (`Cargo.toml`):
- `codegen-units = 1`, `lto = true`, `opt-level = "s"`, `strip = true`
- `panic = "abort"` intentionally **not** set: task panics should fail the download, not crash the app.

---

## 9. Open Questions / Future Work

1. **Move/Rename UI** — `move_download` and `rename_download` Tauri commands are implemented and tested; a context-menu or detail-panel button is needed.
2. **Background update polling** — `backgroundUpdateNotifications` setting is stored but the executor does not yet periodically call `check_for_updates`.
3. **macOS/Linux** — The Tauri stack supports both; NSIS is Windows-only. Cross-platform bundle targets and platform-specific path defaults need verification.
4. **Proxy configuration** — `reqwest` supports HTTP/SOCKS proxies; no UI or settings key exists.
5. **CHANGELOG** — A `CHANGELOG.md` following [Keep a Changelog](https://keepachangelog.com) format should be added before the first public release.
