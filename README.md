# OrangeDL

<p align="center">
  <em>A Windows-first desktop download manager focused on reliable transfers, resume support, and a clean Tauri-based workflow.</em>
</p>

## Why I made it

bad wifi
---

## Features

- Add downloads from HTTP or HTTPS URLs
- Run multiple downloads concurrently
- Pause, resume, cancel, and retry downloads
- Resume partial downloads with HTTP `Range` support when the server allows it
- Store download state in SQLite under the app data directory
- Use `.part` files until transfers complete
- Show live progress, speed, status, and ETA updates
- Search, filter, and review download history
- Apply optional per-download speed limits
- Drag and drop URLs into the downloads page
- Keep downloads running when the window is hidden to the tray

---

## Installation

Download the latest release from GitHub:

[![Download from GitHub](https://github.com/machiav3lli/oandbackupx/blob/034b226cea5c1b30eb4f6a6f313e4dadcbb0ece4/badge_github.png)](https://github.com/MisplacedOrange/OrangeDL/releases/latest)

OrangeDL stores app data in the platform-specific application data directory managed by Tauri. Download records and related metadata are persisted locally on the user's machine.

### Windows trust / signing

Unsigned GitHub-downloaded Windows builds may still show SmartScreen warnings depending on how the release is distributed and signed. If you publish Windows installers, make sure your release process and signature verification steps are documented for users.

### Privacy Policy

OrangeDL is a local desktop application. It does not operate a hosted OrangeDL account system or an OrangeDL-controlled cloud service.

- OrangeDL stores download records, status data, file paths, and related app settings locally on the user's device
- OrangeDL may connect to third-party servers only when the user provides a download URL or starts a download
- OrangeDL does not sell user data
- OrangeDL does not guarantee the privacy, availability, legality, or integrity of third-party content downloaded through the app
- Users are responsible for reviewing the sources they download from and for complying with applicable laws, license terms, and website policies

Full policy: [docs/privacy-policy.md](docs/privacy-policy.md)

### Terms and Conditions

By using OrangeDL, users agree that they are solely responsible for how they use the application.

- OrangeDL is provided as a general-purpose download tool
- I do not control, monitor, or direct what users download, share, copy, or distribute with the application
- I am not responsible for piracy, copyright infringement, unlawful distribution, or other illegal acts committed by users through or alongside the application
- Users must ensure their use of OrangeDL complies with local law and the rights of content owners
- The software is provided without warranty, subject to the GNU General Public License and the project terms

Full terms: [docs/terms-and-conditions.md](docs/terms-and-conditions.md)

---

## Building From Source

This project is currently Windows-first. The maintained desktop build path is Tauri v2 with a Rust backend and a React frontend.

Requirements:
- Node.js 18 or newer
- Rust stable toolchain
- Tauri v2 system prerequisites for your OS
- On Windows, Microsoft WebView2 runtime and Visual Studio Build Tools are typically required

Setup:

```powershell
git clone https://github.com/MisplacedOrange/OrangeDL.git
cd OrangeDL
npm ci
```

Run the app in development:

```powershell
npm run tauri dev
```

Build a production desktop app:

```powershell
npm run tauri build
```

Useful validation commands:

```powershell
npm run build
cd src-tauri
cargo fmt --all -- --check
cargo check --locked
cargo clippy --all-targets --locked -- -D warnings
```

---

## Support the project

You can support the project by starring the repository, reporting issues clearly, and sharing OrangeDL with people who need a desktop download manager.

## License

This project is licensed under the [GNU General Public License v3.0](LICENSE).
