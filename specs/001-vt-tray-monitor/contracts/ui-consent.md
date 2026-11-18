# Contract: Consent, Key Setup, and User Messages

**Date**: 2026-11-18

## Principles

- Uploads are always initiated by the user and confirmed per file. There is no “always allow” or
  “remember my choice” for uploads.
- The consent dialog defaults to Cancel. Escape cancels. Closing the dialog cancels.
- A check is not consent to upload; a consent dialog must never be bypassed by a check.
- Plain language, no marketing, no alarmism. State facts and consequences.
- All strings live in `src/ui/strings.rs`; no secrets, full paths, or raw API bodies in UI text.

## Upload consent dialog

**Title**: `Upload this file to VirusTotal?`

**Body fields**:
- File name: base name only.
- Size: human-readable (for example `4.2 MB`).
- SHA-256: full 64-character hash, selectable.
- Destination: `VirusTotal (www.virustotal.com)`.
- Notice: `The file content and its name will be sent to VirusTotal. Uploaded files are shared with
  the security community and may be accessed by VirusTotal partners.`
- Quota notice: `This uses at least 2 of your VirusTotal requests (upload and analysis checks).`
- Large-file notice when `size > 200 MiB`: `This is a large file. Upload may take a long time and
  consume significant bandwidth.`

**Buttons**: `Upload` (destructive/default visual emphasis), `Cancel` (default keyboard focus).

**Behavior**:
- Cancel returns without any upload row, quota reservation, or network call.
- Upload records `consent_at` and inserts a `pending` job. The upload worker reserves quota before
  each request.
- If quota is unavailable, the job stays `pending` (queued), the tray shows the reason and reset time,
  and the upload starts once quota allows. Consent is never asked again or lost; the user can cancel
  the queued job from Recent.
- No upload begins before the dialog returns Upload.

## First-run and key setup

**Title**: `Set up VirusTotal`

**Text**:
- `VT Tray needs a VirusTotal API key to check files online. Your key is stored in Windows
  Credential Manager and is never written to configuration files or logs.`
- `Without a key, folder monitoring and local fingerprinting still work; online checks are disabled.`
- Link/button: `Open VirusTotal API key page` (opens the browser only on explicit click).
- Field: masked password input, paste allowed, no reveal by default.
- Buttons: `Save and test`, `Cancel`.

**Validation**:
- Empty/whitespace key is rejected.
- A test request is optional but recommended; if it fails, keep the key only if the user chooses
  `Save anyway`, after showing the safe error category (invalid key, quota, network).
- Never display the key after saving. Only `Key saved`, `Key missing`, or `Key rejected`.

## Check result messages

- **Known**: `<name>: X of Y engines flagged this file.` plus `Open full report`.
- **Known clean**: `<name>: no engines flagged this file in the latest report.` plus `Open full
  report`.
- **Unknown**: `<name> is not in VirusTotal. You can upload it manually if you choose.` No upload
  starts from this message.
- **Stale**: `<name>: the last report is older than 7 days. Rescan or open the report.`
- **Error**: short category plus `Retry` when safe. Never include raw response bodies.

## Upload progress and completion

- Uploading: `Uploading <name>...` (no percentage unless cheaply available).
- Analyzing: `VirusTotal is analyzing <name>...`
- Completed: `Analysis complete: X of Y engines flagged <name>.` plus `Open report`.
- Failed: `Upload failed: <safe reason>. The file was not changed.` plus `Retry` only if the job is
  retryable.
- Canceled: `Upload canceled. The file was not changed.`

## Quota messages

- `VirusTotal limit reached (4 per minute). Try again in <n> seconds.`
- `Daily limit reached. Manual checks resume at <UTC time>.`
- `Monthly limit reached. Online checks are paused until <UTC date>.`
- `Automatic checks are paused to keep requests available for manual checks.`
- Quota messages never offer to bypass the limit.

## Destructive or sensitive actions

- Removing a watched folder: confirm; history is kept unless the user separately clears it.
- Clearing history: confirm; state that cached reports and detections are removed.
- Removing the API key: confirm; state that online checks stop immediately.
- Disabling autostart: no confirmation; show the new state.
- No action deletes, quarantines, or modifies a scanned file.

## Accessibility and localization

- Keyboard reachable; Cancel is the default focus in consent.
- Dialog text is selectable where the native dialog permits.
- English-only in v1; strings are centralized for future localization.
- Do not rely on color alone for verdicts; include text counts.
