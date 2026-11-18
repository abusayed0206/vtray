# Research: VT Tray - Always-On VirusTotal Companion

**Date**: 2026-11-18

**Status**: Complete; all technical unknowns resolved. No clarification markers remain.

## Decisions

### 1. Tray and shell integration: native Win32 via `windows`

- **Decision**: Use `windows` 0.62.2 directly for a hidden top-level window, message loop,
  `Shell_NotifyIconW`, `CreatePopupMenu`/`TrackPopupMenu`, `NIF_INFO` balloons, `TaskbarCreated`,
  `CreateMutexW`, and `WM_COPYDATA`. Do not add `tray-icon`, `winit`, `tao`, Tauri, or a GUI
  framework in v1.
- **Rationale**: This is a Windows-only, tray-first app. Native Win32 has the smallest runtime and
  dependency footprint, gives direct control of shell quirks, and avoids an event-loop dependency.
  `windows` is Microsoft's current Rust projection (0.62.2, October 2025) and is actively maintained.
- **Alternatives considered**:
  - `tray-icon` 0.26.1 + `tao` 0.37.2: current and easier, but adds an event loop and several
    dependencies for a tray-only UI. Acceptable fallback if native menu code becomes a schedule risk.
  - `winit` 0.31: currently a beta (0.30.13 is stable); avoid a beta foundation.
  - Tauri/WebView2 and egui/eframe: rejected for memory use and unnecessary rendering surface.
- **Consequences**: More `unsafe` Win32 plumbing, but a small, testable shell adapter. Must handle
  `TaskbarCreated`, DPI, menu foreground-window rules, and icon cleanup carefully.

### 2. HTTP client: blocking `ureq`

- **Decision**: Use `ureq` 3.5.2 with default rustls/gzip plus the `json` feature. A single
  `ureq::Agent` is owned by one API worker thread. No Tokio, `reqwest`, or async runtime.
- **Rationale**: The workload is low-volume, serialized, and quota-limited. Blocking I/O keeps the
  dependency tree, binary size, and idle memory small. `ureq` 3.5.2 is current (November 2026),
  supports rustls, proxies, timeouts, and JSON.
- **Alternatives considered**:
  - `reqwest` 0.13.5 + Tokio 1.54.1: capable but unnecessary runtime and dependency weight.
  - `native-tls` backend: only if OS certificate-store integration is required; default rustls is
    sufficient for VirusTotal. Enable `platform-verifier` only if a deployment needs OS roots.
- **Consequences**: Manual multipart framing (below). TLS roots are static `webpki-roots`; document
  that corporate TLS interception may require the native-tls or platform-verifier build option.

### 3. File watching: `notify` 8.2.2 with `notify-debouncer-full` 0.7.0

- **Decision**: Use the stable `notify` 8.2.2 and `notify-debouncer-full` 0.7.0. Watch only
  user-selected folders recursively. Debounce, then require stable size and modification time before
  fingerprinting.
- **Rationale**: `notify` uses `ReadDirectoryChangesW` on Windows. The full debouncer handles
  create/rename/write bursts and editor/installer patterns better than a naive event handler.
  `notify` 9.0.0-rc.9 exists but is a release candidate; avoid it for a safety tool.
- **Alternatives considered**:
  - Direct `ReadDirectoryChangesW`: more control but reimplements buffering, overflow recovery, and
    rename pairing.
  - Whole-volume NTFS USN journal: deferred; requires administrator access and journal-wrap logic.
  - Periodic full-folder scans: higher I/O and slower detection; use only as a recovery fallback.
- **Consequences**: Must handle watcher errors/buffer overflow by rebuilding the watcher; network
  shares may not deliver reliable events, so validate and warn.

### 4. Local state: SQLite via `rusqlite`

- **Decision**: Use `rusqlite` 0.43.0 with the `bundled` feature and a bounded cache
  (`PRAGMA cache_size=-2000`, `PRAGMA mmap_size=0`, WAL). Store fingerprint cache, path cache,
  detections, upload jobs, and quota events. Prune by age and row count.
- **Rationale**: The app needs transactional queue/quota updates and simple queries for recent
  history. Bundled SQLite avoids a system dependency; a 2 MB page cache keeps memory predictable.
  `rusqlite` 0.43.0 is current (November 2026).
