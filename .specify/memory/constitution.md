# VT Tray Constitution

<!--
Sync Impact Report
Version change: N/A -> 1.0.0 (initial ratification)
Modified principles: N/A (initial adoption)
Added sections: Core Principles (I-V), Additional Constraints, Development Workflow, Governance
Removed sections: None
Follow-up TODOs: None
-->

## Core Principles

### I. Consent and Privacy First

No file content may leave the user's machine without an explicit, informed confirmation for that
specific upload. Monitoring and hash-only checks MUST never cause an automatic file upload. Every
upload confirmation MUST show the file name, size, content fingerprint, and a plain-language warning
that submitted files are shared with the VirusTotal security community and may be redistributed.
API credentials MUST be stored in the operating system's protected credential store, MUST never be
written to configuration, logs, command lines, crash reports, or telemetry, and MUST be removable by
the user. Only the file's base name may be sent to the service; local directory paths stay local.

**Rationale**: VirusTotal submissions are shared with third parties and can expose confidential
code. Consent must be a product invariant, not a setting.

### II. Windows-Native, Tray-First, Low-Resource

The product MUST run as a per-user Windows tray application with no required main window, no
background service, and no embedded browser runtime. It MUST use native Windows APIs for shell
integration, notifications, menus, dialogs, autostart, and single-instance behavior. Blocking,
bounded background workers are preferred over an async runtime or a GUI framework. Idle memory
SHOULD remain at or below 25 MB and idle CPU at or below 1% on a typical Windows 10/11 x64 machine.

**Rationale**: An always-on security helper must be invisible in cost. Native integration also keeps
the attack surface and dependency tree small.

### III. Quota Discipline and Transparency

Every request to VirusTotal MUST pass through one shared quota gate that enforces the free-account
limits (4 requests per minute, 500 per day, 15.5K per month), persists counters across restarts,
and counts uploads and analysis polls as requests. Automatic hash checks MUST be off by default and,
when enabled, MUST use a capped budget with requests reserved for manual actions. The user MUST be
able to see remaining quota, queued work, and the reason for any pause. The app MUST back off on
HTTP 429, MUST NOT retry storms, and MUST NOT hide quota exhaustion behind generic errors.

**Rationale**: The free tier is small and shared by all activity. A single unbounded monitor can
exhaust the day's budget and make manual checks fail.

### IV. Testable, Modular Core

Business logic MUST live in a platform-independent library that can be tested without a desktop
session or a live VirusTotal account. Windows shell code MUST be a thin adapter around that core.
Candidate detection, fingerprinting, caching, quota accounting, upload state transitions, and error
mapping MUST have deterministic unit tests. Integration tests MUST use a local mock service and MUST
NOT consume the user's API quota or send real files. A change is not complete until formatting,
linting, and tests pass.

**Rationale**: Native tray code is hard to test; isolating it keeps safety-critical behavior
verifiable and regressions cheap to catch.

### V. Least Privilege, Local-First, No Destructive Actions

The app MUST run without administrator rights, MUST write only to per-user locations, and MUST
persist only what the feature needs (settings, cache, quota counters, and recent results). It MUST
NOT enable telemetry or remote logging. It MUST NOT quarantine, delete, block, or modify scanned
files based on VirusTotal results; VirusTotal is one reputation signal, not a local verdict or an
antivirus engine. Removing the app or clearing its data MUST be straightforward and MUST remove
autostart and context-menu registrations.

**Rationale**: Reputation data can be wrong or stale. Destructive automation would turn a helper
into a new risk.

## Additional Constraints

- **Platform**: Windows 10/11 x64, per-user desktop session. No Windows service, pre-login agent, or
  multi-user server mode in v1.
- **Stack**: Rust stable; native Win32 integration; blocking HTTP with TLS; an embedded local
  database; no Electron, Tauri, WebView2, or GPU-rendered UI framework.
- **VirusTotal access**: Free API v3 only. No premium-only endpoints. Manual uploads only, up to the
  service maximum (650 MB); files above 200 MB require an explicit warning.
- **Monitoring scope**: User-selected folders only in v1. Whole-volume real-time monitoring via the
  NTFS USN journal is deferred.
- **Default behavior**: Monitoring and local detection are on; automatic hash checks are off until
  enabled; automatic uploads are prohibited.
- **Data handling**: Credential in the OS credential vault; configuration, cache, and logs under
  per-user application data directories; no secrets in the repository or test fixtures.
- **Language**: English UI and documentation for v1.

## Development Workflow

- Spec-Driven Development order: constitution -> specification -> plan -> tasks -> implementation ->
  convergence. Implementation MUST NOT begin before the plan and tasks are reviewed.
- Quality gates: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`
  MUST pass. Release builds MUST also pass a manual smoke test on Windows.
- Test policy: Unit tests for core logic; integration tests against a local mock VirusTotal service;
  manual end-to-end checks with a user-supplied key and non-sensitive test files. Live API calls are
  forbidden in automated tests.
- Security review: Before the first release, verify credential handling, log redaction, upload
  consent, path handling, and registry changes. Any new external data flow requires a spec update.
- Change control: Changes to upload consent, privacy behavior, quota policy, or destructive actions
  require a constitution review and a specification update before implementation.

## Governance

This constitution supersedes conflicting local practices. Amendments MUST be documented in the
specification set, reviewed, and accompanied by a semantic version bump. Versioning follows
MAJOR for incompatible governance changes, MINOR for new or materially expanded principles, and
PATCH for clarifications. Every plan MUST include a Constitution Check; violations MUST be recorded
in Complexity Tracking with a rejected simpler alternative. Compliance is reviewed at each feature
completion and before any public release.

**Version**: 1.0.0 | **Ratified**: 2026-11-18 | **Last Amended**: 2026-11-18
