# OrangeDL 1.1.0 Release Notes

## What's New

### Themes
Five built-in color themes are available from Settings → Appearance. Themes apply instantly and persist across restarts.

- **Creamsicle** — Pastel orange on warm cream, the OrangeDL classic (default)
- **Midnight Marmalade** — Dark cocoa with a bright orange glow
- **Peach Fizz** — Soft coral and peach, sweet and fizzy
- **Mint Squeeze** — Cool pastel mint with an orange twist
- **Bubblegum** — Playful pastel pink, extra cheerful

### Video Downloads (yt-dlp integration)
OrangeDL can now download from YouTube, Bilibili, and other platforms supported by [yt-dlp](https://github.com/yt-dlp/yt-dlp).

- Automatic yt-dlp detection and one-click install from within the app
- Thumbnail preview, title, uploader, and duration shown before confirming the download
- Quality selector: Best, 1080p, 4K, Audio only
- Output saved to the configured download folder

### Settings: Export and Import
Settings can now be exported to a JSON file and restored from one — useful for backups or syncing preferences across machines.

## Bug Fixes

- **Resume after remote change** — When a download resumes, OrangeDL now re-fetches the full file if the server's ETag or Last-Modified header changed. Previously, resuming against a replaced file would continue writing stale bytes into the existing partial download.
- **Progress update guard** — Progress events now only update downloads that are actively in the `downloading` state, preventing stale events from overwriting a terminal status.
- **Settings load performance** — App settings are now loaded in a single batched query instead of one query per key.

## Release Artifacts

| File | Description |
|---|---|
| `OrangeDL.exe` | Portable executable — run directly, no install needed |
| `OrangeDL.exe.sha256` | SHA-256 checksum for the portable exe |
| `OrangeDL_*_x64-setup.exe` | NSIS installer for a standard Windows installation |
| `OrangeDL_*_x64-setup.exe.sha256` | SHA-256 checksum for the installer |
| `orangedl-bootstrap-windows-x64.exe` | Windows bootstrapper |
| `orangedl-bootstrap-windows-x64.exe.sha256` | SHA-256 checksum |
| `orangedl-bootstrap-linux-x64` | Linux bootstrapper |
| `orangedl-bootstrap-linux-x64.sha256` | SHA-256 checksum |
| `orangedl-bootstrap-macos` | macOS bootstrapper |
| `orangedl-bootstrap-macos.sha256` | SHA-256 checksum |

Verify a download on Windows:
```powershell
(Get-FileHash OrangeDL.exe -Algorithm SHA256).Hash
```

Compare the output to the contents of `OrangeDL.exe.sha256`.

## Release Checklist

1. Build and verify the app and bootstrapper on Windows.
2. Publish the Git tag for `v1.1.0`.
3. Confirm the draft GitHub release contains the portable exe, installer, bootstrapper binaries, and all `.sha256` sidecar files.
4. Verify checksums before publishing.
5. Publish the release.