- **Alternatives considered**:
  - `redb` 4.6.0: pure Rust and attractive, but query/reporting code is more manual for quota and
    history.
  - JSON files: simpler but weak for concurrent workers, crash safety, and pruning.
- **Consequences**: Requires an MSVC C toolchain for the bundled build; document this. Use one
  connection per worker or a small mutex-protected connection; keep transactions short.

### 5. Credential storage: Windows Credential Manager via `keyring`

- **Decision**: Use `keyring` 4.2.0 with its default `v1` mode, which includes the Windows native
  store. Target name `VTTray/api-key`. Provide add/replace/remove through a masked native dialog.
- **Rationale**: The free API key is personal and must not be embedded or stored in plaintext.
  Credential Manager is the standard per-user protected store. `keyring` 4.2.0 is current.
- **Alternatives considered**:
  - DPAPI-encrypted blob in `%LOCALAPPDATA%`: acceptable fallback but more custom crypto and
    migration code.
  - Plaintext config: rejected by Principle I.
  - Environment variable: useful for development only; never the default and never logged.
- **Consequences**: First run requires a key; local monitoring works without one. If the credential
  is rejected, mark it invalid and stop online actions until replaced.

### 6. Single instance and IPC: named mutex + `WM_COPYDATA`

- **Decision**: `CreateMutexW` with `Local\VTTray.SingleInstance` for single instance; a hidden top-level window and
  `WM_COPYDATA` for `--check <path>` handoff. If the first window is not ready within 3 seconds, the
  second process writes a one-shot pending-check file and exits.
- **Rationale**: No dependency on the stale `single-instance` 0.3.3 crate (last release 2021). The
  payload is a bounded, validated path; no remote network surface. `WM_COPYDATA` is sufficient for
  one-way check requests.
- **Alternatives considered**:
  - `single-instance` 0.3.3: stale; avoid.
  - `interprocess` 2.4.8 local sockets: capable but unnecessary for one-way handoff.
  - Named pipes: useful later if result-returning CLI support is added.
- **Consequences**: Validate the path and reject non-files; window messages cannot cross sessions,
  so no extra session check is needed. If the existing instance does not respond, write the pending
  file and exit; never launch another process or a second tray icon.

### 7. Dialogs and notifications: native

- **Decision**: Use `rfd` 0.18.0 for native file/folder pickers and consent dialogs; use
  `Shell_NotifyIconW` `NIF_INFO` balloons for notifications. No `notify-rust` dependency.
- **Rationale**: `rfd` uses native Windows dialogs and only loads on demand. `notify-rust` is aimed
  at Linux/BSD/macOS; native balloon notifications match the tray and need no extra runtime.
- **Alternatives considered**:
  - Raw `IFileOpenDialog`/TaskDialog: fewer dependencies but more COM/Win32 boilerplate.
  - Custom results window: rejected; full reports open in the browser.
- **Consequences**: Balloons may be suppressed by Focus Assist; always keep a recent list in the
  tray menu so users can recover missed notifications.

### 8. Autostart and Explorer integration: registry directly

- **Decision**: Use `winreg` 0.56.0 to write/remove the HKCU Run entry and the HKCU file context-menu
  verb. Do not add `auto-launch` 0.6.0.
- **Rationale**: Both are small, per-user registry operations with no admin requirement. Direct
  control makes cleanup and tests straightforward.
- **Alternatives considered**:
  - `auto-launch` 0.6.0: current but an unnecessary wrapper.
  - Task Scheduler: more moving parts than a per-user tray app needs.
- **Consequences**: Uninstall/disable must remove both entries. Context-menu quick check is
  hash-only and must not expose upload.

### 9. Resources: embedded icon/manifest via `embed-resource`

- **Decision**: Embed a small `.ico` and DPI-aware manifest with `embed-resource` 3.0.12 and
  `LoadImageW`. Do not use the `image` crate.
- **Rationale**: `embed-resource` is current (3.0.12, October 2026). `winres` 0.1.12 is from 2021
  and stale. A small icon needs no image-processing pipeline.
- **Alternatives considered**:
  - `winres`: stale.
  - `image` 0.25.10: heavy for one icon.
  - Loose `.ico` beside the executable: breakable and easy to lose.
- **Consequences**: Requires the Windows SDK resource compiler (`rc.exe`); document the fallback of
  a loose icon for development.

