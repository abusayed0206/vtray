# Quickstart: VT Tray

**Date**: 2026-11-18

## Prerequisites

- Windows 10/11 x64.
- Rust stable with the MSVC toolchain (`rustup default stable-msvc`).
- Visual Studio Build Tools with the Windows SDK (for `link.exe` and `rc.exe`).
- A VirusTotal account and API key for online checks. Keep the key out of the repository.
- No administrator rights are needed.

## Build and test

```powershell
cargo build
cargo test
cargo build --release
```

Expected: unit and integration tests pass without a real API key or live VirusTotal calls. The
release binary is at `target\release\vt-tray.exe`.

If `rc.exe` is missing, install the Windows SDK, or set `VTRAY_SKIP_RESOURCE=1` for a development
build without the embedded icon and manifest. Release builds should embed both.

## First run

1. Start `vt-tray.exe`. A tray icon appears; there is no main window.
2. Open the tray menu, then `Settings -> Set or replace API key...`.
3. Paste the key into the masked field and choose `Save key`.
4. Add a folder with `Add watched folder...`. Prefer a small test folder, not a volume root.
5. Leave `Automatic hash checks` off for the first run. Remove folders from the `Watched folders`
   submenu.

Data locations:
- Preferences: `%APPDATA%\VTTray\config.json`
- Cache/history: `%LOCALAPPDATA%\VTTray\cache.db`
- Logs: `%LOCALAPPDATA%\VTTray\logs`
- Credential: Windows Credential Manager, generic credential `api-key.VTTray`

## Manual smoke test

Use benign files only.

1. **Check a known file**: copy a harmless Windows executable into the watched folder. Confirm the
   tray notifies once, and that a manual `Check a file...` shows the result without uploading.
2. **Check an unknown file**: create a small text file with random content, use `Check a file...`,
   and confirm the unknown result. No upload occurs.
3. **Manual upload**: after the unknown result, confirm the consent dialog once by choosing No.
   Repeat the check and choose Yes; confirm upload progress, analysis, completion, and that the
   local file is unchanged.
4. **Large-file warning**: do not upload multi-hundred-MB files during routine testing; the
   consent dialog adds a warning above 200 MB.
5. **Quota**: trigger four rapid checks and confirm the fifth is refused locally with a reset time.
6. **Folder remove**: remove the watched folder and confirm new files no longer notify.
7. **Autostart**: toggle `Settings -> Start with Windows`, confirm the HKCU Run value, then toggle it off.
8. **Single instance**: run `vt-tray.exe --check "C:\path\to\file.txt"` twice; only one tray icon
   remains and each request is handled once.
9. **Explorer quick check**: enable `Settings -> Explorer context menu`, right-click an unknown file,
   choose `Show more options` → `Check with VTTray`. Expect "VirusTotal does not know this file ... Nothing was uploaded";
   no consent dialog appears (FR-020).
10. **Explorer restart**: restart `explorer.exe`; the tray icon returns.
11. **Memory**: leave the app idle for 10 minutes; confirm working set is at or below 25 MB.
12. **Remove from PC** (last, on a test setup): `Remove VTTray from this PC...`, choose Yes. Confirm
    the Run value, the Explorer entry, the `api-key.VTTray` credential, and `config.json` are gone.

## Mock API development

Integration tests use a local blocking mock server; see `tests/integration/mock_server.rs`.
It covers known/unknown lookups, 401/404/429, multipart framing, upload-URL flow, rescan, and
analysis polling. No live request is made by `cargo test`. The tray UI currently uses the fixed
VirusTotal base URL; use the automated tests for transport coverage.

## Validation commands

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Troubleshooting

- **Tray icon missing**: check `%LOCALAPPDATA%\VTTray\logs`; restart Explorer or the app. The app
  re-adds the icon on `TaskbarCreated`.
- **Key rejected**: confirm the key is active and the account is allowed to use the API. The app
  never displays the stored key.
- **429**: wait for the shown reset time; do not retry rapidly. The quota gate persists across
  restarts.
- **`rc.exe` not found**: install the Windows SDK or set `VTRAY_SKIP_RESOURCE=1` for development.
- **PowerShell scripts blocked**: Spec Kit scripts must be run with
  `powershell -NoProfile -ExecutionPolicy Bypass -File ...`; this is a development-only workaround.
- **Corporate TLS**: the default rustls roots may not trust an interception proxy; use the
  documented platform-verifier/native-tls build option if required.
- **Uninstall**: choose `Remove VTTray from this PC...` in the tray menu. It removes autostart, the
  Explorer entry, the saved key, and all data, then exits. Afterwards delete the program file and
  `%LOCALAPPDATA%\VTTray` (log files).

## Out of scope for v1

- Whole-volume or USN-journal monitoring.
- Automatic uploads.
- Quarantine, deletion, or file modification.
- A custom results window; full reports open in the browser.
- Installer/MSIX packaging and code signing.
- Non-English UI.

