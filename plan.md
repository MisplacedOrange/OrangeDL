# OrangeDL Plan

Last updated: 2026-06-06

## Current State

OrangeDL already has a working Windows-first desktop app with:

- Tauri v2 shell
- Rust download manager
- SQLite-backed history and settings
- Queue executor with bounded concurrency
- Pause, resume, cancel, retry, and checksum verification
- Tray support, deep links, first-run setup, and release/bootstrapper work

The priority is not adding more product surface. The priority is making the current downloader, release path, and installer flow trustworthy.

## Release Blockers

1. Align visible versions across `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, README, and release notes.
2. Keep the bootstrapper and release workflow green in CI.
3. Decide the Windows installer trust posture clearly: unsigned per-machine, signed per-machine, or a different packaging choice.
4. Verify the release process from a clean Windows VM before any broad release.

## Downloader Priorities

1. Add integration coverage for:
   - range resume
   - early EOF
   - redirects
   - retry behavior
   - cancellation
   - cleanup of `.part` files
   - same-filename contention
   - long filename handling
2. Fix remaining correctness risks:
   - validate `Content-Range` offsets on resume
   - resolve pause/cancel vs complete races
   - replace fixed shutdown sleeps with actual task joins
   - reconcile DB state vs filesystem state on startup
   - decide whether "global speed limit" is truly aggregate or should be renamed

## Packaging And Installer

The runnable file that downloads OrangeDL release artifacts is the bootstrapper built from:

- `bootstrapper/src/main.rs`

After building, the Windows executable is:

- `bootstrapper/target/release/orangedl-bootstrap.exe`

Build it with:

```powershell
cargo build --manifest-path bootstrapper/Cargo.toml --release --locked
```

Run it to download only:

```powershell
.\bootstrapper\target\release\orangedl-bootstrap.exe --download-only
```

Run it to download and launch the installer:

```powershell
.\bootstrapper\target\release\orangedl-bootstrap.exe
```

## Documentation

Keep:

- `docs/release.md`
- `docs/windows-vm-qa.md`

Do not keep handoff or agent-specific planning artifacts in the repo.

## Definition Of Done For Version 1

- App build passes.
- App Rust tests pass.
- App clippy passes.
- Bootstrapper tests and clippy pass.
- Release workflow targets the intended tag.
- README and release docs match actual behavior.
- Clean Windows VM QA passes using release artifacts.
