# Data Model: VT Tray - Always-On VirusTotal Companion

**Date**: 2026-11-18

**Storage split**:
- Preferences and watched folders: `%APPDATA%\VTTray\config.json`
- Cache, detections, upload jobs, and quota ledger: `%LOCALAPPDATA%\VTTray\cache.db` (SQLite)
- API credential: Windows Credential Manager only (`VTTray/api-key`)
- Logs: `%LOCALAPPDATA%\VTTray\logs` (rotating; never contain the credential)

All timestamps are Unix epoch seconds in UTC unless a field name says otherwise. Paths are stored
locally only; they are never sent to VirusTotal.

## Entities

### WatchedFolder (config)

| Field | Type | Constraints |
|-------|------|-------------|
| `path` | string | Absolute, normalized; must exist and be a directory when added; volume roots rejected |
| `enabled` | boolean | Default true |
| `added_at` | integer | UTC epoch seconds |
| `last_checked_at` | integer | Nullable; last reconciliation scan |

Relationships: one watched folder produces many detections. Removing a folder does not delete its
historical detections unless the user clears history.

### FileRecord (SQLite `files`)

One row per content fingerprint. This is the reputation cache.

| Field | Type | Constraints |
|-------|------|-------------|
| `sha256` | text | Primary key; exactly 64 lowercase hex characters |
| `size_bytes` | integer | >= 0 |
| `verdict_state` | text | `unknown`, `known`, `stale`, `error` |
| `reputation` | integer | Nullable; service score if present |
| `malicious_count` | integer | >= 0 |
| `suspicious_count` | integer | >= 0 |
| `undetected_count` | integer | >= 0 |
| `harmless_count` | integer | >= 0 |
| `timeout_count` | integer | >= 0 |
| `type_description` | text | Nullable; service-provided type |
| `meaningful_name` | text | Nullable; service-provided name |
| `first_submission_at` | integer | Nullable |
| `last_analysis_at` | integer | Nullable |
| `last_checked_at` | integer | Not null |
| `summary_json` | text | Nullable; bounded minimal summary, not the raw API response |

Validation:
- `sha256` must be lowercase hex; reject anything else.
- Counts are non-negative and are replaced atomically on refresh.
- `summary_json` is capped (for example 16 KB) and contains only display fields.
- A `known` row older than 7 days (the freshness threshold) is treated as `stale` for display.

### PathRecord (SQLite `paths`)

Maps a local path to a fingerprint to avoid re-hashing unchanged files.

| Field | Type | Constraints |
|-------|------|-------------|
| `path` | text | Primary key; normalized local absolute path |
| `size_bytes` | integer | >= 0 |
| `mtime_ns` | integer | Last modification time in nanoseconds since epoch |
| `sha256` | text | Nullable; foreign key to `files.sha256` |
| `last_seen_at` | integer | Not null |

Validation:
- Invalidate and re-hash if `size_bytes` or `mtime_ns` differs.
- Delete the row if the path no longer exists during reconciliation.
- Never send `path` to the service.

### Detection (SQLite `detections`)

A user-visible detection event. Deduplicated by `(path, sha256)` while in a non-terminal state.

| Field | Type | Constraints |
|-------|------|-------------|
| `id` | integer | Primary key, autoincrement |
| `path` | text | Not null |
| `sha256` | text | Nullable until hashed |
| `base_name` | text | Not null; safe for display |
| `size_bytes` | integer | Nullable |
| `detected_at` | integer | Not null |
| `state` | text | `new`, `hashing`, `checked`, `unknown`, `reported`, `dismissed`, `error` |
| `notified_at` | integer | Nullable |
| `last_error` | text | Nullable; redacted |

State transitions:
- `new -> hashing -> checked | unknown | error`
- `unknown -> reported` when the user uploads, or `dismissed` when ignored.
- `checked -> reported` only after a manual upload; a check itself never uploads.
- `error -> hashing` on a user-requested retry.

Retention: 30 days or the newest 1,000 rows, whichever is smaller.

### UploadJob (SQLite `uploads`)

| Field | Type | Constraints |
|-------|------|-------------|
| `id` | integer | Primary key, autoincrement |
| `sha256` | text | Not null; 64 lowercase hex |
| `base_name` | text | Not null; sent to the service instead of the full path |
| `size_bytes` | integer | 0..681,574,400 (650 MiB) |
| `consent_at` | integer | Not null; the explicit confirmation time |
| `status` | text | `pending`, `uploading`, `analyzing`, `completed`, `failed`, `canceled` |
| `analysis_id` | text | Nullable; service analysis reference |
| `upload_url_used` | boolean | True when the large-file flow was used |
| `attempts` | integer | >= 0; capped by policy |
| `created_at` | integer | Not null |
| `updated_at` | integer | Not null |
| `last_error` | text | Nullable; redacted |

