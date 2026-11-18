# Feature Specification: VT Tray - Always-On VirusTotal Companion

**Feature Branch**: `001-vt-tray-monitor`

**Created**: 2026-11-18

**Status**: Implemented and validated (2026-11-18: 97 automated tests; manual smoke test on Windows 11 26H2 passed)

**Input**: User description: "A Windows tray application that runs always on, monitors selected
folders for new executable files, checks their hashes against VirusTotal v3, and supports manual
hash checks and manual uploads with explicit consent. Low memory, no automatic uploads."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Manual file check and consented upload (Priority: P1)

A user can invoke a check on any file from the tray, see whether the file is already known to
VirusTotal, and upload it only after an explicit confirmation that explains how submissions are
shared. If the file is already known, the user can view the existing result instead of uploading
it again.

**Why this priority**: This is the safe fallback for every unknown file and the minimum useful
product on its own. It delivers value even if folder monitoring is disabled.

**Independent Test**: With a configured credential, check a known benign file and confirm a
reputation summary appears without an upload. Check an unknown non-sensitive test file, confirm the
consent dialog, and verify a result appears. Repeat and cancel the consent dialog; verify no upload
occurred.

**Acceptance Scenarios**:

1. **Given** a configured credential and network access, **When** the user checks a known file,
   **Then** the app shows a reputation summary and does not upload the file.
2. **Given** an unknown file, **When** the user checks it, **Then** the app reports that the file is
   unknown and offers a separate upload action; no file content is sent by the check.
3. **Given** the user chooses to upload, **When** the confirmation appears, **Then** it shows the
   file name, size, content fingerprint, and a clear warning that the file will be shared with the
   VirusTotal security community.
4. **Given** the confirmation is visible, **When** the user cancels, **Then** no upload occurs and
   the file remains unchanged.
5. **Given** a file already has a recent result, **When** the user opens it, **Then** the app offers
   the existing result and an optional rescan instead of silently uploading a duplicate.
6. **Given** an upload finishes, **When** analysis completes, **Then** the app shows a summary and a
   link to the full public report; it does not delete, quarantine, or modify the file.

---

### User Story 2 - Always-on folder monitoring with safe alerts (Priority: P2)

A user can add folders to monitor. While the app runs in the tray, new executable or script-like
files are detected, fingerprinted locally, and announced. Monitoring never uploads a file. If the
user enables automatic hash checks, only the fingerprint is looked up, within a capped budget.

**Why this priority**: This is the always-on value of the product. It is separable from manual
checking and remains safe when the user is away.

**Independent Test**: Add a test folder, place a stable test executable in it, and verify a tray
notification appears without any upload. With automatic hash checks off, verify no reputation
request is made. With them on, verify only a hash lookup is made and an unknown result offers a
manual upload action.

**Acceptance Scenarios**:

1. **Given** a watched folder, **When** the app restarts, **Then** the folder is still watched and
   its state is shown in the tray menu.
2. **Given** monitoring is active, **When** a new stable executable appears, **Then** the user is
   notified within five seconds of the file becoming stable.
3. **Given** a file is still downloading or being written, **When** it changes repeatedly, **Then**
   it is not treated as a final candidate or fingerprinted until it is stable.
4. **Given** automatic hash checks are off, **When** a new candidate is detected, **Then** no
   reputation request is made and the notification offers manual check or upload.
5. **Given** automatic hash checks are on, **When** a new candidate is detected, **Then** only its
   fingerprint is checked, quota rules apply, and no upload occurs automatically.
6. **Given** an unknown result, **When** the user chooses upload from the notification or recent
   list, **Then** the full consent flow from User Story 1 is required.
7. **Given** a watched folder is removed, **When** new files appear there, **Then** they are no
   longer detected or announced.

---

### User Story 3 - Control privacy, quota, and preferences (Priority: P3)

A user can see how much VirusTotal quota remains, why work is paused, and what is queued. They can
turn automatic hash checks on or off, control notifications, start the app at login, replace or
remove the credential, and clear cached results and history.

**Why this priority**: The free account has finite capacity and the app handles sensitive files.
Users need visible control without editing files or using a console.

**Independent Test**: Open the tray menu and verify quota and queue status are visible. Toggle
automatic checks and verify the next detection respects the change. Remove the credential and verify
online actions are disabled while local monitoring continues. Clear history and verify it is gone
without losing watched folders.

**Acceptance Scenarios**:

1. **Given** the app has made requests, **When** the user opens the tray status, **Then** remaining
   quota and any pause reason are visible in plain language.
2. **Given** automatic hash checks are enabled, **When** the user disables them, **Then** no further
   automatic reputation requests occur for new detections.
3. **Given** no credential is configured, **When** the user tries an online action, **Then** the app
   explains how to add one and does not send data.
