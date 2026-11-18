# Implementation Plan: VT Tray - Always-On VirusTotal Companion

**Branch**: `001-vt-tray-monitor` | **Date**: 2026-11-18 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/001-vt-tray-monitor/spec.md`

## Summary

Build a Windows-only, always-on tray application in Rust that watches user-selected folders for new
executable and script-like files, fingerprints them locally, and offers safe VirusTotal checks. File
uploads are always manual and require a per-file consent dialog with a community-sharing warning.
Automatic hash checks are opt-in and never upload. The app uses native Win32 shell integration, a
blocking HTTP client, a local SQLite cache, and Windows Credential Manager, targeting low idle memory
and no administrator rights.

## Technical Context

**Language/Version**: Rust stable (2024 edition), MSVC toolchain

**Primary Dependencies**:
- `windows` 0.62.2 (Win32 shell, tray, menus, notifications, mutex, registry, message loop)
- `notify` 8.2.2 + `notify-debouncer-full` 0.7.0 (stable; `notify` 9 is still a release candidate)
- `ureq` 3.5.2 with `json` and default `rustls`/`gzip` features (blocking HTTP, no Tokio)
- `sha2` 0.11.1, `rusqlite` 0.43.0 with `bundled`
- `keyring` 4.2.0 (Windows Credential Manager through its default `v1` mode)
- `rfd` 0.18.0 (native file/folder dialogs), `winreg` 0.56.0 (HKCU autostart)
- `serde` 1.0.229 + `serde_json` 1.0.154, `directories` 6.0.0, `thiserror` 2
- `log` 0.4 + `flexi_logger` 0.31.10 (rotating local logs)
- Build: `embed-resource` 3.0.12 for icon/manifest; tests: `tempfile` 3

**Storage**: SQLite cache and quota ledger at `%LOCALAPPDATA%\VTTray\cache.db`; JSON preferences at
`%APPDATA%\VTTray\config.json`; rotating logs at `%LOCALAPPDATA%\VTTray\logs`; API credential in
Windows Credential Manager only. No secrets in configuration or logs.

**Testing**: `cargo test`; unit tests for core policy, quota, fingerprinting, candidate detection,
cache, and upload state; integration tests against a local mock VirusTotal server implemented with a
bounded `std::net::TcpListener`; manual Windows smoke tests. Live VirusTotal calls are not used in
automated tests.

**Target Platform**: Windows 10/11 x64, per-user desktop session

**Project Type**: Desktop tray application with a testable core library

**Performance Goals**: At most 25 MB working set and 1% CPU while idle; tray ready within 5 seconds
of login; new stable candidate notified within 5 seconds; known-file result within 10 seconds when
the network is available.

**Constraints**: Free VirusTotal limits of 4 requests/minute, 500/day, 15.5K/month; uploads limited
to 650 MiB (warn above 200 MiB); no administrator rights; no service; no WebView/Electron/Tauri; one
running instance; manual uploads only.

**Scale/Scope**: Single user; up to 10 watched folders; bounded cache of 10,000 files and 100 queued
uploads; recent history capped by age and row count.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Gate | Result |
|-----------|------|--------|
| I. Consent and Privacy First | Manual-only uploads; per-file consent; credential in OS vault; basename only; no telemetry | PASS |
| II. Windows-Native, Tray-First, Low-Resource | Native Win32 tray; no service; blocking workers; no GUI framework; 25 MB idle target | PASS |
| III. Quota Discipline and Transparency | Single persistent quota gate; manual reserve; auto-check off by default; 429 backoff | PASS |
| IV. Testable, Modular Core | Core library isolated from Win32 adapters; mock service; deterministic tests | PASS |
| V. Least Privilege, Local-First, No Destructive Actions | HKCU only; no admin; no quarantine/delete; local cache; removable registrations | PASS |

No violations. Complexity Tracking is intentionally empty.

## Project Structure

### Documentation (this feature)

```text
specs/001-vt-tray-monitor/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── vt-api.md
│   ├── tray-ipc.md
│   └── ui-consent.md
└── tasks.md
```

### Source Code (repository root)

```text
Cargo.toml
build.rs
resources/
├── vt-tray.ico
├── vt-tray.rc
└── app.manifest

src/
├── main.rs
├── lib.rs
├── app/
│   ├── mod.rs
│   ├── logging.rs
│   ├── window.rs
│   ├── tray.rs
│   ├── menu.rs
│   ├── notifications.rs
│   └── dialogs.rs
├── platform/
│   ├── mod.rs
│   └── windows/
│       ├── mod.rs
│       ├── autostart.rs
│       ├── context_menu.rs
│       ├── credential.rs
│       ├── single_instance.rs
│       ├── ipc.rs
│       └── shell.rs
├── core/
│   ├── mod.rs
│   ├── errors.rs
│   ├── types.rs
│   ├── config.rs
│   ├── cache.rs
│   ├── quota.rs
│   ├── hash.rs
│   ├── candidates.rs
│   ├── policy.rs
│   └── settings.rs
├── api/
│   ├── mod.rs
│   ├── client.rs
│   ├── models.rs
│   ├── multipart.rs
│   └── errors.rs
├── watcher/
│   └── mod.rs
├── worker/
│   ├── mod.rs
│   └── upload.rs
└── ui/
    ├── mod.rs
    └── strings.rs

