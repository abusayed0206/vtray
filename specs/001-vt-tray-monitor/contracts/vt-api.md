# Contract: VirusTotal API v3 Client

**Date**: 2026-11-18

**Base URL**: `https://www.virustotal.com/api/v3`

**Authentication**: `x-apikey: <key>`, read from Windows Credential Manager immediately before a
request. The key never appears in logs, URLs, configuration, command lines, or error text.

**Client**: blocking `ureq` 3.5.2 `Agent`; one agent owned by the API worker. Timeouts:
connect 10 s, overall 120 s for lookups and 30 min for uploads. TLS via rustls defaults.

**Quota**: every request below reserves one unit from the single `QuotaGate` before sending:
4/minute sliding, 500/day, 15.5K/month, with 100 daily units reserved for manual actions. A denied
reservation returns a local quota error and sends nothing.

## Operations

### 1. Lookup by hash

```
GET /files/{sha256}
```

- Path parameter: 64 lowercase hex characters. Reject other formats locally.
- Success: HTTP 200 with `data` object. Required display fields:
  - `data.id` (SHA-256)
  - `data.type`
  - `data.attributes.last_analysis_stats` (`malicious`, `suspicious`, `undetected`, `harmless`,
    `timeout`)
  - `data.attributes.reputation`
  - `data.attributes.type_description`
  - `data.attributes.meaningful_name`
  - `data.attributes.first_submission_date`
  - `data.attributes.last_analysis_date`
  - up to five `data.attributes.last_analysis_results` entries with detections, bounded and
    redacted for display
- Unknown file: HTTP 404. This is a normal result, not an error.
- Parse defensively: missing optional fields become null/zero; a malformed success body is a
  protocol error and does not overwrite a previously good cache entry.
- Never use `GET /files/{id}/download_url` (premium-only).

### 2. Rescan an existing file

```
POST /files/{sha256}/analyse
```

- No body.
- Success: HTTP 200 with an `analysis` identifier in `data.id` (or equivalent documented shape).
- Used only after an explicit user action on a stale known file. Rate-limited by the same quota gate.
- If the service returns 404, treat the file as no longer known; offer a manual upload instead.

### 3. Upload a small file (direct)

```
POST /files
Content-Type: multipart/form-data; boundary=<random>
Content-Length: <exact byte length>
```

- Only for files `<= 33,554,432` bytes (32 MiB).
- One part, field name `file`, filename = base name only:
  - `Content-Disposition: form-data; name="file"; filename="<escaped base name>"`
  - `Content-Type: application/octet-stream`
  - raw file bytes
- The body is streamed; the file is never read fully into memory.
- Success: HTTP 200 with `data.type == "analysis"` and `data.id` (analysis ID).
- `filename` is sanitized: no path separators, no control characters, no quotes/backslashes; fallback
  to `upload.bin` if empty or unsafe.
- Uploaded content is shared with the security community; the consent dialog must have been
  accepted and `consent_at` persisted before this call.

### 4. Request a large-file upload URL

```
GET /files/upload_url
```

- Used for files `> 32 MiB` and `<= 681,574,400` bytes (650 MiB).
- Success: HTTP 200 with a one-time `data` URL.
- The URL is used exactly once, immediately, and is never logged. It is not persisted.
- Do not send the file to the base `/files` endpoint when using this flow.

### 5. Upload to the returned URL

```
POST <upload_url>
Content-Type: multipart/form-data; boundary=<random>
Content-Length: <exact byte length>
```

- Same multipart shape and streaming rules as operation 3.
- Success: HTTP 200 with `data.type == "analysis"` and `data.id`.
- The returned URL may already contain query parameters; do not append or rewrite them.

### 6. Poll analysis

```
GET /analyses/{analysis_id}
```

- Success: HTTP 200 with `data.attributes.status` (`queued`, `in-progress`, `completed`) and
  `data.attributes.stats` when completed.
- Poll schedule: 10 s, 30 s, 60 s, 120 s, 240 s, 300 s. Stop after six polls and mark the job
  `failed` with a retryable timeout reason. Never poll in a tight loop.
- A completed analysis yields the file SHA-256 and stats; upsert `FileRecord` and set the job
  `completed`.
- Each poll consumes one quota unit.

## Error mapping

| Condition | Local behavior |
|-----------|----------------|
| 200 | Parse; update cache/job |
| 400 | Protocol error; show service message if safe; do not retry automatically |
| 401/403 | Mark credential invalid; stop online actions; prompt to replace key |
| 404 lookup | Normal unknown result; no retry |
| 404 upload/rescan | Endpoint/file unavailable; explain and offer manual retry |
| 409 | Conflict/busy; back off and surface a retryable error |
| 429 | Honor `Retry-After`; exponential backoff 60/120/300 s; pause queue |
| 5xx | Retry up to 3 times with capped exponential backoff and jitter |
| Network/DNS/TLS | Retry transient work; keep local monitoring; no credential change |
| Timeout | Same as network; never auto-resume a canceled upload |
| Unauthorized/forbidden response body | Redact any key-like values before display/logging |

Retries never create duplicate uploads: the upload job state and consent row are committed before the
request, and a failed upload requires an explicit user retry.

## Multipart encoding rules

- Boundary: 32 random hex characters, generated per request.
- Exact `Content-Length = preamble + file_size + epilogue`.
- Preamble:
  ```
  --<boundary>\r\n
  Content-Disposition: form-data; name="file"; filename="<base_name>"\r\n
  Content-Type: application/octet-stream\r\n
  \r\n
  ```
- Epilogue:
  ```
  \r\n--<boundary>--\r\n
  ```
- Header values contain no CR/LF. Filename escaping follows RFC 7578 and is tested with quotes,
  spaces, non-ASCII, and path separators.
- The file is opened read-only and streamed; deletion or truncation during upload fails the job.

## Data minimization

- Lookup: SHA-256 only.
- Upload: file bytes, base name, and the API key header. No local path, username, hostname, or
  telemetry.
- Logs: request method, endpoint template, status, duration, and byte counts only. Never headers or
  query strings from upload URLs. Base names and fingerprints may appear in local logs (FR-013).

## Contract test cases

- Known file 200 with full and minimal attributes.
- Unknown file 404.
- Malformed 200 body.
- 401 and 403.
- 429 with and without `Retry-After`.
- 500 then 200 retry.
- Multipart exact bytes and `Content-Length` for empty, 1-byte, 32 MiB boundary, and large-file
  URL flows.
- Filename sanitization and base-name-only guarantee.
- Analysis status transition to `completed` and six-poll timeout.
- Quota denial before any socket write.
