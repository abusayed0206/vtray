use crate::core::errors::AppError;
use crate::core::types::{
    Detection, DetectionState, FileRecord, PathRecord, QuotaEndpoint, QuotaEvent, QuotaOutcome,
    UploadJob, UploadStatus, VerdictState,
};
use rusqlite::{Connection, OptionalExtension, Row, params};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub struct Cache {
    conn: Mutex<Connection>,
}

impl Cache {
    pub fn open(path: &Path) -> Result<Self, AppError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        let cache = Self {
            conn: Mutex::new(conn),
        };
        cache.configure()?;
        Ok(cache)
    }

    pub fn in_memory() -> Result<Self, AppError> {
        let conn = Connection::open_in_memory()?;
        let cache = Self {
            conn: Mutex::new(conn),
        };
        cache.configure()?;
        Ok(cache)
    }

    fn configure(&self) -> Result<(), AppError> {
        let conn = self.lock()?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA foreign_keys=ON;
             PRAGMA cache_size=-2000;
             PRAGMA temp_store=MEMORY;
             PRAGMA busy_timeout=5000;",
        )?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS meta (
                 key TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS files (
                 sha256 TEXT PRIMARY KEY,
                 size_bytes INTEGER NOT NULL,
                 verdict_state TEXT NOT NULL,
                 reputation INTEGER,
                 malicious_count INTEGER NOT NULL DEFAULT 0,
                 suspicious_count INTEGER NOT NULL DEFAULT 0,
                 undetected_count INTEGER NOT NULL DEFAULT 0,
                 harmless_count INTEGER NOT NULL DEFAULT 0,
                 timeout_count INTEGER NOT NULL DEFAULT 0,
                 type_description TEXT,
                 meaningful_name TEXT,
                 first_submission_at INTEGER,
                 last_analysis_at INTEGER,
                 last_checked_at INTEGER NOT NULL,
                 summary_json TEXT
             );
             CREATE TABLE IF NOT EXISTS paths (
                 path TEXT PRIMARY KEY,
                 size_bytes INTEGER NOT NULL,
                 mtime_ns INTEGER NOT NULL,
                 sha256 TEXT,
                 last_seen_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS detections (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 path TEXT NOT NULL,
                 sha256 TEXT,
                 base_name TEXT NOT NULL,
                 size_bytes INTEGER,
                 detected_at INTEGER NOT NULL,
                 state TEXT NOT NULL,
                 notified_at INTEGER,
                 last_error TEXT
             );
             CREATE TABLE IF NOT EXISTS uploads (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 sha256 TEXT NOT NULL,
                 base_name TEXT NOT NULL,
                 size_bytes INTEGER NOT NULL,
                 consent_at INTEGER NOT NULL,
                 status TEXT NOT NULL,
                 analysis_id TEXT,
                 upload_url_used INTEGER NOT NULL DEFAULT 0,
                 attempts INTEGER NOT NULL DEFAULT 0,
                 created_at INTEGER NOT NULL,
                 updated_at INTEGER NOT NULL,
                 last_error TEXT
             );
             CREATE TABLE IF NOT EXISTS quota_events (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 ts INTEGER NOT NULL,
                 endpoint TEXT NOT NULL,
                 cost INTEGER NOT NULL DEFAULT 1,
                 outcome TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS quota_state (
                 key TEXT PRIMARY KEY,
                 value TEXT NOT NULL,
                 updated_at INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_paths_sha256 ON paths(sha256);
             CREATE INDEX IF NOT EXISTS idx_detections_time ON detections(detected_at);
             CREATE INDEX IF NOT EXISTS idx_uploads_status ON uploads(status);
             CREATE INDEX IF NOT EXISTS idx_quota_events_ts ON quota_events(ts);",
        )?;
        conn.execute(
            "INSERT INTO meta(key, value) VALUES('schema_version', ?1)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![crate::core::settings::SCHEMA_VERSION.to_string()],
        )?;
        Ok(())
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, AppError> {
        self.conn
            .lock()
            .map_err(|_| AppError::Internal("database lock was poisoned".to_string()))
    }

    pub fn upsert_file(&self, record: &FileRecord) -> Result<(), AppError> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO files (
                 sha256, size_bytes, verdict_state, reputation, malicious_count,
                 suspicious_count, undetected_count, harmless_count, timeout_count,
                 type_description, meaningful_name, first_submission_at, last_analysis_at,
                 last_checked_at, summary_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
             ON CONFLICT(sha256) DO UPDATE SET
                 size_bytes=excluded.size_bytes,
                 verdict_state=excluded.verdict_state,
                 reputation=excluded.reputation,
                 malicious_count=excluded.malicious_count,
                 suspicious_count=excluded.suspicious_count,
                 undetected_count=excluded.undetected_count,
                 harmless_count=excluded.harmless_count,
                 timeout_count=excluded.timeout_count,
                 type_description=excluded.type_description,
                 meaningful_name=excluded.meaningful_name,
                 first_submission_at=excluded.first_submission_at,
                 last_analysis_at=excluded.last_analysis_at,
                 last_checked_at=excluded.last_checked_at,
                 summary_json=excluded.summary_json",
            params![
                record.sha256,
                record.size_bytes as i64,
                record.verdict_state.as_str(),
                record.reputation,
                record.malicious_count as i64,
                record.suspicious_count as i64,
                record.undetected_count as i64,
                record.harmless_count as i64,
                record.timeout_count as i64,
                record.type_description,
                record.meaningful_name,
                record.first_submission_at,
                record.last_analysis_at,
                record.last_checked_at,
                record.summary_json,
            ],
        )?;
        Ok(())
    }

    pub fn get_file(&self, sha256: &str) -> Result<Option<FileRecord>, AppError> {
        let conn = self.lock()?;
        let record = conn
            .query_row(
                "SELECT sha256, size_bytes, verdict_state, reputation, malicious_count,
                        suspicious_count, undetected_count, harmless_count, timeout_count,
                        type_description, meaningful_name, first_submission_at, last_analysis_at,
                        last_checked_at, summary_json
                 FROM files WHERE sha256 = ?1",
                params![sha256],
                file_from_row,
            )
            .optional()?;
        Ok(record)
    }

    pub fn upsert_path(&self, record: &PathRecord) -> Result<(), AppError> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO paths(path, size_bytes, mtime_ns, sha256, last_seen_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(path) DO UPDATE SET
                 size_bytes=excluded.size_bytes,
                 mtime_ns=excluded.mtime_ns,
                 sha256=excluded.sha256,
                 last_seen_at=excluded.last_seen_at",
            params![
                path_key(&record.path),
                record.size_bytes as i64,
                record.mtime_ns as i64,
                record.sha256,
                record.last_seen_at,
            ],
        )?;
        Ok(())
    }

    pub fn get_path(&self, path: &Path) -> Result<Option<PathRecord>, AppError> {
        let conn = self.lock()?;
        let record = conn
            .query_row(
                "SELECT path, size_bytes, mtime_ns, sha256, last_seen_at FROM paths WHERE path = ?1",
                params![path_key(path)],
                |row| {
                    Ok(PathRecord {
                        path: PathBuf::from(row.get::<_, String>(0)?),
                        size_bytes: row.get::<_, i64>(1)? as u64,
                        mtime_ns: row.get::<_, i64>(2)? as i128,
                        sha256: row.get(3)?,
                        last_seen_at: row.get(4)?,
                    })
                },
            )
            .optional()?;
        Ok(record)
    }

    pub fn delete_path(&self, path: &Path) -> Result<(), AppError> {
        let conn = self.lock()?;
        conn.execute("DELETE FROM paths WHERE path = ?1", params![path_key(path)])?;
        Ok(())
    }

    pub fn insert_detection(
        &self,
        path: &Path,
        sha256: Option<&str>,
        base_name: &str,
        size_bytes: Option<u64>,
        detected_at: i64,
        state: DetectionState,
    ) -> Result<i64, AppError> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO detections(path, sha256, base_name, size_bytes, detected_at, state)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                path_key(path),
                sha256,
                base_name,
                size_bytes.map(|size| size as i64),
                detected_at,
                state.as_str(),
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn update_detection_state(
        &self,
        id: i64,
        state: DetectionState,
        notified_at: Option<i64>,
        error: Option<&str>,
    ) -> Result<(), AppError> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE detections SET state=?1, notified_at=COALESCE(?2, notified_at), last_error=?3
             WHERE id=?4",
            params![state.as_str(), notified_at, error, id],
        )?;
        Ok(())
    }

    pub fn recent_detections(&self, limit: u32) -> Result<Vec<Detection>, AppError> {
        let conn = self.lock()?;
        let mut statement = conn.prepare(
            "SELECT id, path, sha256, base_name, size_bytes, detected_at, state, notified_at,
                    last_error
             FROM detections ORDER BY detected_at DESC, id DESC LIMIT ?1",
        )?;
        let rows = statement.query_map(params![limit as i64], detection_from_row)?;
        let mut detections = Vec::new();
        for row in rows {
            detections.push(row?);
        }
        Ok(detections)
    }

    pub fn find_open_detection(
        &self,
        path: &Path,
        sha256: &str,
    ) -> Result<Option<Detection>, AppError> {
        let conn = self.lock()?;
        let record = conn
            .query_row(
                "SELECT id, path, sha256, base_name, size_bytes, detected_at, state, notified_at,
                        last_error
                 FROM detections
                 WHERE path=?1 AND sha256=?2 AND state NOT IN ('dismissed', 'reported')
                 ORDER BY id DESC LIMIT 1",
                params![path_key(path), sha256],
                detection_from_row,
            )
            .optional()?;
        Ok(record)
    }

    pub fn insert_upload(&self, job: &UploadJob) -> Result<i64, AppError> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO uploads(
                 sha256, base_name, size_bytes, consent_at, status, analysis_id,
                 upload_url_used, attempts, created_at, updated_at, last_error
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                job.sha256,
                job.base_name,
                job.size_bytes as i64,
                job.consent_at,
                job.status.as_str(),
                job.analysis_id,
                job.upload_url_used as i64,
                job.attempts as i64,
                job.created_at,
                job.updated_at,
                job.last_error,
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn get_upload(&self, id: i64) -> Result<Option<UploadJob>, AppError> {
        let conn = self.lock()?;
        let job = conn
            .query_row(
                "SELECT id, sha256, base_name, size_bytes, consent_at, status, analysis_id,
                        upload_url_used, attempts, created_at, updated_at, last_error
                 FROM uploads WHERE id=?1",
                params![id],
                upload_from_row,
            )
            .optional()?;
        Ok(job)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_upload(
        &self,
        id: i64,
        status: UploadStatus,
        analysis_id: Option<&str>,
        upload_url_used: bool,
        attempts: u32,
        updated_at: i64,
        last_error: Option<&str>,
    ) -> Result<(), AppError> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE uploads SET status=?1, analysis_id=COALESCE(?2, analysis_id),
                 upload_url_used=?3, attempts=?4, updated_at=?5, last_error=?6
             WHERE id=?7",
            params![
                status.as_str(),
                analysis_id,
                upload_url_used as i64,
                attempts as i64,
                updated_at,
                last_error,
                id,
            ],
        )?;
        Ok(())
    }

    pub fn resumable_uploads(&self) -> Result<Vec<UploadJob>, AppError> {
        let conn = self.lock()?;
        let mut statement = conn.prepare(
            "SELECT id, sha256, base_name, size_bytes, consent_at, status, analysis_id,
                    upload_url_used, attempts, created_at, updated_at, last_error
             FROM uploads WHERE status IN ('pending', 'uploading', 'analyzing')
             ORDER BY id",
        )?;
        let rows = statement.query_map([], upload_from_row)?;
        let mut jobs = Vec::new();
        for row in rows {
            jobs.push(row?);
        }
        Ok(jobs)
    }

    pub fn insert_quota_event(
        &self,
        ts: i64,
        endpoint: QuotaEndpoint,
        outcome: QuotaOutcome,
    ) -> Result<i64, AppError> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO quota_events(ts, endpoint, cost, outcome) VALUES (?1, ?2, 1, ?3)",
            params![ts, endpoint.as_str(), outcome.as_str()],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn update_quota_outcome(&self, id: i64, outcome: QuotaOutcome) -> Result<(), AppError> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE quota_events SET outcome=?1 WHERE id=?2",
            params![outcome.as_str(), id],
        )?;
        Ok(())
    }

    pub fn count_quota_events(&self, since: i64) -> Result<u32, AppError> {
        self.count_quota_events_between(since, i64::MAX)
    }

    /// Counts requests with `since <= ts < until`, ignoring canceled reservations.
    pub fn count_quota_events_between(&self, since: i64, until: i64) -> Result<u32, AppError> {
        let conn = self.lock()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM quota_events WHERE ts >= ?1 AND ts < ?2 AND outcome != 'canceled'",
            params![since, until],
            |row| row.get(0),
        )?;
        Ok(count as u32)
    }

    pub fn quota_events(&self, since: i64) -> Result<Vec<QuotaEvent>, AppError> {
        let conn = self.lock()?;
        let mut statement = conn.prepare(
            "SELECT id, ts, endpoint, outcome FROM quota_events WHERE ts >= ?1 ORDER BY ts",
        )?;
        let rows = statement.query_map(params![since], |row| {
            let endpoint: String = row.get(2)?;
            let outcome: String = row.get(3)?;
            Ok(QuotaEvent {
                id: row.get(0)?,
                ts: row.get(1)?,
                endpoint: QuotaEndpoint::from_str(&endpoint).unwrap_or(QuotaEndpoint::FileLookup),
                outcome: QuotaOutcome::from_str(&outcome).unwrap_or(QuotaOutcome::HttpError),
            })
        })?;
        let mut events = Vec::new();
        for row in rows {
            events.push(row?);
        }
        Ok(events)
    }

    pub fn get_quota_state(&self, key: &str) -> Result<Option<String>, AppError> {
        let conn = self.lock()?;
        let value = conn
            .query_row(
                "SELECT value FROM quota_state WHERE key=?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?;
        Ok(value)
    }

    pub fn set_quota_state(&self, key: &str, value: &str, now: i64) -> Result<(), AppError> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO quota_state(key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
            params![key, value, now],
        )?;
        Ok(())
    }

    pub fn clear_history(&self) -> Result<(), AppError> {
        let conn = self.lock()?;
        conn.execute_batch("DELETE FROM detections; DELETE FROM uploads;")?;
        Ok(())
    }

    pub fn clear_cache(&self) -> Result<(), AppError> {
        let conn = self.lock()?;
        conn.execute_batch(
            "DELETE FROM detections;
             DELETE FROM uploads;
             DELETE FROM paths;
             DELETE FROM files;",
        )?;
        Ok(())
    }

    /// Deletes every stored row, quota counters included. Only for removing VT Tray from the PC;
    /// "Clear local data" keeps quota counters so VirusTotal's limits stay respected.
    pub fn wipe_all(&self) -> Result<(), AppError> {
        let conn = self.lock()?;
        conn.execute_batch(
            "DELETE FROM detections;
             DELETE FROM uploads;
             DELETE FROM paths;
             DELETE FROM files;
             DELETE FROM quota_events;
             DELETE FROM quota_state;",
        )?;
        Ok(())
    }

    pub fn prune(&self, now: i64, retention_days: u32) -> Result<(), AppError> {
        let cutoff = now.saturating_sub((retention_days.clamp(1, 365) as i64) * 86_400);
        let quota_cutoff = now.saturating_sub(40 * 86_400);
        let conn = self.lock()?;
        conn.execute(
            "DELETE FROM paths WHERE last_seen_at < ?1
             AND path NOT IN (SELECT path FROM detections WHERE detected_at >= ?1)",
            params![cutoff],
        )?;
        conn.execute(
            "DELETE FROM detections WHERE detected_at < ?1",
            params![cutoff],
        )?;
        conn.execute(
            "DELETE FROM uploads WHERE updated_at < ?1
             AND status IN ('completed', 'failed', 'canceled')",
            params![cutoff],
        )?;
        conn.execute(
            "DELETE FROM quota_events WHERE ts < ?1",
            params![quota_cutoff],
        )?;
        conn.execute(
            "DELETE FROM files WHERE last_checked_at < ?1
             AND sha256 NOT IN (SELECT sha256 FROM paths WHERE sha256 IS NOT NULL)
             AND sha256 NOT IN (SELECT sha256 FROM uploads WHERE status IN ('pending', 'uploading', 'analyzing'))",
            params![cutoff],
        )?;
        Ok(())
    }

    pub fn file_count(&self) -> Result<u32, AppError> {
        let conn = self.lock()?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))?;
        Ok(count as u32)
    }
}