4. **Given** a credential exists, **When** the user removes it, **Then** it is removed from protected
   storage, online actions stop, and local monitoring continues.
5. **Given** cached results and history exist, **When** the user clears them, **Then** they are
   removed but watched folders and preferences remain unless the user also resets those.
6. **Given** the app runs, **When** it performs normal operations, **Then** it does not request
   administrator rights, install a service, or send telemetry.

---

### User Story 4 - Quick check from Explorer (Priority: P4)

A user can right-click a file in Explorer and choose a quick check. The request is handed to the
running tray app, which performs a hash-only check. The context-menu action never uploads.

**Why this priority**: It removes friction for users who already know which file they care about,
but the tray and manual picker flows already deliver the core value.

**Independent Test**: With the tray app running, right-click a test file, choose the quick-check
action, and verify the tray processes it without opening a console window or uploading. Repeat with
the tray app stopped and verify it starts and processes the check.

**Acceptance Scenarios**:

1. **Given** the tray app is running, **When** the user invokes quick check on a file, **Then** the
   existing instance handles it and no duplicate instance appears.
2. **Given** the tray app is not running, **When** the user invokes quick check, **Then** the app
   starts, shows its tray icon, and processes the check.
3. **Given** a quick check is requested, **When** it runs, **Then** only a fingerprint lookup is
   performed and no upload is possible from that action.

---

### Edge Cases

- No credential, an invalid credential, or a revoked credential.
- Quota nearly exhausted, quota exhausted, HTTP 429, or a long service outage.
- Offline, captive portal, DNS failure, TLS failure, or proxy interference.
- File locked by another process, deleted between detection and fingerprinting, renamed, or moved.
- Partial downloads, temporary files, archive extraction, or a file that keeps changing.
- Zero-byte files, very large files, files above the upload limit, and files above 200 MiB.
- Duplicate events for the same file, rapid create/rename/delete sequences, and app restart during a
  pending upload.
- Watched folder deleted, disconnected, on a network share, or inaccessible.
- User logs off, machine sleeps, resumes, or the system clock changes.
- Explorer restarts (tray icon must return), second instance launched, or user quits during work.
- Notification suppressed by Focus Assist or the user misses it.
- User tries to upload a confidential file or cancels halfway through the consent flow.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The app MUST run as a per-user tray application with no required main window, and MUST
  support an optional start-at-login preference.
- **FR-002**: Users MUST be able to add, replace, and remove the VirusTotal credential. The
  credential MUST be stored in protected operating-system storage, MUST never be shown in full after
  entry, and MUST never appear in logs, configuration, command lines, or error reports.
- **FR-003**: Users MUST be able to check any file manually. The check MUST compute a local content
  fingerprint and report whether the service knows the file; it MUST NOT upload file content.
- **FR-004**: Users MUST be able to upload a file only after an explicit confirmation for that file.
  The confirmation MUST show the file name, size, fingerprint, and a warning that the submission is
  shared with the security community. Canceling MUST send nothing.
- **FR-005**: When a file is already known, the app MUST offer the existing result and an optional
  rescan rather than silently uploading a duplicate. A cached result older than 7 days is stale and is
  looked up again.
- **FR-006**: The app MUST support uploads up to the service maximum, show progress, and warn before
  files above 200 MiB are uploaded. Failures MUST leave the original file untouched.
- **FR-007**: Users MUST be able to add and remove watched folders. The app MUST monitor them for new
  executable and script-like files, ignore partial or temporary files, and wait for a file to become
  stable before fingerprinting it.
- **FR-008**: Monitoring MUST notify the user about a new candidate. It MUST NOT upload. Automatic
  fingerprint lookups MUST be off by default and MUST be controlled by a single opt-in setting.
- **FR-009**: All service requests MUST pass through one quota gate that respects the free-account
  limits, persists across restarts, counts uploads and analysis polls, reserves capacity for manual
  actions, and surfaces remaining quota and pause reasons.
- **FR-010**: The app MUST show a concise result summary and a link to the full service report. It
  MUST NOT quarantine, delete, block, or modify files based on service results.
- **FR-011**: The app MUST persist preferences, watched folders, a bounded result cache, recent
  detections, pending work, and quota state. Users MUST be able to clear cached and historical data.
- **FR-012**: The app MUST enforce a single running instance. Relaunching MUST focus or signal the
  existing instance. Quitting MUST remove the tray icon and stop worker activity cleanly.
- **FR-013**: The app MUST NOT send telemetry or remote logs. Local logs MUST redact credentials and
  MAY contain file names and fingerprints, but MUST NOT send full local paths to the service.
- **FR-014**: The app MUST handle missing credentials, authentication failures, quota exhaustion,
  transient service failures, and offline operation with actionable messages. It MUST queue work
  where safe and MUST NOT lose consent or silently retry after a limit.
