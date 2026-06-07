# Clean Windows VM QA Checklist

Use this checklist before publishing a broad Windows release. Test the final artifacts downloaded from the draft GitHub release, not locally built files.

Record the Windows version, VM image source, artifact names, release tag, tester, and date in the release notes or QA log.

## Environment

- [ ] Clean Windows 10 or Windows 11 VM with no OrangeDL install already present.
- [ ] Standard non-developer user profile available.
- [ ] Network access available for GitHub and the chosen test download URLs.
- [ ] Microsoft WebView2 runtime present or installed by the normal Windows environment.

## Artifact Verification

- [ ] Download the NSIS installer and `.sha256` sidecar from the draft GitHub release.
- [ ] Run `Get-FileHash .\OrangeDL_*_x64-setup.exe -Algorithm SHA256`.
- [ ] Confirm the hash matches the `.sha256` sidecar exactly.
- [ ] Record whether Windows identifies the publisher as unknown or signed.

## Installer Behavior

- [ ] Launch the installer from the downloaded artifact.
- [ ] Record whether User Account Control appears.
- [ ] Record whether Microsoft Defender SmartScreen appears.
- [ ] Complete installation successfully.
- [ ] Confirm OrangeDL appears in the Start Menu.
- [ ] Confirm OrangeDL appears in Windows Apps uninstall list.
- [ ] Start OrangeDL from the Start Menu.

## Basic App Smoke Test

- [ ] App opens without a console window or startup error.
- [ ] First-run setup, if shown, can be completed.
- [ ] Add a plain HTTP or HTTPS download URL.
- [ ] Progress, speed, status, and destination path are visible.
- [ ] Pause the active download.
- [ ] Resume the paused download.
- [ ] Cancel a download and confirm the UI state is understandable.
- [ ] Retry a failed or cancelled download if the UI offers retry.
- [ ] Complete at least one download.
- [ ] Confirm the completed file exists at the expected destination.

## Persistence And Shell Integration

- [ ] Quit OrangeDL completely.
- [ ] Reopen OrangeDL and confirm history persists.
- [ ] Hide the window to tray and restore it.
- [ ] Test `orangedl://add?url=https%3A%2F%2Fexample.com%2Ffile.zip` and confirm it opens the add flow.
- [ ] If file open or reveal actions are exposed, confirm they work only for completed downloads and expected paths.

## Cleanup And Uninstall

- [ ] Use any visible cleanup actions and confirm their wording distinguishes history cleanup from file deletion.
- [ ] Uninstall OrangeDL through Windows Apps.
- [ ] Confirm app binaries and shortcuts are removed.
- [ ] Confirm downloaded user files are not removed by uninstall.

## Failure Notes

For each failure, record:

- Exact artifact name and release tag.
- Windows version.
- Steps to reproduce.
- Expected result.
- Actual result.
- Screenshot or log location, if available.
