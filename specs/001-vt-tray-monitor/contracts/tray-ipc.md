# Contract: Single Instance and Tray IPC

**Date**: 2026-11-18

**Scope**: Windows per-user session only.

## Names

| Item | Value |
|------|-------|
| Mutex | `Local\VTTray.SingleInstance` |
| Hidden window class | `VTTrayHiddenWindow` |
| Hidden window title | `VTTray` |
| IPC message | `WM_COPYDATA` |
| `COPYDATASTRUCT.dwData` | `1` (check request) |
| Credential target | `VTTray/api-key` |
| Run key | `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\VTTray` |
| Context-menu verb | `HKCU\Software\Classes\*\shell\VTTrayCheck` |

## Command line

```
vtray.exe
vtray.exe --check <absolute-file-path>
```

- No other public flags in v1. Unknown flags exit with code 2 and a message in the log, not a console.
- `--check` is hash-only. It must never upload, never open a consent dialog, and never prompt for the
  API key. Without a key it fingerprints locally and shows `Key missing`; no request is sent.
- The path is resolved to an absolute path; directories, reparse points, and missing files are
  rejected with a tray notification.

## Startup

1. Create/open the named mutex.
2. If it already exists, this is a secondary process:
   - Wait up to 3 seconds for the hidden window (`FindWindowW`).
   - If found, send the check payload and exit 0.
   - If not found, write the canonical path to the one-shot pending file and exit 0. Never launch
     another process: the mutex owner is already starting (or hung) and consumes the file.
3. If it does not exist, become the primary:
   - Register the hidden window class and create the hidden window.
   - Add the tray icon.
   - After the hidden window exists, read and delete the one-shot pending file, if present, and read
     it once more 5 seconds later to catch a secondary that wrote it after the first read. A
     duplicate check is harmless: it is hash-only and cache-first.
   - Process any startup check.
4. On normal exit, remove the tray icon, destroy the window, release the mutex, and remove the
   pending file.

## WM_COPYDATA payload

- Sender allocates a UTF-16LE, NUL-terminated absolute path in its own process.
- `cbData` is the byte length including the terminating NUL, at most 65,536 bytes.
- `lpData` points to the path buffer; `dwData == 1`.
- Receiver:
  1. Reject if `cbData < 2` or `cbData > 65,536` or not even.
  2. Reject if the final UTF-16 code unit is not NUL.
  3. Copy into a local `Vec<u16>`, validate UTF-16, and convert to `PathBuf`.
  4. Reject relative paths, directories, and reparse points.
  5. Return `TRUE` if accepted, `FALSE` otherwise.
- The receiver never dereferences the sender pointer after returning; the copy is mandatory.
- Window messages cannot cross Windows sessions, so no extra session check is needed.

## Pending check file

- Path: `%LOCALAPPDATA%\VTTray\pending-check.txt`.
- Contains one UTF-8 absolute path, no BOM, at most 32,768 characters.
- Written atomically: write to `pending-check.tmp` in the same directory, flush, then rename.
- Read once at primary startup, delete immediately, then validate and enqueue.
- Malformed or missing file is ignored and deleted; no dialog.
- The file is local-only and never contains a credential or file content.

## User-visible behavior

- A secondary `--check` handoff produces one balloon: checking, then the verdict or an error.
- If the primary does not answer within 3 seconds, the secondary writes the pending file and
  exits; it never starts a second tray icon.
- Double-clicking the executable with no arguments when an instance exists simply brings the tray
  menu up (or shows a balloon if the menu cannot be shown programmatically).
- `--check` never creates a console window; the binary uses `windows_subsystem = "windows"` in
  release builds.

## Security and limits

- The mutex is per-session (`Local\`), not global; no cross-user IPC.
- No network listener, named pipe, or COM server is exposed in v1.
- The payload is bounded and validated; malformed messages are dropped.
- The receiver must not trust `cbData`; it copies exactly the bounded bytes and NUL-terminates.
- A failed handoff never deletes or modifies the target file.

## Test cases

- First instance starts; second instance exits and sends a check.
- Second instance starts while the first is still initializing (fallback path).
- Payload with relative path, directory, missing file, reparse point, invalid UTF-16, missing NUL,
  and oversized buffer.
- Pending file written after the primary's first read is picked up by the 5-second re-read.
- Explorer restart re-adds the tray icon through `TaskbarCreated`.
- Pending file is consumed once and removed; malformed file is ignored.