tests/
├── unit.rs            # declares tests/unit/* as modules -> `unit` test binary
├── unit/
│   ├── cache.rs  candidates.rs  hash.rs  policy.rs
│   ├── quota.rs  redaction.rs  settings.rs  upload.rs
├── integration.rs     # declares tests/integration/* as modules -> `integration` test binary
├── integration/
│   ├── mock_server.rs  api_flow.rs  manual_check.rs  manual_upload.rs
│   ├── monitoring.rs  # includes auto-check cases
│   ├── ipc.rs
│   └── quota_flow.rs  autostart.rs  context_menu.rs   # planned (T064, T065, T075)
└── fixtures/
```

**Structure Decision**: One Cargo package with a library (`src/lib.rs`) for all testable logic and a
binary (`src/main.rs`) for the Win32 shell. Platform adapters are thin; the core has no Win32
dependency and can be tested on any desktop target. No workspace split is justified for this scope. Cargo only auto-discovers `tests/*.rs`, so `tests/unit.rs` and
`tests/integration.rs` declare their folder's files with `mod`; this yields two test binaries (`unit`, `integration`).

## Implementation Design

### Process and threads

- **Main/UI thread**: hidden top-level Win32 window, message loop, tray icon, popup menus, native
  dialogs, notifications, and `TaskbarCreated` recovery.
- **Watcher thread**: `notify` watcher with `notify-debouncer-full`; emits stable candidate events.
- **Hash worker**: streams SHA-256, consults the path cache, writes cache rows, and enqueues work.
- **API worker**: single serialized request queue; owns the `ureq::Agent`; enforces the quota gate.
- **Upload worker**: one active upload at a time; streams multipart bodies; polls analysis with
  backoff; notifies the UI thread through a channel and a posted window message.
- **IPC**: second instance and `--check <path>` hand off through `WM_COPYDATA` to the hidden window;
  `--check` never opens a console and never exposes upload.

### Windows integration

- `Shell_NotifyIconW` with `NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP`; `NOTIFYICON_VERSION_4`.
- `CreatePopupMenu`/`TrackPopupMenu` for Check, Upload, Recent, Watched folders, Auto-check,
  Start with Windows, Quota status, Open logs, Quit.
- `NIF_INFO` balloon notifications for detection, verdict, quota warnings, and failures.
- Register and handle `TaskbarCreated` so the icon returns after Explorer restarts.
- Named mutex `Local\VTTray.SingleInstance` for single instance; hidden window class `VTTrayHiddenWindow` for IPC.
- HKCU `Software\Microsoft\Windows\CurrentVersion\Run` for autostart; HKCU context-menu verb for
  Explorer check only.
- `rfd` native dialogs for file/folder selection and consent; no custom results window.
- DPI-aware manifest; embedded icon; `windows_subsystem = "windows"` release binary.

### Monitoring and candidate policy

- Recursive watches only on user-selected folders; reject volume roots and warn about network shares.
- Candidate types: PE files detected by `MZ` + `PE\0\0`, plus scripts and installers by extension
  (`.exe`, `.dll`, `.sys`, `.scr`, `.com`, `.bat`, `.cmd`, `.ps1`, `.vbs`, `.js`, `.msi`, `.lnk`,
  `.iso`); manual check accepts any file.
- Ignore partial/temp names (`.tmp`, `.part`, `.crdownload`, `~$*`) and reparse points.
- Debounce at least one second after the last write, then require size and modification time to be
  stable across two checks before fingerprinting.
- Path cache keyed by normalized path with size and modification time; unchanged files are not
  re-hashed.

### VirusTotal flow

- Hash-only check: `GET /api/v3/files/{sha256}`; unknown is HTTP 404.
- Known file: show `last_analysis_stats`, notable engine results, type, first/last seen, and a link
  to `https://www.virustotal.com/gui/file/{sha256}`. Offer `POST /api/v3/files/{id}/analyse` for a
  rescan when the result is stale.
- Manual upload: compute hash, check existing report, confirm consent, then:
  - `POST /api/v3/files` for files up to 32 MiB;
  - `GET /api/v3/files/upload_url` then one `POST` to the returned URL for files up to 650 MiB.
- Multipart is streamed with a known `Content-Length`; the field name is `file`; only the base name
  is sent; `Content-Type` is `application/octet-stream`.
- Poll `GET /api/v3/analyses/{id}` with backoff; cap at six polls, then mark the job `failed`
  with a retryable timeout reason; only the user can retry.
  On completion, store the summary and notify; failures preserve the upload record and file.
- Never call premium endpoints such as `download_url`.

### Quota and error policy

- One `QuotaGate` owns three windows: 4/minute sliding, 500/day, 15.5K/month. Every request costs one
  unit, including uploads and analysis polls.
- Reserve at least 100 daily requests for manual actions; pause automatic checks below that reserve.
- Persist timestamps/costs in SQLite; reset daily and monthly windows at 00:00 UTC.
- HTTP 429: exponential backoff (60 s, 120 s, 300 s), honor `Retry-After` when present, and pause the
  queue; no retry storms.
- 401/403: mark the credential invalid, stop online actions, keep local monitoring.
- 5xx/network: retry transient work with capped exponential backoff; never retry a canceled upload.
- Log redaction: never log `x-apikey`, request headers, or full paths sent to the service.

### Test strategy

- Unit tests: quota windows and reserve, candidate classification, stability/debounce, SHA-256
  streaming, path-cache invalidation, upload state transitions, error mapping, log redaction.
- Integration tests: mock server for known/unknown/404/401/429/5xx, multipart framing and basename,
  large-file upload URL flow, analysis polling and completion, cancel behavior.
- Manual smoke tests: first-run key setup, check known/unknown file, consent cancel, small upload,
  large upload, folder add/remove, Explorer restart, autostart, single instance, idle memory.

## Complexity Tracking

> No constitution violations. No additional projects, services, GUI frameworks, or async runtimes
> are introduced. The native Win32 shell is required by Principle II and is isolated behind
> adapters to keep Principle IV satisfied.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| None | N/A | N/A |