fn path_key(path: &Path) -> String {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let text = canonical.to_string_lossy();
    #[cfg(windows)]
    {
        text.to_lowercase()
    }
    #[cfg(not(windows))]
    {
        text.into_owned()
    }
}

fn file_from_row(row: &Row<'_>) -> rusqlite::Result<FileRecord> {
    let verdict: String = row.get(2)?;
    Ok(FileRecord {
        sha256: row.get(0)?,
        size_bytes: row.get::<_, i64>(1)? as u64,
        verdict_state: VerdictState::from_str(&verdict).unwrap_or(VerdictState::Error),
        reputation: row.get(3)?,
        malicious_count: row.get::<_, i64>(4)? as u32,
        suspicious_count: row.get::<_, i64>(5)? as u32,
        undetected_count: row.get::<_, i64>(6)? as u32,
        harmless_count: row.get::<_, i64>(7)? as u32,
        timeout_count: row.get::<_, i64>(8)? as u32,
        type_description: row.get(9)?,
        meaningful_name: row.get(10)?,
        first_submission_at: row.get(11)?,
        last_analysis_at: row.get(12)?,
        last_checked_at: row.get(13)?,
        summary_json: row.get(14)?,
    })
}

fn detection_from_row(row: &Row<'_>) -> rusqlite::Result<Detection> {
    let state: String = row.get(6)?;
    Ok(Detection {
        id: row.get(0)?,
        path: PathBuf::from(row.get::<_, String>(1)?),
        sha256: row.get(2)?,
        base_name: row.get(3)?,
        size_bytes: row.get::<_, Option<i64>>(4)?.map(|size| size as u64),
        detected_at: row.get(5)?,
        state: DetectionState::from_str(&state).unwrap_or(DetectionState::Error),
        notified_at: row.get(7)?,
        last_error: row.get(8)?,
    })
}

fn upload_from_row(row: &Row<'_>) -> rusqlite::Result<UploadJob> {
    let status: String = row.get(5)?;
    Ok(UploadJob {
        id: row.get(0)?,
        sha256: row.get(1)?,
        base_name: row.get(2)?,
        size_bytes: row.get::<_, i64>(3)? as u64,
        consent_at: row.get(4)?,
        status: UploadStatus::from_str(&status).unwrap_or(UploadStatus::Failed),
        analysis_id: row.get(6)?,
        upload_url_used: row.get::<_, i64>(7)? != 0,
        attempts: row.get::<_, i64>(8)? as u32,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        last_error: row.get(11)?,
    })
}
