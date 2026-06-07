# OrangeDL Release Guide

This guide documents the Windows-first release path for OrangeDL. It is intentionally conservative: do not present a build as broadly ready until the release blockers in `plan.md` are resolved and the clean Windows VM checklist passes.

## Current Artifact Posture

- OrangeDL is Windows-first. Other platform artifacts should be treated as experimental unless a release explicitly says otherwise.
- The desktop installer is an NSIS Windows installer.
- The current NSIS configuration is per-machine, so installing normally requires administrator approval through User Account Control.
- Windows artifacts are unsigned unless the release notes explicitly say they are Authenticode-signed.
- Unsigned installers may trigger Microsoft Defender SmartScreen or "Unknown publisher" warnings.
- SHA-256 sidecars are used for integrity verification. They confirm that a downloaded file matches the published artifact, but they do not prove publisher identity and do not replace code signing.
- The app currently supports HTTP and HTTPS downloads only. Do not imply support for torrents, browser extensions, video extraction, scheduling, cloud sync, credential/cookie-authenticated downloads, or other protocols.

## Prerequisites

- Node.js 18 or newer
- Rust stable
- Tauri v2 Windows prerequisites
- A clean working tree
- A version tag such as `vX.Y.Z`
- Matching versions in `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json`

## Release Readiness Gate

Do not publish a broad Windows release until these are true:

- App build, Rust app tests, and Rust app clippy pass.
- Bootstrapper build, tests, and clippy pass. (Note: bootstrapper clippy currently fails at `bootstrapper/src/main.rs:257` — this must be fixed before release.)
- The release workflow is known to publish against the intended tag for both tag pushes and manual dispatch.
- Version numbers in package metadata, Tauri config, README, and release notes agree.
- Clean Windows VM QA passes using the final artifacts. See `docs/qa-checklist.md`.
- Release notes clearly state unsigned/per-machine installer behavior and current product limitations.
- Notification sound and background update notification toggles are removed from the Settings UI (they are stored but do nothing).
- Global speed limit is either aggregate-enforced or renamed to reflect that it applies per-transfer.

## Local Validation

Run these before tagging:

```powershell
npm ci
npm run build
cargo fmt --all --manifest-path src-tauri/Cargo.toml -- --check
cargo check --manifest-path src-tauri/Cargo.toml --locked
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

For a manual smoke test, start OrangeDL, add an HTTP or HTTPS URL, pause it, resume it, let it complete, quit, reopen the app, and confirm the history row is still present.

## GitHub Release Build

1. Update versions in `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json`.
2. Commit the release changes.
3. Create and push a version tag:

```powershell
git tag vX.Y.Z
git push origin vX.Y.Z
```

4. Wait for the `Release` workflow to finish.
5. Inspect the draft GitHub Release.
6. Confirm it contains the NSIS installer, bootstrapper binaries, and `.sha256` files.
7. Download the artifacts from the draft release and verify checksums locally.
8. Run clean Windows VM QA.
9. Publish the release after verification.

## Artifact Verification

Verify installer checksums before publishing and document the expected checksum in the GitHub release body:

```powershell
Get-FileHash .\OrangeDL_*_x64-setup.exe -Algorithm SHA256
Get-Content .\OrangeDL_*_x64-setup.exe.sha256
```

The hash from `Get-FileHash` must match the checksum sidecar exactly.

Recommended user-facing checksum wording:

> Verify the SHA-256 checksum before installing. The checksum confirms file integrity against this release asset, but it does not replace publisher signing.

## Bootstrapper Validation

Run:

```powershell
.\orangedl-bootstrap-windows-x64.exe --download-only
.\orangedl-bootstrap-windows-x64.exe --tag vX.Y.Z --download-only
```

The bootstrapper should select the Windows x64 NSIS installer, verify the `.sha256` sidecar when present, and fail closed if verification does not match. If `--asset-url` is documented for power users, state that it trusts the specified HTTPS asset source and is not the normal release path.

## Clean Windows VM QA

Before broad release, validate the final release artifacts on a clean Windows machine. Use [windows-vm-qa.md](windows-vm-qa.md) as the checklist.

Minimum smoke coverage:

- Checksum verification before install.
- UAC and SmartScreen behavior recorded.
- Installer launches and completes.
- OrangeDL appears in Start Menu and Windows Apps uninstall list.
- `orangedl://add?url=https%3A%2F%2Fexample.com%2Ffile.zip` opens the add flow.
- Add, pause, resume, cancel, retry, and complete downloads.
- Quit and reopen; history survives restart.
- Uninstall removes binaries and shortcuts without deleting downloaded files.

## Signing

Public Windows releases should be Authenticode-signed with a timestamp server before broad distribution. Until signing is configured, release notes must clearly state that Windows may show SmartScreen warnings and User Account Control prompts.

Do not describe unsigned installers as "trusted", "verified", or "safe" based only on checksum sidecars.

## Release Notes Language

Use direct language in each Windows release:

```text
Windows installer status: unsigned, per-machine NSIS installer.

This build may show Microsoft Defender SmartScreen or "Unknown publisher" warnings because the installer is not Authenticode-signed. Installing may require administrator approval through User Account Control.

Verify the SHA-256 checksum published with this release before installing. Checksums confirm artifact integrity, not publisher identity.

Known limitations: OrangeDL currently supports HTTP and HTTPS downloads only. Resume support depends on the remote server. Browser extensions, torrents, video extraction, credential/cookie-authenticated downloads, scheduling, and cloud sync are not supported in this release. The global speed limit applies per transfer, not in aggregate across all active transfers. Each download attempt has a 24-hour streaming timeout; slow connections on large files will retry and resume automatically without data loss.
```