### 10. Multipart upload: manual streaming

- **Decision**: Build the multipart body manually with a boundary, stream the file, and set an exact
  `Content-Length`. Do not use `ureq`'s `unversioned::multipart` API.
- **Rationale**: `ureq` 3.x marks multipart as unversioned/non-semver; a small streaming encoder is
  safer for a long-lived app and avoids loading large files into memory. Setting `Content-Length`
  prevents chunked encoding and is accepted by the upload endpoint.
- **Alternatives considered**:
  - `ureq` multipart: convenient but unstable API; pinning a minor version is fragile.
  - `reqwest` multipart: stable but pulls Tokio.
  - Read the whole file into memory: violates the low-memory goal for large files.
- **Consequences**: Must correctly escape the base name, choose a random boundary, and test framing
  with a mock server. Only send the base name, never the full path.

### 11. Results presentation: browser, not a custom window

- **Decision**: Show a concise tray notification and store a summary; open
  `https://www.virustotal.com/gui/file/{sha256}` in the default browser for full details.
- **Rationale**: Avoids a GUI framework and a large results UI; the browser already renders the full
  report. Keeps idle memory low and the app focused.
- **Alternatives considered**:
  - Native list view/window: more code and surface area.
  - Embedded browser: rejected by Principle II.
- **Consequences**: Requires network/browser for full details; the local summary must still be
  useful offline.

### 12. Quota gate: custom, persistent

- **Decision**: Implement a custom `QuotaGate` with a 4/minute sliding window, 500/day, and 15.5K/
  month, persisted in SQLite, with a manual reserve and a paused state. Do not use `governor`.
- **Rationale**: The limits are simple, but the cross-process persistence, daily/monthly resets,
  manual reserve, and user-visible pause reasons are app-specific. A small tested component is
  clearer than adapting a generic rate limiter.
- **Alternatives considered**:
  - `governor` 0.10.4: good token bucket, but still needs persistent windows and reserve logic.
  - In-memory only: loses state on restart and can exceed the daily limit.
- **Consequences**: Must handle clock changes conservatively and reset at 00:00 UTC. Count every
  request, including uploads and analysis polls.

### 13. Monitoring scope and automatic checks

- **Decision**: V1 watches user-selected folders only. Monitoring detects and notifies but never
  uploads. Automatic hash checks are off by default; when enabled, they are fingerprint-only,
  budgeted, and pausable.
- **Rationale**: Whole-volume real-time monitoring needs USN journal privileges and wrap handling;
  it is a separate feature. Auto-uploads violate Principle I. Auto hash checks still send indicators
  to VirusTotal, so they are opt-in.
- **Alternatives considered**:
  - Auto-upload new executables: rejected (privacy, quota, consent, terms).
  - Auto hash check on by default: rejected to avoid surprise data sharing and quota burn.
  - Whole-drive `ReadDirectoryChangesW`: rejected due to buffer overflow and performance.
- **Consequences**: The tray recent list is essential for missed notifications. USN support remains a
  future feature with an explicit admin/elevation design.

### 14. Testing approach: local mock service

- **Decision**: Use a minimal blocking mock HTTP server in integration tests, built on
  `std::net::TcpListener`, plus `tempfile` for isolated databases. No live API calls in tests.
- **Rationale**: Deterministic, no quota consumption, no real file submission, and no credential
  exposure. Covers 200/401/404/429/5xx and multipart framing.
- **Alternatives considered**:
  - `wiremock`/`httpmock`: feature-rich but add an async runtime or heavy dependencies.
  - Live VirusTotal tests: unacceptable for privacy, quota, and reproducibility.
- **Consequences**: The mock must be maintained alongside the contract; contract tests are the
  guardrail.

### 15. Logging and packaging

- **Decision**: Use `log` + `flexi_logger` with size-based rotation under `%LOCALAPPDATA%\VTTray\
  logs`; redact credentials and headers. Ship a portable release binary first; add an installer
  later if needed.
- **Rationale**: Rotating local logs aid support without telemetry. A portable binary keeps v1 small
  and avoids installer complexity; autostart and context menu are per-user registry entries.
- **Alternatives considered**:
  - `simplelog`: simple but no built-in rotation policy.
  - MSI/MSIX in v1: extra packaging and signing work; defer.