State transitions:
- `pending -> uploading -> analyzing -> completed`
- `pending | uploading | analyzing -> failed` after a terminal error
- `pending | uploading -> canceled` by the user
- `failed -> pending` only by an explicit retry; never automatically
- `completed` is terminal

Validation:
- An upload row cannot exist without `consent_at`.
- Files above 200 MiB require an explicit large-file warning before `pending`.
- Files above 650 MiB are rejected before consent.
- A canceled upload never resumes automatically after restart.

### QuotaEvent (SQLite `quota_events`)

One row per service request, written before the request is sent.

| Field | Type | Constraints |
|-------|------|-------------|
| `id` | integer | Primary key, autoincrement |
| `ts` | integer | UTC epoch seconds; not null |
| `endpoint` | text | Logical operation: `file_lookup`, `file_upload`, `upload_url`, `analysis_poll`, `rescan` |
| `cost` | integer | Default 1; reserved for future weighted costs |
| `outcome` | text | `reserved`, `ok`, `http_error`, `network_error`, `canceled` |

Quota windows are derived from these rows:
- minute: last 60 seconds, max 4
- day: current UTC day, max 500
- month: current UTC month, max 15,500
- manual reserve: at least 100 daily units are unavailable to automatic checks

Retention: 40 days so the monthly window and diagnostics survive month boundaries.

### QuotaState (SQLite `quota_state`)

Small key/value table for reset markers and pause reasons.

| Field | Type | Constraints |
|-------|------|-------------|
| `key` | text | Primary key |
| `value` | text | Not null |
| `updated_at` | integer | Not null |

Keys: `day_reset_at`, `month_reset_at`, `paused_reason`, `paused_until`, `last_429_at`.

### Preference (config)

| Field | Type | Default |
|-------|------|---------|
| `watched_folders` | array of WatchedFolder | empty |
| `auto_check_enabled` | boolean | false |
| `notifications_enabled` | boolean | true |
| `autostart_enabled` | boolean | false |
| `monitoring_paused` | boolean | false |
| `cache_retention_days` | integer | 30 |
| `log_level` | string | `info` |
| `schema_version` | integer | 1 |

The credential is never part of this file.

### CredentialState (in memory only)

| Field | Type | Meaning |
|-------|------|---------|
| `status` | enum | `present`, `absent`, `invalid` |
| `last_validated_at` | integer | Nullable |

Only the status is cached in memory; the secret is read on demand and zeroized after use where
practical.

## Relationships

```text
WatchedFolder 1 ── * Detection
FileRecord    1 ── * PathRecord
FileRecord    1 ── * UploadJob
Detection     * ── 1 FileRecord (by sha256, once hashed)
UploadJob     * ── 1 FileRecord (by sha256)
QuotaEvent    independent ledger
Preference    1 ── * WatchedFolder
```

## End-to-end flows

1. **Detection**: watcher event -> stabilization -> path cache -> SHA-256 -> `FileRecord` upsert if
   new -> `Detection` row -> notify. No service request unless auto-check is enabled.
2. **Manual check**: file picker -> SHA-256 -> path cache -> `QuotaGate` reserve ->
   `GET /files/{sha256}` -> `FileRecord` upsert -> display summary. No upload.
3. **Manual upload**: picker or recent item -> check existing report -> consent dialog -> insert
   `UploadJob(pending)` -> `QuotaGate` reserve -> upload -> `UploadJob(analyzing)` -> poll analysis
   -> `UploadJob(completed)` -> notify and store summary.
4. **Quota pause**: reserve fails -> no request is sent -> queue remains pending -> UI shows the
   reason and reset time.
5. **Credential removal**: credential deleted from vault -> status `absent` -> online actions
   disabled -> monitoring and local cache remain.

## Bounded retention

- `paths`: 90 days since last seen, or 10,000 rows.
- `files`: 30 days since last checked unless referenced by a pending upload or a recent detection.
- `detections`: 30 days or 1,000 rows.
- `uploads`: 90 days; failed/canceled rows older than 30 days may be pruned.
- `quota_events`: 40 days.
- Logs: 7 days or 10 MB per file, whichever is smaller.
- Database: checkpoint WAL and vacuum after pruning when fragmentation is high.

## Privacy rules

- Full local paths never leave the machine; only the base name and SHA-256 are sent on upload.
- Logs redact `x-apikey`, authorization data, and request headers.
- No telemetry, remote logging, or crash upload.
- Clearing data removes cache/history but does not remove watched folders unless the user resets
  preferences; removing the credential is a separate explicit action.
