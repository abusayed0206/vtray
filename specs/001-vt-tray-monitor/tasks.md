# Tasks: VT Tray - Always-On VirusTotal Companion

**Input**: Design documents from `/specs/001-vt-tray-monitor/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/

**Tests**: Included. The constitution requires a testable core, and the plan defines unit and mock
integration coverage. Write the listed tests first and confirm they fail before implementing the
corresponding code. No live VirusTotal calls in automated tests.

**Organization**: Tasks are grouped by user story so each story can be implemented, tested, and
delivered independently. Paths assume the single-package layout in plan.md.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: US1, US2, US3, or US4
- Every task includes an exact file path

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Create the Rust package, build resources, and test scaffolding.

- [x] T001 Create the package layout and module skeletons per plan.md in `Cargo.toml`, `src/lib.rs`, `src/main.rs`, `src/app/mod.rs`, `src/platform/mod.rs`, `src/platform/windows/mod.rs`, `src/core/mod.rs`, `src/api/mod.rs`, `src/watcher/mod.rs`, `src/worker/mod.rs`, `src/ui/mod.rs`, `tests/unit/main.rs`, and `tests/integration/main.rs` (Cargo only discovers `tests/*.rs`; each `main.rs` declares its folder's files with `mod`)
- [x] T002 Add and pin dependencies, build-dependencies, and the release profile in `Cargo.toml`; set `#![windows_subsystem = "windows"]` for release in `src/main.rs`
- [x] T003 [P] Configure formatting, linting, and ignores in `rustfmt.toml`, `.clippy.toml`, and `.gitignore`
- [x] T004 [P] Add the tray icon, resource script, DPI-aware manifest, and resource embedding in `resources/vt-tray.ico`, `resources/vt-tray.rc`, `resources/app.manifest`, and `build.rs`
- [x] T005 [P] Implement rotating local logging with credential/header redaction in `src/app/logging.rs`
- [x] T006 [P] Create the mock HTTP server skeleton with bounded request parsing in `tests/integration/mock_server.rs`
- [x] T007 [P] Add a benign upload fixture and test data notes in `tests/fixtures/benign.txt`
- [x] T008 Run `cargo check`, `cargo fmt --check`, and `cargo clippy --all-targets -- -D warnings`; fix baseline issues

**Checkpoint**: The package builds and the test harness compiles.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core infrastructure required by every user story. No story work starts until this phase
is complete.

**CRITICAL**: The quota gate, credential store, database, API client, and tray shell must exist first.

### Core models and persistence

- [x] T009 [P] Define shared error types and redaction helpers in `src/core/errors.rs`
- [x] T010 Implement domain types and state enums for files, paths, detections, uploads, and quota events in `src/core/types.rs`
- [x] T011 Write failing persistence tests for schema creation, upserts, path invalidation, and retention in `tests/unit/cache.rs`
- [x] T012 Implement SQLite connection setup, schema versioning, WAL, `cache_size=-2000`, and pruning in `src/core/cache.rs`
- [x] T013 Implement file, path, detection, upload-job, and quota repositories in `src/core/cache.rs`
- [x] T014 [P] Write failing configuration tests for defaults, validation, atomic save, and corrupt-file recovery in `tests/unit/settings.rs`
- [x] T015 Implement preferences load/save, watched-folder validation, and schema version in `src/core/config.rs` and `src/core/settings.rs`

### Hashing and quota

- [x] T016 [P] Write failing SHA-256 streaming and path-cache invalidation tests in `tests/unit/hash.rs`
- [x] T017 Implement streaming SHA-256, base-name extraction, and stable-file checks in `src/core/hash.rs`
- [x] T018 [P] Write failing quota tests for 4/minute, 500/day, 15.5K/month, manual reserve, UTC reset, and 429 pause in `tests/unit/quota.rs`
- [x] T019 Implement the persistent `QuotaGate` and reservation lifecycle in `src/core/quota.rs`

### API transport

- [x] T020 [P] Define minimal tolerant service response models in `src/api/models.rs`
- [x] T021 Implement the blocking `ureq` client with `x-apikey`, timeouts, status mapping, and redacted errors in `src/api/client.rs` and `src/api/errors.rs`
- [x] T022 [P] Write failing multipart framing tests for empty, 1-byte, boundary-size, and unsafe filenames in `tests/integration/mock_server.rs`
- [x] T023 Implement the streaming multipart encoder with exact `Content-Length` and filename sanitization in `src/api/multipart.rs`
- [x] T024 Implement lookup, rescan, direct upload, upload-URL, and analysis-poll methods in `src/api/client.rs`

### Windows shell and services

- [x] T025 [P] Write failing policy and credential-state tests for present, absent, invalid, and upload-denial decisions in `tests/unit/policy.rs`
- [x] T026 Implement the Credential Manager wrapper with target `VTTray/api-key` in `src/platform/windows/credential.rs`
- [x] T027 Implement the hidden Win32 window, message loop, and `TaskbarCreated` handling in `src/app/window.rs`
- [x] T028 Implement tray icon add/remove/update and native balloon notifications in `src/app/tray.rs` and `src/app/notifications.rs`
- [x] T029 Implement the tray menu builder, command enum, and `Quit` action in `src/app/menu.rs`
- [x] T030 Implement the single-instance mutex, hidden-window lookup, and `WM_COPYDATA` receive/send in `src/platform/windows/single_instance.rs` and `src/platform/windows/ipc.rs`
- [x] T031 Implement HKCU autostart registration/removal in `src/platform/windows/autostart.rs`
- [x] T032 Implement native file/folder pickers, consent dialogs, and shell-open helpers in `src/app/dialogs.rs` and `src/platform/windows/shell.rs`
- [x] T033 Create centralized user-facing strings in `src/ui/strings.rs`
- [x] T034 Implement bounded worker channels, clean `Quit` shutdown, and restart supervision in `src/worker/mod.rs`

### Foundational validation

- [x] T035 [P] Extend the mock server for known, unknown, 401, 403, 404, 429, and 5xx responses in `tests/integration/mock_server.rs`
- [x] T036 Add an integration test harness with isolated config, database, and mock base URL in `tests/integration/mock_server.rs`
- [x] T037 Run the foundational tests and fix all failures with `cargo test`

**Checkpoint**: Foundation ready. US1-US4 can begin.

---

## Phase 3: User Story 1 - Manual Check and Consented Upload (Priority: P1) MVP

**Goal**: A user can pick a file, check its hash, see a result, and deliberately upload it with a
per-file consent dialog. No upload occurs from a check, and cancel sends nothing.

**Independent Test**: With the mock server, check a known and an unknown file, cancel an upload and
verify zero requests/rows, then accept a small and a large upload and verify analysis completion.

### Tests for US1

- [x] T038 [P] [US1] Write failing upload state-machine tests for pending, uploading, analyzing, completed, failed, canceled, and retry rules in `tests/unit/upload.rs`
- [x] T039 [P] [US1] Write failing manual known/unknown check tests, including a known-result timing assertion under 10 seconds, in `tests/integration/manual_check.rs`
- [x] T040 [P] [US1] Write failing consent-cancel and consent-accept tests that assert no request before consent in `tests/integration/manual_upload.rs`
- [x] T041 [P] [US1] Add zero-byte, above-650-MiB rejection, and analysis-poll (complete, six-poll timeout) tests to `tests/integration/api_flow.rs` (direct multipart, upload-URL, 429, 401 already covered)

### Implementation for US1

- [x] T042 [P] [US1] Implement the manual check use case, cache-first lookup, rescan option, and result summary in `src/worker/mod.rs`
- [x] T043 [P] [US1] Implement the per-file consent dialog with cancel-first focus, community warning, and the >200 MiB warning in `src/app/dialogs.rs`
- [x] T044 [P] [US1] Add `Check file` and `Upload file` menu commands and picker wiring in `src/app/menu.rs` and `src/app/window.rs`
- [x] T045 [US1] Implement the upload state machine and persisted consent gate, including restart-safe pending/failed states, in `src/worker/upload.rs`
- [x] T046 [US1] Implement direct and large-file upload execution up to 650 MiB with streaming multipart, progress, and failure paths that leave the original file untouched in `src/worker/upload.rs` and `src/api/client.rs`
- [x] T047 [US1] Implement analysis polling with 10/30/60/120/240/300-second backoff and a six-poll cap in `src/worker/upload.rs`
- [x] T048 [US1] Implement result, progress, failure, and cancel notifications plus `Open report`, with advisory wording and no file-modifying action, in `src/app/notifications.rs` and `src/platform/windows/shell.rs`
- [x] T049 [US1] Enforce credential, quota, timeout, 429, 401/403, offline, captive-portal, TLS/proxy, and retry policy in the manual flows in `src/worker/mod.rs` and `src/worker/upload.rs`
- [x] T050 [US1] Persist file, detection, consent, and upload state around manual actions, including restart during a pending upload, in `src/core/cache.rs` and `src/worker/mod.rs`
- [x] T051 [US1] Run US1 tests and verify the MVP manually with the mock server

**Checkpoint**: US1 is independently functional. No upload path exists without consent.

---

## Phase 4: User Story 2 - Always-On Folder Monitoring (Priority: P2)

**Goal**: Added folders are watched continuously; new stable executable/script-like files trigger one
local notification. Monitoring never uploads. Automatic hash checks are opt-in and budgeted.

**Independent Test**: Add a temp folder, create a benign executable-like file, verify one
notification and no upload; verify unchanged files are not re-hashed; verify removal stops events.

### Tests for US2

- [x] T052 [P] [US2] Write failing candidate classification tests for PE magic, script extensions, temp/partial names, reparse points, and manual-any-file in `tests/unit/candidates.rs`
- [x] T053 [P] [US2] Extend `tests/integration/monitoring.rs` (add-folder and partial-file cases exist) with stable-file within five seconds, duplicate-event, remove-folder, pause/resume, and locked/deleted files
- [x] T054 [P] [US2] Write a failing auto-check test proving the opt-in default is off, budgeted checks never upload, and the manual reserve is honored in `tests/integration/monitoring.rs`

### Implementation for US2

- [x] T055 [P] [US2] Implement candidate classification and ignore rules in `src/core/candidates.rs`
- [x] T056 [P] [US2] Implement recursive watcher creation, error recovery, and overflow rebuild in `src/watcher/mod.rs`
- [x] T057 [US2] Implement debounce, stable-size/mtime confirmation, path-cache reuse, and locked/deleted/renamed handling in `src/watcher/mod.rs` and `src/core/hash.rs`
- [x] T058 [US2] Implement add/remove/enable watched-folder flows, inaccessible/disconnected/network-share warnings, and persistence in `src/app/menu.rs` and `src/core/config.rs`
- [x] T059 [US2] Implement opt-in auto hash checks through the quota gate, hash-only, with reserve pause in `src/worker/mod.rs`
- [x] T060 [US2] Implement the recent detections menu, deduplication, and notifications that identify the file and state in `src/app/menu.rs` and `src/app/notifications.rs`
- [x] T061 [US2] Implement monitoring pause/resume and reconciliation scan for missed events in `src/watcher/mod.rs`
- [x] T062 [US2] Run US2 tests and verify monitoring manually with a temp folder

**Checkpoint**: US2 works independently; no upload is reachable from monitoring.

---

## Phase 5: User Story 3 - Privacy, Quota, and Preferences (Priority: P3)

**Goal**: The user can manage the API key, see quota state and pause reasons, control autostart and
notifications, clear local data, and trust that logs and requests minimize data.

**Independent Test**: Clear/set a mock credential, exhaust a quota window, verify online actions
pause with the correct reason and resume after reset, toggle autostart, and verify redacted logs.

### Tests for US3

- [x] T063 [P] [US3] Write failing settings validation, credential status, and privacy-guard tests in `tests/unit/settings.rs` and `tests/unit/redaction.rs`
- [x] T064 [P] [US3] Write failing quota exhaustion, manual-reserve, 7-day simulation, reset, sleep/resume, and clock-change integration tests in `tests/integration/quota_flow.rs`
- [x] T065 [P] [US3] Write failing autostart add/remove, startup-within-five-seconds, and registry-cleanup tests against a testable adapter in `tests/integration/autostart.rs` (startup-within-five-seconds is measured manually in T082/T086)

### Implementation for US3

- [x] T066 [P] [US3] Implement first-run and key add/replace/remove dialogs with masked input in `src/app/dialogs.rs`
- [x] T067 [P] [US3] Implement credential invalidation handling and online-action disablement in `src/platform/windows/credential.rs` and `src/worker/mod.rs`
- [x] T068 [US3] Implement quota status, queued work, manual reserve, pause reasons, and reset-time UI in `src/app/menu.rs` and `src/ui/strings.rs`
- [x] T069 [US3] Implement preferences persistence for auto-check, notifications, and autostart in `src/core/settings.rs` and `src/platform/windows/autostart.rs`
- [x] T070 [US3] Implement clear-history and remove-key confirmations with explicit consequences in `src/app/dialogs.rs` and `src/core/cache.rs`
- [x] T071 [US3] Audit all request and log paths for basename-only, header redaction, and no telemetry in `src/core/errors.rs`, `src/api/client.rs`, and `src/app/logging.rs`
- [x] T072 [US3] Run US3 tests and verify quota/credential/autostart manually with the mock server

**Checkpoint**: US3 works independently and makes privacy/quota state visible.

---

## Phase 6: User Story 4 - Explorer Quick Check (Priority: P4)

**Goal**: A `--check <path>` invocation and an Explorer context-menu verb perform a hash-only check
with no upload and no duplicate tray instance.

**Independent Test**: Run `vtray.exe --check` against a file while the app is running, while it is
initializing, and while it is stopped; verify one tray icon and one hash-only check. Validate
malformed payloads are rejected.

### Tests for US4

- [x] T073 [P] [US4] Write failing IPC payload validation tests for valid paths, relative paths, directories, reparse points, invalid UTF-16, missing NUL, and oversized data in `tests/integration/ipc.rs`
- [x] T074 [US4] Write failing single-instance and pending-file fallback tests in `tests/integration/ipc.rs`
- [x] T075 [P] [US4] Write a failing context-menu registration/removal and orphan-cleanup test against an injectable registry adapter in `tests/integration/context_menu.rs`

### Implementation for US4

- [x] T076 [P] [US4] Implement `--check` argument parsing, canonical path validation, and the one-shot pending-check file in `src/main.rs` and `src/platform/windows/ipc.rs`
- [x] T077 [P] [US4] Implement HKCU Explorer context-menu registration/removal in `src/platform/windows/context_menu.rs`
- [x] T078 [US4] Implement the second-instance sender, 3-second wait, and pending-file fallback (no second launch) in `src/platform/windows/single_instance.rs` and `src/platform/windows/ipc.rs`
- [x] T079 [US4] Implement the hash-only quick-check flow with an explicit no-upload guard in `src/worker/mod.rs`
- [x] T080 [US4] Run US4 tests and verify Explorer restart, single instance, and quick check manually

**Checkpoint**: US4 works independently; quick check can never upload.

---

## Phase 7: Polish and Cross-Cutting Concerns

**Purpose**: Release hardening and final validation.

- [x] T081 [P] Run `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings`; fix all findings
- [x] T082 [P] Measure idle working set and CPU for 10 minutes; fix any path above 25 MB or 1% idle CPU (2026-11-18: 12.1 MB avg / 12.2 MB max working set, 0.001% avg CPU over 10 min, release build)
- [x] T083 [P] Review every user-facing string for safe errors, advisory wording, no raw response bodies, and the SC-006 interaction counts in `src/ui/strings.rs`
- [x] T084 [P] Verify release icon, DPI manifest, `windows` subsystem, LTO/strip settings, and absence of a console window in `Cargo.toml`, `build.rs`, and `resources/`
- [x] T085 [P] Verify no secrets, full paths, or headers appear in logs, config, database, or crash output
- [x] T086 Run every `quickstart.md` step with benign files and a disposable API key (2026-11-18: all 12 steps passed on the user's desktop; tray started by the user, not a sandboxed tool)
- [x] T087 Run the full `cargo test` suite, including the zero-destructive-action guard, and document any unavailable environment checks
- [x] T088 [P] Update `spec.md` status and implementation notes only if behavior changed during implementation
- [x] T089 Review the final diff for scope creep; remove dead code, unused dependencies, and speculative abstractions
- [x] T090 Run the Spec Kit analyze/converge pass and resolve any remaining requirement/task inconsistency

---

## Dependencies and Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies.
- **Foundational (Phase 2)**: Depends on Setup; blocks every user story.
- **US1 (Phase 3)**: Depends on Foundational; MVP.
- **US2 (Phase 4)**: Depends on Foundational; integrates with US1 for auto checks but remains
  independently testable.
- **US3 (Phase 5)**: Depends on Foundational and US1 for quota/credential UI; US2 optional.
- **US4 (Phase 6)**: Depends on Foundational and US1 for the check flow.
- **Polish (Phase 7)**: Depends on all stories selected for release.

### Within Each Story

- Tests first; confirm they fail before implementation.
- Models and persistence before services.
- Services before shell/menu wiring.
- Core logic before integration.
- Complete one story and its checkpoint before starting the next priority.

### Parallel Opportunities

- Setup tasks T003-T007 are independent.
- Foundational tests T011, T014, T016, T018, T020, T022, T025 can be written in parallel.
- US1 tests T038-T041 are independent; then T042-T044 can proceed in parallel.
- US2 tests T052-T054 are independent; T055-T056 can proceed in parallel.
- US3 tests T063-T065 are independent; T066-T067 can proceed in parallel.
- US4 tests T073-T075 are independent; T076-T077 can proceed in parallel.
- Polish tasks marked [P] can run in parallel after implementation stabilizes.

## MVP Strategy

1. Complete Phases 1-2.
2. Complete US1 and validate with the mock server.
3. Use the MVP locally with a disposable key and benign files.
4. Add US2, then US3, then US4, validating each checkpoint independently.

## Notes

- No live VirusTotal request is required for automated tests.
- Do not commit or paste API keys; use a disposable key for manual testing.
- No task deletes, quarantines, or modifies a scanned file.
- No task adds a service, installer, webview, async runtime, or admin requirement.