- **Consequences**: Uninstall documentation must remove Run/context-menu entries and optionally the
  credential. Code signing is recommended before public distribution to reduce SmartScreen friction.

## VirusTotal API facts used by the design

Source: official VirusTotal documentation, accessed 2026-11-18; see the workspace reference
`../../../VIRUSTOTAL_API.md` (from this feature directory) and the official pages listed below.

- Base URL: `https://www.virustotal.com/api/v3`; authentication header `x-apikey`.
- Public free limits: 4 requests/minute, 500/day, 15.5K/month; free accounts must not be used in
  business workflows, commercial products, or services.
- `GET /files/{id}` accepts SHA-256, SHA-1, or MD5; use SHA-256. Unknown files return 404.
- `POST /files` accepts multipart/form-data with a `file` field; files up to 32 MB.
- `GET /files/upload_url` returns a one-time upload URL for files larger than 32 MiB (up to 650 MiB).
- `GET /analyses/{id}` returns analysis status and statistics; poll with backoff.
- `POST /files/{id}/analyse` requests a rescan of an existing file.
- Uploaded files are shared with the security community; privacy warning is mandatory.
- `GET /files/{id}/download_url` is premium-only; not used by this app.

## Risks and mitigations

| Risk | Mitigation |
|------|------------|
| Quota exhaustion from a busy folder | Auto-check off by default; 4/min gate; reserve 100/day for manual; pause and explain |
| Accidental confidential upload | Per-file consent; community-sharing warning; no auto-upload; no context-menu upload |
| API key leakage | Credential Manager; redacted logs; never argv/config; remove key action; no telemetry |
| Watcher buffer overflow or missed events | Rebuild watcher on error; stable-size check; optional periodic reconciliation scan |
| Explorer restart hides tray icon | Handle `TaskbarCreated` and re-add the icon |
| File locked/deleted during hash/upload | Retry with bounded backoff; skip deleted files; preserve original file |
| Large-file memory spike | Stream hashing and multipart upload; exact Content-Length; no full-file buffering |
| False positives/negatives | Present results as advisory; no quarantine/delete; link to full report |
| Notification suppression | Persistent recent list in tray menu |
| SmartScreen/AV friction | Code-sign releases; avoid injection/persistence beyond HKCU Run; document behavior |
| Clock changes affect quota windows | Use UTC and conservative reset; persist timestamps |
| Corporate TLS interception | Document platform-verifier/native-tls build option if needed |

## Open questions

None blocking. The following are documented assumptions, not clarifications:

- Automatic hash checks default to off; users can enable them.
- Whole-volume monitoring is deferred to a later feature.
- V1 is per-user and does not run before login.
- English-only UI in v1.

## References

- Spec Kit 1.2.6 project scaffold (opencode integration), initialized 2026-11-18.
- `windows` 0.62.2: https://crates.io/crates/windows (updated 2025-10-06)
- `tray-icon` 0.26.1: https://crates.io/crates/tray-icon (updated 2026-10-07)
- `notify` 8.2.2 stable: https://crates.io/crates/notify
- `notify-debouncer-full` 0.7.0 stable: https://crates.io/crates/notify-debouncer-full
- `ureq` 3.5.2: https://crates.io/crates/ureq (updated 2026-11-12)
- `rusqlite` 0.43.0: https://crates.io/crates/rusqlite (updated 2026-11-15)
- `redb` 4.6.0: https://crates.io/crates/redb (updated 2026-11-13)
- `keyring` 4.2.0: https://crates.io/crates/keyring (updated 2026-08-29)
- `rfd` 0.18.0: https://crates.io/crates/rfd (updated 2026-11-07)
- `winreg` 0.56.0: https://crates.io/crates/winreg (updated 2026-03-14)
- `embed-resource` 3.0.12: https://crates.io/crates/embed-resource (updated 2026-10-06)
- VirusTotal API v3 files: https://docs.virustotal.com/reference/files
- VirusTotal file upload: https://docs.virustotal.com/reference/files-scan
- VirusTotal large-file upload URL: https://docs.virustotal.com/reference/files-upload-url
- VirusTotal analysis: https://docs.virustotal.com/reference/analysis
- VirusTotal public vs premium limits: https://docs.virustotal.com/reference/public-vs-premium-api
- VirusTotal errors: https://docs.virustotal.com/reference/errors