- **FR-015**: The app MUST validate watched folders, show inaccessible or disconnected folders, and
  stop monitoring a folder when the user removes it.
- **FR-016**: The app MUST run without administrator rights and MUST write only to per-user
  locations. Autostart and context-menu entries MUST be removable without leaving orphaned state.
  A "Remove VTTray from this PC" action MUST, after confirmation, remove autostart, the Explorer
  entry, the stored credential, and all local data in one step (constitution V). "Clear local data"
  keeps preferences and quota counters.
- **FR-017**: V1 MUST monitor selected folders only. Whole-volume monitoring is out of scope.
- **FR-018**: Users MUST be able to pause and resume monitoring; the tray state MUST make the current
  mode visible.
- **FR-019**: Results MUST be presented as advisory reputation information, not as a guarantee that a
  file is safe or unsafe.
- **FR-020**: The Explorer quick-check action MUST perform a hash-only check, MUST hand off to a
  single running instance, and MUST NOT expose an upload action.
  Without a credential it fingerprints locally and reports that a key is needed; it never prompts.

### Key Entities *(include if data involved)*

- **Watched Folder**: A user-selected directory, its enabled state, and when it was added.
- **File Candidate**: A detected file with its local path, base name, size, modification time,
  content fingerprint, stability state, and first/last seen times.
- **Reputation Record**: The fingerprint, whether the service knows it, the detection summary, the
  last check time, and whether the result is fresh or stale.
- **Upload Job**: The file identity, consent time, current state (pending, uploading, analyzing,
  completed, failed, canceled), and a service job reference.
- **Quota Ledger**: Request costs and timestamps, remaining capacity for each limit window, reset
  times, and the current pause reason.
- **User Preferences**: Watched folders, automatic-check setting, notification setting, autostart
  setting, and cache-retention preference.
- **Credential State**: Whether a credential is present, absent, or rejected; the credential itself
  lives only in protected operating-system storage.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 95% of manual checks for files already known to the service show a result within ten
  seconds when the network is available.
- **SC-002**: Across a 30-day test, 100% of uploads have a recorded explicit confirmation and zero
  automatic uploads occur.
- **SC-003**: A new stable executable in a watched folder produces a notification within five
  seconds of becoming stable.
- **SC-004**: After startup and with monitoring active, the app uses at most 25 MB of working memory
  and at most 1% CPU while idle on the reference machine.
- **SC-005**: During a seven-day typical-use test, the app never exceeds the free-account request
  limits and warns the user before a limit is reached: when automatic checks pause at 400 daily
  requests, when 25 or fewer requests remain today, and when 90% of the monthly limit is used.
- **SC-006**: A user can complete a manual check in three interactions or fewer and a consented
  upload in six interactions or fewer, starting from the tray.
- **SC-007**: A second launch never creates a duplicate instance; the existing tray responds within
  one second.
- **SC-008**: After a full test cycle, zero credentials appear in configuration, logs, crash output,
  or command lines, and the credential can be removed by the user.
- **SC-009**: 100% of detection and result notifications identify the file and state; zero files are
  deleted, quarantined, or modified.
- **SC-010**: With start-at-login enabled, the tray is available within five seconds of login on the
  reference machine.

## Assumptions

- Target is Windows 10/11 x64 in a per-user desktop session.
- The user has a personal, non-commercial VirusTotal free account and supplies their own credential;
  the user accepts VirusTotal's terms, including community sharing of submissions.
- Network access and the VirusTotal service are available for online actions.
- Uploads are always manual. Automatic hash checks are off by default and, when enabled, lookup
  fingerprints only.
- V1 monitors user-selected folders; whole-volume monitoring is deferred.
- The app runs in the user session with autostart. It is not a service and does not run before login.
- VirusTotal is an advisory reputation source, not an antivirus engine or a local verdict.
- English is the only UI language in v1.
- Files up to the service maximum (650 MiB) are supported; files above 200 MiB receive a warning.
- The user has permission to fingerprint and optionally upload the files they select.

## Out of Scope

- Automatic uploads, auto-quarantine, auto-delete, blocking, or remediation.
- Whole-volume real-time monitoring and NTFS USN journal integration.
- URL, domain, or IP address submission and reporting.
- Premium VirusTotal endpoints or bulk intelligence feeds.
- Windows service, pre-login operation, or multi-user/server deployment.
- Cloud sync, mobile apps, telemetry, and automatic updates.
- A custom results window; full reports open in the user's browser.

## Dependencies

- VirusTotal free API availability, terms, and request limits.
- Windows protected credential storage for the user's credential.
- A user-supplied VirusTotal credential.
- Network connectivity for online actions.

