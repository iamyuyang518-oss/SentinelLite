//! SQLite persistence for normalized process records.
//!
//! Process events, invalid records, pairing revisions and session ends are
//! written in one transaction per batch. Event IDs are stable, so replaying a
//! batch is idempotent. A pairing revision updates the original stop record
//! in place; if the revision is seen before the stop record (for example after
//! an application restart), it is retained and applied when the stop arrives.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use chrono::{Duration as ChronoDuration, SecondsFormat, Utc};
use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};
use sentinel_events::{
    PairingOrder, PairingRevision, PairingStatus, ProcessEvent, ProcessEventType, SessionEnd,
    time::{now_unix_ms, unix_ms_to_rfc3339},
};
use sentinel_ipc::events::EventBatch;
use serde::{Deserialize, Serialize};

use crate::retention::{MAX_SESSION_ROWS, RetentionPolicy, RetentionStats};

pub const SCHEMA_VERSION: i64 = 2;
pub const MAX_QUERY_LIMIT: u32 = 200;
pub const DEFAULT_QUERY_LIMIT: u32 = 100;
pub const MAX_QUERY_OFFSET: u32 = 100_000;
pub const MAX_FILTER_CHARS: usize = 160;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("storage database could not be opened: {0}")]
    Open(String),
    #[error("storage schema version {found} is unsupported (supported: {supported})")]
    UnsupportedSchema { found: i64, supported: i64 },
    #[error("storage database is corrupt: {0}")]
    Corrupt(String),
    #[error("storage query failed: {0}")]
    Query(String),
    #[error("storage retention failed: {0}")]
    Retention(String),
    #[error("invalid storage filter: {0}")]
    InvalidFilter(String),
}

fn query_error(error: rusqlite::Error) -> StorageError {
    StorageError::Query(error.to_string())
}

#[derive(Debug, Clone, Default)]
struct RuntimeStatus {
    last_ingest_at: Option<String>,
    last_retention_at: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct IngestStats {
    pub inserted_events: u64,
    pub inserted_invalid: u64,
    pub inserted_revisions: u64,
    pub inserted_session_ends: u64,
    pub revisions_applied: u64,
    pub gaps: u64,
    pub duplicates: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct StorageStatus {
    pub path: Option<String>,
    pub schema_version: i64,
    pub event_count: u64,
    pub invalid_count: u64,
    pub revision_count: u64,
    pub gap_count: u64,
    pub session_count: u64,
    pub oldest_event_at: Option<String>,
    pub newest_event_at: Option<String>,
    pub database_bytes: u64,
    pub retention: RetentionPolicy,
    pub last_ingest_at: Option<String>,
    pub last_retention_at: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventFilter {
    pub from: Option<String>,
    pub to: Option<String>,
    pub image: Option<String>,
    pub pid: Option<u32>,
    pub event_type: Option<String>,
    pub session_id: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoredEvent {
    pub event_id: String,
    pub session_id: String,
    pub sequence: u64,
    pub event_type: String,
    pub pid: u32,
    pub create_time: u64,
    pub image_name: Option<String>,
    pub event_time: Option<String>,
    pub occurred_at: Option<String>,
    pub received_at: Option<String>,
    pub normalized_at: Option<String>,
    pub pairing_state: String,
    pub pairing_order: Option<String>,
    pub paired_event_id: Option<String>,
    pub record: ProcessEvent,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventQueryResult {
    pub total: u64,
    pub limit: u32,
    pub offset: u32,
    pub events: Vec<StoredEvent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventDetail {
    pub event: StoredEvent,
    pub paired_event: Option<StoredEvent>,
    pub revisions: Vec<PairingRevision>,
    pub session_end: Option<SessionEnd>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionEventSet {
    pub session_id: String,
    pub events: Vec<ProcessEvent>,
    pub truncated: bool,
    pub invalid_records: u64,
    pub ingestion_gaps: u64,
    pub closed: bool,
    pub end_reason: Option<String>,
    pub session_end: Option<SessionEnd>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionRecord {
    pub session_id: String,
    pub first_seen_at: Option<String>,
    pub last_seen_at: Option<String>,
    pub last_cursor: u64,
    pub closed: bool,
    pub end_reason: Option<String>,
    pub ended_at: Option<String>,
    pub end_error: Option<String>,
    pub event_count: u64,
    pub has_session_end: bool,
}

pub struct Storage {
    connection: Mutex<Connection>,
    path: Option<PathBuf>,
    retention: RetentionPolicy,
    runtime: Mutex<RuntimeStatus>,
}

impl fmt::Debug for Storage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Storage")
            .field("path", &self.path)
            .field("retention", &self.retention)
            .finish_non_exhaustive()
    }
}

impl Storage {
    pub fn open(path: &Path, retention: RetentionPolicy) -> Result<Self, StorageError> {
        retention.validate().map_err(StorageError::InvalidFilter)?;
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|error| {
                    StorageError::Open(format!(
                        "could not create database directory {}: {error}",
                        parent.display()
                    ))
                })?;
            }
        }
        let connection = Connection::open(path)
            .map_err(|error| StorageError::Open(format!("{}: {error}", path.display())))?;
        let storage = Self {
            connection: Mutex::new(connection),
            path: Some(path.to_path_buf()),
            retention,
            runtime: Mutex::new(RuntimeStatus::default()),
        };
        storage.initialize()?;
        Ok(storage)
    }

    pub fn open_in_memory(retention: RetentionPolicy) -> Result<Self, StorageError> {
        retention.validate().map_err(StorageError::InvalidFilter)?;
        let connection =
            Connection::open_in_memory().map_err(|error| StorageError::Open(error.to_string()))?;
        let storage = Self {
            connection: Mutex::new(connection),
            path: None,
            retention,
            runtime: Mutex::new(RuntimeStatus::default()),
        };
        storage.initialize()?;
        Ok(storage)
    }

    fn initialize(&self) -> Result<(), StorageError> {
        let mut connection = self.lock();
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(query_error)?;
        connection
            .execute_batch(
                "PRAGMA journal_mode=WAL;
                 PRAGMA synchronous=NORMAL;
                 PRAGMA temp_store=MEMORY;
                 PRAGMA auto_vacuum=INCREMENTAL;",
            )
            .map_err(query_error)?;
        migrate(&mut connection)?;
        verify_integrity(&connection)?;
        drop(connection);
        self.run_retention().map_err(|error| match error {
            StorageError::Retention(message) => StorageError::Retention(message),
            other => other,
        })?;
        Ok(())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.connection
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Test-only write-failure injection. SQLite's `query_only` makes the
    /// connection reject writes exactly at the transaction boundary used by
    /// ingestion, without touching the real user database.
    #[cfg(test)]
    pub(crate) fn set_query_only_for_test(&self, enabled: bool) -> Result<(), StorageError> {
        let connection = self.lock();
        connection
            .execute_batch(if enabled {
                "PRAGMA query_only=ON;"
            } else {
                "PRAGMA query_only=OFF;"
            })
            .map_err(query_error)
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn retention(&self) -> RetentionPolicy {
        self.retention
    }

    /// Apply one bounded IPC batch. Replaying the same batch is a no-op.
    pub fn apply_batch(&self, batch: &EventBatch) -> Result<IngestStats, StorageError> {
        if batch.session_id.is_empty() || batch.session_id.chars().count() > 128 {
            return Err(StorageError::InvalidFilter(
                "batch session id is empty or too long".to_owned(),
            ));
        }
        let now = now_rfc3339();
        let first_event_time = batch
            .records
            .iter()
            .find_map(|envelope| match &envelope.record {
                sentinel_events::PipelineRecord::ProcessEvent(event) => event_time(event),
                _ => None,
            });
        let mut connection = self.lock();
        let transaction = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(query_error)?;

        ensure_session(
            &transaction,
            &batch.session_id,
            first_event_time.as_deref(),
            now.as_deref(),
        )?;

        let mut stats = IngestStats::default();
        for envelope in &batch.records {
            match &envelope.record {
                sentinel_events::PipelineRecord::ProcessEvent(event) => {
                    if insert_process_event(&transaction, event)? {
                        stats.inserted_events += 1;
                        if event.event_type == ProcessEventType::Stopped
                            && apply_revisions_for_stop(&transaction, &event.event_id)? > 0
                        {
                            stats.revisions_applied += 1;
                        }
                    } else {
                        stats.duplicates += 1;
                    }
                }
                sentinel_events::PipelineRecord::InvalidEvent(event) => {
                    let record_json = serde_json::to_string(event)
                        .map_err(|error| StorageError::Query(error.to_string()))?;
                    let changed = transaction
                        .execute(
                            "INSERT OR IGNORE INTO invalid_events
                             (event_id, session_id, occurred_at, received_at, normalized_at, record_json)
                             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                            params![
                                event.event_id,
                                event.session_id,
                                event.occurred_at,
                                event.received_at,
                                event.normalized_at,
                                record_json
                            ],
                        )
                        .map_err(query_error)?;
                    if changed > 0 {
                        stats.inserted_invalid += 1;
                    } else {
                        stats.duplicates += 1;
                    }
                }
                sentinel_events::PipelineRecord::PairingRevision(revision) => {
                    let record_json = serde_json::to_string(revision)
                        .map_err(|error| StorageError::Query(error.to_string()))?;
                    let changed = transaction
                        .execute(
                            "INSERT OR IGNORE INTO pairing_revisions
                             (stop_event_id, start_event_id, session_id, reason, normalized_at, record_json)
                             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                            params![
                                revision.stop_event_id,
                                revision.start_event_id,
                                revision.session_id,
                                revision.reason,
                                revision.normalized_at,
                                record_json
                            ],
                        )
                        .map_err(query_error)?;
                    if changed > 0 {
                        stats.inserted_revisions += 1;
                        if apply_revision_record(&transaction, revision)? {
                            stats.revisions_applied += 1;
                        }
                    } else {
                        stats.duplicates += 1;
                    }
                }
                sentinel_events::PipelineRecord::SessionEnd(end) => {
                    let record_json = serde_json::to_string(end)
                        .map_err(|error| StorageError::Query(error.to_string()))?;
                    let reason = serde_json::to_value(end.reason)
                        .ok()
                        .and_then(|value| value.as_str().map(ToOwned::to_owned))
                        .unwrap_or_else(|| "unknown".to_owned());
                    let mut inserted_end = transaction
                        .execute(
                            "UPDATE sessions SET closed = 1, end_reason = ?2, ended_at = ?3,
                             end_error = ?4, session_end_json = ?5, last_seen_at = COALESCE(?3, last_seen_at)
                             WHERE session_id = ?1 AND session_end_json IS NULL",
                            params![
                                end.session_id,
                                reason,
                                end.ended_at,
                                end.error,
                                record_json
                            ],
                        )
                        .map_err(query_error)?
                        > 0;
                    if !inserted_end {
                        let exists: bool = transaction
                            .query_row(
                                "SELECT 1 FROM sessions WHERE session_id = ?1",
                                params![end.session_id],
                                |_| Ok(true),
                            )
                            .optional()
                            .map_err(query_error)?
                            .unwrap_or(false);
                        if !exists {
                            transaction
                                .execute(
                                    "INSERT INTO sessions
                                     (session_id, first_seen_at, last_seen_at, closed, end_reason,
                                      ended_at, end_error, session_end_json)
                                     VALUES (?1, ?2, ?3, 1, ?4, ?3, ?5, ?6)",
                                    params![
                                        end.session_id,
                                        end.ended_at,
                                        now,
                                        reason,
                                        end.error,
                                        record_json
                                    ],
                                )
                                .map_err(query_error)?;
                            inserted_end = true;
                        }
                    }
                    if inserted_end {
                        stats.inserted_session_ends += 1;
                    } else {
                        stats.duplicates += 1;
                    }
                }
            }
        }

        let expired = batch.cursor_expired;
        if expired {
            transaction
                .execute(
                    "INSERT OR IGNORE INTO ingestion_gaps
                     (session_id, detected_at, missed_records) VALUES (?1, ?2, ?3)",
                    params![batch.session_id, now, batch.missed_records as i64],
                )
                .map_err(query_error)?;
            stats.gaps = 1;
        }

        update_cursor(&transaction, &batch.session_id, batch.next_cursor)?;
        transaction.commit().map_err(query_error)?;
        drop(connection);

        {
            let mut runtime = self
                .runtime
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            runtime.last_ingest_at = now;
        }
        Ok(stats)
    }

    pub fn mark_session_discontinued(&self, session_id: &str) -> Result<bool, StorageError> {
        let now = now_rfc3339();
        let connection = self.lock();
        let changed = connection
            .execute(
                "UPDATE sessions SET closed = 1, end_reason = 'unobserved',
                 end_error = 'sensor restarted or disconnected before SessionEnd',
                 last_seen_at = COALESCE(?2, last_seen_at)
                 WHERE session_id = ?1 AND closed = 0",
                params![session_id, now],
            )
            .map_err(query_error)?;
        Ok(changed > 0)
    }

    pub fn ingest_cursor(&self) -> Result<Option<(String, u64)>, StorageError> {
        let connection = self.lock();
        connection
            .query_row(
                "SELECT session_id, cursor FROM ingestion_state WHERE id = 1",
                [],
                |row| {
                    let session_id: Option<String> = row.get(0)?;
                    let cursor: i64 = row.get(1)?;
                    Ok(session_id.map(|session_id| (session_id, cursor.max(0) as u64)))
                },
            )
            .map_err(query_error)
    }

    pub fn query_events(&self, filter: &EventFilter) -> Result<EventQueryResult, StorageError> {
        let (limit, offset) = validate_filter(filter)?;
        let mut clauses = vec!["1 = 1".to_owned()];
        let mut values: Vec<Value> = Vec::new();
        if let Some(from) = normalized_filter(&filter.from, "from")? {
            clauses.push("event_time >= ?".to_owned());
            values.push(Value::Text(from));
        }
        if let Some(to) = normalized_filter(&filter.to, "to")? {
            clauses.push("event_time <= ?".to_owned());
            values.push(Value::Text(to));
        }
        if let Some(image) = normalized_filter(&filter.image, "image")? {
            clauses.push("image_name LIKE ? COLLATE NOCASE".to_owned());
            values.push(Value::Text(format!("%{image}%")));
        }
        if let Some(pid) = filter.pid {
            if pid == 0 {
                return Err(StorageError::InvalidFilter(
                    "pid must be greater than zero".to_owned(),
                ));
            }
            clauses.push("pid = ?".to_owned());
            values.push(Value::Integer(i64::from(pid)));
        }
        if let Some(event_type) = normalized_filter(&filter.event_type, "eventType")?.as_deref() {
            if !matches!(event_type, "started" | "stopped") {
                return Err(StorageError::InvalidFilter(
                    "eventType must be started or stopped".to_owned(),
                ));
            }
            clauses.push("event_type = ?".to_owned());
            values.push(Value::Text(event_type.to_owned()));
        }
        if let Some(session_id) = normalized_filter(&filter.session_id, "sessionId")? {
            clauses.push("session_id = ?".to_owned());
            values.push(Value::Text(session_id));
        }
        let where_sql = clauses.join(" AND ");
        let count_sql = format!("SELECT COUNT(*) FROM process_events WHERE {where_sql}");
        let query_sql = format!(
            "SELECT event_id, session_id, sequence, event_type, pid, create_time, image_name,
                    event_time, occurred_at, received_at, normalized_at, pairing_state,
                    pairing_order, paired_event_id, record_json
             FROM process_events WHERE {where_sql}
             ORDER BY event_time IS NULL, event_time DESC, sequence DESC
             LIMIT ? OFFSET ?"
        );

        let connection = self.lock();
        let total: i64 = connection
            .query_row(&count_sql, params_from_iter(values.iter()), |row| {
                row.get(0)
            })
            .map_err(query_error)?;
        let mut statement = connection.prepare(&query_sql).map_err(query_error)?;
        let mut query_values = values;
        query_values.push(Value::Integer(i64::from(limit)));
        query_values.push(Value::Integer(i64::from(offset)));
        let rows = statement
            .query_map(params_from_iter(query_values.iter()), stored_event_from_row)
            .map_err(query_error)?;
        let mut events = Vec::new();
        for row in rows {
            events.push(row.map_err(query_error)??);
        }
        Ok(EventQueryResult {
            total: total.max(0) as u64,
            limit,
            offset,
            events,
        })
    }

    pub fn get_event(&self, event_id: &str) -> Result<Option<EventDetail>, StorageError> {
        if event_id.is_empty() || event_id.len() > 128 {
            return Err(StorageError::InvalidFilter(
                "eventId is empty or too long".to_owned(),
            ));
        }
        let connection = self.lock();
        let event = connection
            .query_row(
                "SELECT event_id, session_id, sequence, event_type, pid, create_time, image_name,
                        event_time, occurred_at, received_at, normalized_at, pairing_state,
                        pairing_order, paired_event_id, record_json
                 FROM process_events WHERE event_id = ?1",
                params![event_id],
                stored_event_from_row,
            )
            .optional()
            .map_err(query_error)?;
        let Some(event) = event.transpose()? else {
            return Ok(None);
        };
        let paired_event = if let Some(paired_id) = &event.paired_event_id {
            connection
                .query_row(
                    "SELECT event_id, session_id, sequence, event_type, pid, create_time, image_name,
                            event_time, occurred_at, received_at, normalized_at, pairing_state,
                            pairing_order, paired_event_id, record_json
                     FROM process_events WHERE event_id = ?1",
                    params![paired_id],
                    stored_event_from_row,
                )
                .optional()
                .map_err(query_error)?
                .transpose()?
        } else {
            None
        };
        let mut revisions = Vec::new();
        let mut statement = connection
            .prepare(
                "SELECT record_json FROM pairing_revisions
                 WHERE stop_event_id = ?1 OR start_event_id = ?1
                 ORDER BY normalized_at DESC",
            )
            .map_err(query_error)?;
        let rows = statement
            .query_map(params![event_id], |row| {
                let json: String = row.get(0)?;
                Ok(json)
            })
            .map_err(query_error)?;
        for row in rows {
            let json = row.map_err(query_error)?;
            revisions.push(
                serde_json::from_str(&json)
                    .map_err(|error| StorageError::Corrupt(error.to_string()))?,
            );
        }
        let session_end_json: Option<String> = connection
            .query_row(
                "SELECT session_end_json FROM sessions WHERE session_id = ?1",
                params![event.session_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(query_error)?
            .flatten();
        let session_end = session_end_json
            .map(|json| {
                serde_json::from_str(&json)
                    .map_err(|error| StorageError::Corrupt(error.to_string()))
            })
            .transpose()?;
        Ok(Some(EventDetail {
            event,
            paired_event,
            revisions,
            session_end,
        }))
    }

    /// Load a bounded session event set for behavior analysis. The caller must
    /// treat `truncated` as a data-quality limitation, never as a complete
    /// session.
    pub fn load_session_for_analysis(
        &self,
        session_id: &str,
        max_events: u32,
    ) -> Result<Option<SessionEventSet>, StorageError> {
        if session_id.is_empty() || session_id.chars().count() > 128 {
            return Err(StorageError::InvalidFilter(
                "sessionId is empty or too long".to_owned(),
            ));
        }
        if !(1..=50_000).contains(&max_events) {
            return Err(StorageError::InvalidFilter(
                "maxEvents must be between 1 and 50000".to_owned(),
            ));
        }
        let connection = self.lock();
        let session: Option<(i64, Option<String>, Option<String>)> = connection
            .query_row(
                "SELECT closed, end_reason, session_end_json FROM sessions WHERE session_id = ?1",
                params![session_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(query_error)?;
        let Some((closed, end_reason, session_end_json)) = session else {
            return Ok(None);
        };
        let mut statement = connection
            .prepare(
                "SELECT record_json FROM process_events
                 WHERE session_id = ?1
                 ORDER BY event_time IS NULL, event_time ASC, sequence ASC
                 LIMIT ?2",
            )
            .map_err(query_error)?;
        let limit = i64::from(max_events) + 1;
        let rows = statement
            .query_map(params![session_id, limit], |row| {
                let json: String = row.get(0)?;
                Ok(json)
            })
            .map_err(query_error)?;
        let mut events = Vec::new();
        let mut truncated = false;
        for row in rows {
            if events.len() >= max_events as usize {
                truncated = true;
                break;
            }
            let json = row.map_err(query_error)?;
            events.push(
                serde_json::from_str(&json)
                    .map_err(|error| StorageError::Corrupt(error.to_string()))?,
            );
        }
        let invalid_records: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM invalid_events WHERE session_id = ?1",
                params![session_id],
                |row| row.get(0),
            )
            .map_err(query_error)?;
        let ingestion_gaps: i64 = connection
            .query_row(
                "SELECT COALESCE(SUM(missed_records), 0) FROM ingestion_gaps WHERE session_id = ?1",
                params![session_id],
                |row| row.get(0),
            )
            .map_err(query_error)?;
        let session_end = session_end_json
            .map(|json| {
                serde_json::from_str(&json)
                    .map_err(|error| StorageError::Corrupt(error.to_string()))
            })
            .transpose()?;
        Ok(Some(SessionEventSet {
            session_id: session_id.to_owned(),
            events,
            truncated,
            invalid_records: invalid_records.max(0) as u64,
            ingestion_gaps: ingestion_gaps.max(0) as u64,
            closed: closed != 0,
            end_reason,
            session_end,
        }))
    }

    pub fn list_sessions(&self, limit: u32) -> Result<Vec<SessionRecord>, StorageError> {
        let limit = limit.clamp(1, 100);
        let connection = self.lock();
        let mut statement = connection
            .prepare(
                "SELECT s.session_id, s.first_seen_at, s.last_seen_at, s.last_cursor, s.closed,
                        s.end_reason, s.ended_at, s.end_error,
                        (SELECT COUNT(*) FROM process_events p WHERE p.session_id = s.session_id),
                        s.session_end_json IS NOT NULL
                 FROM sessions s
                 ORDER BY s.last_seen_at IS NULL, s.last_seen_at DESC
                 LIMIT ?1",
            )
            .map_err(query_error)?;
        let rows = statement
            .query_map(params![limit], |row| {
                let last_cursor: i64 = row.get(3)?;
                let closed: i64 = row.get(4)?;
                let event_count: i64 = row.get(8)?;
                Ok(SessionRecord {
                    session_id: row.get(0)?,
                    first_seen_at: row.get(1)?,
                    last_seen_at: row.get(2)?,
                    last_cursor: last_cursor.max(0) as u64,
                    closed: closed != 0,
                    end_reason: row.get(5)?,
                    ended_at: row.get(6)?,
                    end_error: row.get(7)?,
                    event_count: event_count.max(0) as u64,
                    has_session_end: row.get::<_, i64>(9)? != 0,
                })
            })
            .map_err(query_error)?;
        let mut sessions = Vec::new();
        for row in rows {
            sessions.push(row.map_err(query_error)?);
        }
        Ok(sessions)
    }

    pub fn status(&self) -> Result<StorageStatus, StorageError> {
        let connection = self.lock();
        let schema_version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(query_error)?;
        let event_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM process_events", [], |row| row.get(0))
            .map_err(query_error)?;
        let invalid_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM invalid_events", [], |row| row.get(0))
            .map_err(query_error)?;
        let revision_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM pairing_revisions", [], |row| {
                row.get(0)
            })
            .map_err(query_error)?;
        let gap_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM ingestion_gaps", [], |row| row.get(0))
            .map_err(query_error)?;
        let session_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))
            .map_err(query_error)?;
        let oldest_event_at: Option<String> = connection
            .query_row(
                "SELECT event_time FROM process_events
                 WHERE event_time IS NOT NULL ORDER BY event_time ASC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(query_error)?;
        let newest_event_at: Option<String> = connection
            .query_row(
                "SELECT event_time FROM process_events
                 WHERE event_time IS NOT NULL ORDER BY event_time DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(query_error)?;
        let runtime = self
            .runtime
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Ok(StorageStatus {
            path: self
                .path
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
            schema_version,
            event_count: event_count.max(0) as u64,
            invalid_count: invalid_count.max(0) as u64,
            revision_count: revision_count.max(0) as u64,
            gap_count: gap_count.max(0) as u64,
            session_count: session_count.max(0) as u64,
            oldest_event_at,
            newest_event_at,
            database_bytes: self.database_bytes(),
            retention: self.retention,
            last_ingest_at: runtime.last_ingest_at.clone(),
            last_retention_at: runtime.last_retention_at.clone(),
        })
    }

    pub fn run_retention(&self) -> Result<RetentionStats, StorageError> {
        let cutoff = retention_cutoff(self.retention.max_age_days);
        let now = now_rfc3339();
        let mut connection = self.lock();
        let transaction = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|error| StorageError::Retention(error.to_string()))?;
        let mut stats = RetentionStats::default();

        stats.deleted_events += delete(
            &transaction,
            "DELETE FROM process_events
             WHERE (event_time IS NOT NULL AND event_time < ?1)
                OR (event_time IS NULL AND normalized_at IS NOT NULL AND normalized_at < ?1)",
            params![cutoff],
        )?;
        stats.deleted_invalid += delete(
            &transaction,
            "DELETE FROM invalid_events
             WHERE (occurred_at IS NOT NULL AND occurred_at < ?1)
                OR (occurred_at IS NULL AND normalized_at IS NOT NULL AND normalized_at < ?1)",
            params![cutoff],
        )?;
        stats.deleted_revisions += delete(
            &transaction,
            "DELETE FROM pairing_revisions
             WHERE stop_event_id NOT IN (SELECT event_id FROM process_events)
               AND (normalized_at IS NULL OR normalized_at < ?1)",
            params![cutoff],
        )?;
        delete(
            &transaction,
            "DELETE FROM ingestion_gaps WHERE detected_at < ?1",
            params![cutoff],
        )?;

        let event_count: i64 = transaction
            .query_row("SELECT COUNT(*) FROM process_events", [], |row| row.get(0))
            .map_err(|error| StorageError::Retention(error.to_string()))?;
        if event_count > self.retention.max_events as i64 {
            let excess = event_count - self.retention.max_events as i64;
            stats.deleted_events += delete(
                &transaction,
                "DELETE FROM process_events WHERE event_id IN
                 (SELECT event_id FROM process_events
                  ORDER BY event_time IS NULL, event_time ASC LIMIT ?1)",
                params![excess],
            )?;
        }

        let session_count: i64 = transaction
            .query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))
            .map_err(|error| StorageError::Retention(error.to_string()))?;
        if session_count > MAX_SESSION_ROWS as i64 {
            let excess = session_count - MAX_SESSION_ROWS as i64;
            stats.deleted_sessions += delete(
                &transaction,
                "DELETE FROM sessions WHERE session_id IN
                 (SELECT s.session_id FROM sessions s
                  WHERE s.closed = 1
                    AND NOT EXISTS (SELECT 1 FROM process_events p WHERE p.session_id = s.session_id)
                  ORDER BY s.last_seen_at IS NULL, s.last_seen_at ASC LIMIT ?1)",
                params![excess],
            )?;
        }

        transaction
            .commit()
            .map_err(|error| StorageError::Retention(error.to_string()))?;
        connection
            .execute_batch("PRAGMA incremental_vacuum(256);")
            .map_err(|error| StorageError::Retention(error.to_string()))?;

        // The row and time bounds are primary. If the file still exceeds the
        // byte budget (for example many long image paths), delete the oldest
        // events in bounded chunks. SQLite may not return every freed page to
        // the filesystem immediately; the status reports the actual size.
        let mut chunks = 0;
        while self.database_bytes() > self.retention.max_bytes && chunks < 10 {
            let transaction = connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(|error| StorageError::Retention(error.to_string()))?;
            let deleted = delete(
                &transaction,
                "DELETE FROM process_events WHERE event_id IN
                 (SELECT event_id FROM process_events
                  ORDER BY event_time IS NULL, event_time ASC LIMIT 10000)",
                [],
            )?;
            delete(
                &transaction,
                "DELETE FROM pairing_revisions
                 WHERE stop_event_id NOT IN (SELECT event_id FROM process_events)",
                params![],
            )?;
            transaction
                .commit()
                .map_err(|error| StorageError::Retention(error.to_string()))?;
            stats.deleted_events += deleted;
            if deleted == 0 {
                break;
            }
            connection
                .execute_batch("PRAGMA incremental_vacuum(1000);")
                .map_err(|error| StorageError::Retention(error.to_string()))?;
            chunks += 1;
        }
        drop(connection);

        {
            let mut runtime = self
                .runtime
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            runtime.last_retention_at = now;
        }
        stats.database_bytes = self.database_bytes();
        Ok(stats)
    }

    fn database_bytes(&self) -> u64 {
        let Some(path) = self.path.as_ref() else {
            return 0;
        };
        let mut total = std::fs::metadata(path).map_or(0, |metadata| metadata.len());
        // WAL pages contain committed events until checkpoint. Reporting only
        // the main file would understate the actual local storage.
        let mut wal_name = path.as_os_str().to_owned();
        wal_name.push("-wal");
        total = total.saturating_add(
            std::fs::metadata(PathBuf::from(wal_name)).map_or(0, |metadata| metadata.len()),
        );
        total
    }
}

fn migrate(connection: &mut Connection) -> Result<(), StorageError> {
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(query_error)?;
    match version {
        0 => {
            let transaction = connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(query_error)?;
            transaction.execute_batch(SCHEMA_SQL).map_err(query_error)?;
            transaction
                .pragma_update(None, "user_version", SCHEMA_VERSION)
                .map_err(query_error)?;
            transaction.commit().map_err(query_error)?;
            Ok(())
        }
        1 => {
            // Schema 1 stored the stop side of an in-order pair as its own
            // event ID. Repair the link so detail queries return the other
            // lifecycle event. The JSON record is the source of truth.
            let transaction = connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(query_error)?;
            transaction
                .execute_batch(
                    "UPDATE process_events
                     SET paired_event_id = CASE
                         WHEN event_type = 'started'
                         THEN json_extract(record_json, '$.pairing.stop_event_id')
                         ELSE json_extract(record_json, '$.pairing.start_event_id')
                     END
                     WHERE pairing_state = 'paired';",
                )
                .map_err(query_error)?;
            transaction
                .pragma_update(None, "user_version", SCHEMA_VERSION)
                .map_err(query_error)?;
            transaction.commit().map_err(query_error)?;
            Ok(())
        }
        2 => Ok(()),
        found => Err(StorageError::UnsupportedSchema {
            found,
            supported: SCHEMA_VERSION,
        }),
    }
}

fn verify_integrity(connection: &Connection) -> Result<(), StorageError> {
    let result: String = connection
        .query_row("PRAGMA quick_check(1)", [], |row| row.get(0))
        .map_err(|error| StorageError::Corrupt(error.to_string()))?;
    if result.eq_ignore_ascii_case("ok") {
        Ok(())
    } else {
        Err(StorageError::Corrupt(result))
    }
}

const SCHEMA_SQL: &str = "
CREATE TABLE IF NOT EXISTS sessions (
    session_id TEXT PRIMARY KEY,
    first_seen_at TEXT,
    last_seen_at TEXT,
    last_cursor INTEGER NOT NULL DEFAULT 0,
    closed INTEGER NOT NULL DEFAULT 0,
    end_reason TEXT,
    ended_at TEXT,
    end_error TEXT,
    session_end_json TEXT
);

CREATE TABLE IF NOT EXISTS process_events (
    event_id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    sequence INTEGER NOT NULL,
    event_type TEXT NOT NULL,
    pid INTEGER NOT NULL,
    create_time INTEGER NOT NULL,
    image_name TEXT,
    event_time TEXT,
    occurred_at TEXT,
    received_at TEXT,
    normalized_at TEXT,
    pairing_state TEXT NOT NULL,
    pairing_order TEXT,
    paired_event_id TEXT,
    record_json TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_process_events_session ON process_events(session_id);
CREATE INDEX IF NOT EXISTS idx_process_events_time ON process_events(event_time);
CREATE INDEX IF NOT EXISTS idx_process_events_pid ON process_events(pid);
CREATE INDEX IF NOT EXISTS idx_process_events_type ON process_events(event_type);
CREATE INDEX IF NOT EXISTS idx_process_events_image ON process_events(image_name COLLATE NOCASE);

CREATE TABLE IF NOT EXISTS invalid_events (
    event_id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    occurred_at TEXT,
    received_at TEXT,
    normalized_at TEXT,
    record_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_invalid_events_session ON invalid_events(session_id);

CREATE TABLE IF NOT EXISTS pairing_revisions (
    stop_event_id TEXT PRIMARY KEY,
    start_event_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    reason TEXT NOT NULL,
    normalized_at TEXT,
    record_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_pairing_revisions_session ON pairing_revisions(session_id);

CREATE TABLE IF NOT EXISTS ingestion_gaps (
    session_id TEXT NOT NULL,
    detected_at TEXT NOT NULL,
    missed_records INTEGER NOT NULL,
    PRIMARY KEY (session_id, detected_at, missed_records)
);

CREATE TABLE IF NOT EXISTS ingestion_state (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    session_id TEXT,
    cursor INTEGER NOT NULL DEFAULT 0
);
INSERT OR IGNORE INTO ingestion_state (id, session_id, cursor) VALUES (1, NULL, 0);
";

fn ensure_session(
    transaction: &rusqlite::Transaction<'_>,
    session_id: &str,
    first_seen_at: Option<&str>,
    now: Option<&str>,
) -> Result<(), StorageError> {
    transaction
        .execute(
            "INSERT INTO sessions (session_id, first_seen_at, last_seen_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(session_id) DO UPDATE SET
                last_seen_at = COALESCE(excluded.last_seen_at, sessions.last_seen_at)",
            params![session_id, first_seen_at, now],
        )
        .map_err(query_error)?;
    Ok(())
}

fn update_cursor(
    transaction: &rusqlite::Transaction<'_>,
    session_id: &str,
    cursor: u64,
) -> Result<(), StorageError> {
    transaction
        .execute(
            "UPDATE sessions SET last_cursor = MAX(last_cursor, ?2) WHERE session_id = ?1",
            params![session_id, cursor as i64],
        )
        .map_err(query_error)?;
    transaction
        .execute(
            "INSERT INTO ingestion_state (id, session_id, cursor) VALUES (1, ?1, ?2)
             ON CONFLICT(id) DO UPDATE SET
                session_id = excluded.session_id,
                cursor = CASE
                    WHEN ingestion_state.session_id = excluded.session_id
                     AND ingestion_state.cursor > excluded.cursor
                    THEN ingestion_state.cursor
                    ELSE excluded.cursor
                END",
            params![session_id, cursor as i64],
        )
        .map_err(query_error)?;
    Ok(())
}

fn insert_process_event(
    transaction: &rusqlite::Transaction<'_>,
    event: &ProcessEvent,
) -> Result<bool, StorageError> {
    let record_json =
        serde_json::to_string(event).map_err(|error| StorageError::Query(error.to_string()))?;
    let (pairing_state, pairing_order, paired_event_id) =
        pairing_columns(event.event_type, &event.pairing);
    let event_type = match event.event_type {
        ProcessEventType::Started => "started",
        ProcessEventType::Stopped => "stopped",
    };
    let changed = transaction
        .execute(
            "INSERT OR IGNORE INTO process_events
             (event_id, session_id, sequence, event_type, pid, create_time, image_name,
              event_time, occurred_at, received_at, normalized_at, pairing_state,
              pairing_order, paired_event_id, record_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                event.event_id,
                event.session_id,
                event.sequence as i64,
                event_type,
                i64::from(event.identity.pid),
                event.identity.create_time as i64,
                event.image_name,
                event_time(event),
                event.etw.occurred_at,
                event.received_at,
                event.normalized_at,
                pairing_state,
                pairing_order,
                paired_event_id,
                record_json
            ],
        )
        .map_err(query_error)?;
    Ok(changed > 0)
}

fn pairing_columns(
    event_type: ProcessEventType,
    pairing: &PairingStatus,
) -> (&'static str, Option<String>, Option<String>) {
    match pairing {
        PairingStatus::Paired {
            start_event_id,
            stop_event_id,
            order,
            ..
        } => {
            // Store the *other* side of the lifecycle on each row. The
            // lifecycle order is already captured separately.
            let paired = match event_type {
                ProcessEventType::Started => stop_event_id,
                ProcessEventType::Stopped => start_event_id,
            };
            let order = match order {
                PairingOrder::InOrder => "in_order",
                PairingOrder::OutOfOrder => "out_of_order",
            };
            ("paired", Some(order.to_owned()), Some(paired.clone()))
        }
        PairingStatus::Waiting => ("waiting", None, None),
        PairingStatus::Unpaired { .. } => ("unpaired", None, None),
        PairingStatus::Indeterminate { .. } => ("indeterminate", None, None),
    }
}

fn apply_revisions_for_stop(
    transaction: &rusqlite::Transaction<'_>,
    stop_event_id: &str,
) -> Result<u64, StorageError> {
    let revision_json: Option<String> = transaction
        .query_row(
            "SELECT record_json FROM pairing_revisions WHERE stop_event_id = ?1",
            params![stop_event_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(query_error)?;
    let Some(json) = revision_json else {
        return Ok(0);
    };
    let revision: PairingRevision =
        serde_json::from_str(&json).map_err(|error| StorageError::Corrupt(error.to_string()))?;
    Ok(u64::from(apply_revision_record(transaction, &revision)?))
}

fn apply_revision_record(
    transaction: &rusqlite::Transaction<'_>,
    revision: &PairingRevision,
) -> Result<bool, StorageError> {
    let record_json: Option<String> = transaction
        .query_row(
            "SELECT record_json FROM process_events WHERE event_id = ?1",
            params![revision.stop_event_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(query_error)?;
    let Some(json) = record_json else {
        return Ok(false);
    };
    let mut event: ProcessEvent =
        serde_json::from_str(&json).map_err(|error| StorageError::Corrupt(error.to_string()))?;
    event.pairing = PairingStatus::Paired {
        start_event_id: revision.start_event_id.clone(),
        stop_event_id: revision.stop_event_id.clone(),
        order: PairingOrder::OutOfOrder,
        sequence_match: revision.sequence_match,
    };
    let updated_json =
        serde_json::to_string(&event).map_err(|error| StorageError::Query(error.to_string()))?;
    transaction
        .execute(
            "UPDATE process_events SET pairing_state = 'paired', pairing_order = 'out_of_order',
             paired_event_id = ?2, record_json = ?3 WHERE event_id = ?1",
            params![
                revision.stop_event_id,
                revision.start_event_id,
                updated_json
            ],
        )
        .map_err(query_error)?;
    Ok(true)
}

fn event_time(event: &ProcessEvent) -> Option<String> {
    event
        .etw
        .occurred_at
        .clone()
        .or_else(|| event.received_at.clone())
        .or_else(|| event.normalized_at.clone())
}

fn stored_event_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<Result<StoredEvent, StorageError>> {
    let record_json: String = row.get(14)?;
    let record: ProcessEvent = match serde_json::from_str(&record_json) {
        Ok(record) => record,
        Err(error) => return Ok(Err(StorageError::Corrupt(error.to_string()))),
    };
    Ok(Ok(StoredEvent {
        event_id: row.get(0)?,
        session_id: row.get(1)?,
        sequence: row.get::<_, i64>(2)?.max(0) as u64,
        event_type: row.get(3)?,
        pid: row.get(4)?,
        create_time: row.get::<_, i64>(5)?.max(0) as u64,
        image_name: row.get(6)?,
        event_time: row.get(7)?,
        occurred_at: row.get(8)?,
        received_at: row.get(9)?,
        normalized_at: row.get(10)?,
        pairing_state: row.get(11)?,
        pairing_order: row.get(12)?,
        paired_event_id: row.get(13)?,
        record,
    }))
}

fn validate_filter(filter: &EventFilter) -> Result<(u32, u32), StorageError> {
    let limit = filter.limit.unwrap_or(DEFAULT_QUERY_LIMIT);
    if !(1..=MAX_QUERY_LIMIT).contains(&limit) {
        return Err(StorageError::InvalidFilter(format!(
            "limit must be between 1 and {MAX_QUERY_LIMIT}"
        )));
    }
    let offset = filter.offset.unwrap_or(0);
    if offset > MAX_QUERY_OFFSET {
        return Err(StorageError::InvalidFilter(format!(
            "offset must be at most {MAX_QUERY_OFFSET}"
        )));
    }
    Ok((limit, offset))
}

fn normalized_filter(value: &Option<String>, field: &str) -> Result<Option<String>, StorageError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().count() > MAX_FILTER_CHARS {
        return Err(StorageError::InvalidFilter(format!(
            "{field} is longer than {MAX_FILTER_CHARS} characters"
        )));
    }
    if trimmed.contains('\0') {
        return Err(StorageError::InvalidFilter(format!(
            "{field} contains a NUL character"
        )));
    }
    Ok(Some(trimmed.to_owned()))
}

fn retention_cutoff(max_age_days: u32) -> String {
    let cutoff = Utc::now() - ChronoDuration::days(i64::from(max_age_days));
    cutoff.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn now_rfc3339() -> Option<String> {
    now_unix_ms().and_then(unix_ms_to_rfc3339)
}

fn delete(
    transaction: &rusqlite::Transaction<'_>,
    sql: &str,
    params: impl rusqlite::Params,
) -> Result<u64, StorageError> {
    transaction
        .execute(sql, params)
        .map(|count| count as u64)
        .map_err(|error| StorageError::Retention(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentinel_events::{
        DataCompleteness, EtwEventInfo, EventSource, FieldState, IdentityCertainty,
        PairingSnapshot, PipelineRecord, ProcessIdentity,
    };
    use sentinel_ipc::events::EventEnvelope;

    fn retention() -> RetentionPolicy {
        RetentionPolicy {
            max_age_days: 30,
            max_events: 1_000,
            max_bytes: 16 * 1024 * 1024,
        }
    }

    fn event(
        sequence: u64,
        event_type: ProcessEventType,
        pid: u32,
        pairing: PairingStatus,
    ) -> ProcessEvent {
        ProcessEvent {
            event_id: format!("session:{sequence:016x}"),
            sequence,
            session_id: "session".to_owned(),
            event_type,
            source: EventSource::MicrosoftWindowsKernelProcess,
            identity: ProcessIdentity {
                pid,
                create_time: 133_000_000_000_000_000,
            },
            identity_certainty: IdentityCertainty::Exact,
            parent_pid: Some(4),
            image_name: Some(format!(r"C:\Windows\System32\test-{pid}.exe")),
            etw: EtwEventInfo {
                event_id: if event_type == ProcessEventType::Started {
                    1
                } else {
                    2
                },
                version: 4,
                provider_sequence: Some(sequence),
                timestamp_100ns: Some(133_000_000_000_000_000),
                occurred_at: Some("2026-10-31T00:00:00.000Z".to_owned()),
            },
            process_started_at: Some("2026-10-31T00:00:00.000Z".to_owned()),
            received_at: Some("2026-10-31T00:00:00.001Z".to_owned()),
            normalized_at: Some("2026-10-31T00:00:00.002Z".to_owned()),
            completeness: DataCompleteness {
                identity_certain: true,
                event_time_known: true,
                received_time_known: true,
                normalized_time_known: true,
                parent_pid_available: true,
                image_name: FieldState::Present,
                etw_sequence_available: true,
                issues: Vec::new(),
            },
            pairing,
        }
    }

    fn batch(records: Vec<PipelineRecord>) -> EventBatch {
        let count = records.len() as u64;
        let records = records
            .into_iter()
            .enumerate()
            .map(|(index, record)| EventEnvelope {
                cursor: index as u64 + 1,
                record,
            })
            .collect();
        EventBatch {
            session_id: "session".to_owned(),
            oldest_cursor: 1,
            latest_cursor: count,
            next_cursor: count,
            cursor_expired: false,
            missed_records: 0,
            cursor_ahead: false,
            evictions: 0,
            session_closed: false,
            has_more: false,
            records,
        }
    }

    fn session_end() -> SessionEnd {
        SessionEnd {
            session_id: "session".to_owned(),
            ended_at: Some("2026-10-31T00:00:01.000Z".to_owned()),
            reason: sentinel_events::SessionEndReason::Completed,
            error: None,
            open_start_events: Vec::new(),
            final_stats: None,
            pairing: PairingSnapshot {
                paired: 1,
                unpaired_starts: 0,
                orphan_stops: 0,
                duplicate_starts: 0,
                duplicate_stops: 0,
                pid_reuse_detected: 0,
                sequence_mismatches: 0,
                start_overflow: 0,
                orphan_stop_overflow: 0,
                identity_mismatched_stops: 0,
                out_of_order_pairs: 0,
                indeterminate_identities: 0,
                closed_identity_evictions: 0,
                pending_orphan_stops: 0,
                starts_seen: 1,
                stops_seen: 1,
                observed_start_image_names: vec!["test.exe".to_owned()],
                observed_start_image_names_truncated: false,
            },
        }
    }

    fn revision() -> PairingRevision {
        PairingRevision {
            session_id: "session".to_owned(),
            stop_event_id: "session:0000000000000002".to_owned(),
            start_event_id: "session:0000000000000001".to_owned(),
            sequence_match: Some(true),
            reason: "start arrived after an orphan stop".to_owned(),
            normalized_at: Some("2026-10-31T00:00:00.003Z".to_owned()),
        }
    }

    fn complete_batch() -> EventBatch {
        let start = event(1, ProcessEventType::Started, 100, PairingStatus::Waiting);
        let stop = event(
            2,
            ProcessEventType::Stopped,
            100,
            PairingStatus::Unpaired {
                reason: sentinel_events::UnpairedReason::StopWithoutObservedStart,
            },
        );
        batch(vec![
            PipelineRecord::ProcessEvent(start),
            PipelineRecord::ProcessEvent(stop),
            PipelineRecord::PairingRevision(revision()),
            PipelineRecord::SessionEnd(session_end()),
        ])
    }

    #[test]
    fn duplicate_batches_are_idempotent() {
        let storage = Storage::open_in_memory(retention()).unwrap();
        let first = storage.apply_batch(&complete_batch()).unwrap();
        assert_eq!(first.inserted_events, 2);
        assert_eq!(first.inserted_revisions, 1);
        assert_eq!(first.revisions_applied, 1);
        assert_eq!(first.inserted_session_ends, 1);

        let before = storage.status().unwrap();
        let second = storage.apply_batch(&complete_batch()).unwrap();
        assert_eq!(second.inserted_events, 0);
        assert_eq!(second.inserted_revisions, 0);
        assert!(second.duplicates >= 3);
        let after = storage.status().unwrap();
        assert_eq!(before.event_count, after.event_count);
        assert_eq!(before.revision_count, after.revision_count);
        assert_eq!(before.session_count, after.session_count);
    }

    #[test]
    fn pairing_revision_updates_the_stop_record() {
        let storage = Storage::open_in_memory(retention()).unwrap();
        storage.apply_batch(&complete_batch()).unwrap();
        let detail = storage
            .get_event("session:0000000000000002")
            .unwrap()
            .expect("stop event");
        assert_eq!(detail.event.pairing_state, "paired");
        assert_eq!(detail.event.pairing_order.as_deref(), Some("out_of_order"));
        assert_eq!(
            detail.event.paired_event_id.as_deref(),
            Some("session:0000000000000001")
        );
        assert!(matches!(
            detail.event.record.pairing,
            PairingStatus::Paired {
                order: PairingOrder::OutOfOrder,
                ..
            }
        ));
        assert!(detail.paired_event.is_some());
        assert_eq!(detail.revisions.len(), 1);
        assert!(detail.session_end.is_some());
    }

    #[test]
    fn stored_pair_links_each_side_to_the_other_event() {
        let storage = Storage::open_in_memory(retention()).unwrap();
        let start_id = "session:0000000000000001".to_owned();
        let stop_id = "session:0000000000000002".to_owned();
        let paired = PairingStatus::Paired {
            start_event_id: start_id.clone(),
            stop_event_id: stop_id.clone(),
            order: PairingOrder::InOrder,
            sequence_match: Some(true),
        };
        let start = event(1, ProcessEventType::Started, 100, paired.clone());
        let stop = event(2, ProcessEventType::Stopped, 100, paired);
        storage
            .apply_batch(&batch(vec![
                PipelineRecord::ProcessEvent(start),
                PipelineRecord::ProcessEvent(stop),
            ]))
            .unwrap();

        let stop_detail = storage.get_event(&stop_id).unwrap().unwrap();
        assert_eq!(
            stop_detail
                .paired_event
                .as_ref()
                .map(|event| &event.event_id),
            Some(&start_id)
        );
        let start_detail = storage.get_event(&start_id).unwrap().unwrap();
        assert_eq!(
            start_detail
                .paired_event
                .as_ref()
                .map(|event| &event.event_id),
            Some(&stop_id)
        );
    }

    #[test]
    fn revision_before_stop_is_applied_when_stop_arrives() {
        let storage = Storage::open_in_memory(retention()).unwrap();
        let stop = event(
            2,
            ProcessEventType::Stopped,
            100,
            PairingStatus::Unpaired {
                reason: sentinel_events::UnpairedReason::StopWithoutObservedStart,
            },
        );
        storage
            .apply_batch(&batch(vec![PipelineRecord::PairingRevision(revision())]))
            .unwrap();
        storage
            .apply_batch(&batch(vec![PipelineRecord::ProcessEvent(stop)]))
            .unwrap();
        let detail = storage
            .get_event("session:0000000000000002")
            .unwrap()
            .unwrap();
        assert_eq!(detail.event.pairing_state, "paired");
    }

    #[test]
    fn queries_filter_and_bound_results() {
        let storage = Storage::open_in_memory(retention()).unwrap();
        storage.apply_batch(&complete_batch()).unwrap();
        let all = storage.query_events(&EventFilter::default()).unwrap();
        assert_eq!(all.total, 2);
        assert_eq!(all.events.len(), 2);

        let filter = EventFilter {
            image: Some("test-100".to_owned()),
            pid: Some(100),
            event_type: Some("stopped".to_owned()),
            ..EventFilter::default()
        };
        let filtered = storage.query_events(&filter).unwrap();
        assert_eq!(filtered.total, 1);
        assert_eq!(filtered.events[0].event_type, "stopped");

        let invalid = storage
            .query_events(&EventFilter {
                limit: Some(MAX_QUERY_LIMIT + 1),
                ..EventFilter::default()
            })
            .unwrap_err();
        assert!(matches!(invalid, StorageError::InvalidFilter(_)));
    }

    #[test]
    fn cursor_and_discontinued_session_are_explicit() {
        let storage = Storage::open_in_memory(retention()).unwrap();
        storage.apply_batch(&complete_batch()).unwrap();
        assert_eq!(
            storage.ingest_cursor().unwrap(),
            Some(("session".to_owned(), 4))
        );
        assert!(!storage.mark_session_discontinued("session").unwrap());
        let sessions = storage.list_sessions(10).unwrap();
        assert_eq!(sessions.len(), 1);
        assert!(sessions[0].closed);
        assert_eq!(sessions[0].end_reason.as_deref(), Some("completed"));
        assert!(sessions[0].has_session_end);
    }

    #[test]
    fn discontinued_session_is_not_reported_as_normal() {
        let storage = Storage::open_in_memory(retention()).unwrap();
        let start = event(1, ProcessEventType::Started, 100, PairingStatus::Waiting);
        let mut open = batch(vec![PipelineRecord::ProcessEvent(start)]);
        open.session_closed = false;
        storage.apply_batch(&open).unwrap();
        assert!(storage.mark_session_discontinued("session").unwrap());
        let sessions = storage.list_sessions(10).unwrap();
        assert_eq!(sessions[0].end_reason.as_deref(), Some("unobserved"));
        assert!(!sessions[0].has_session_end);
    }

    #[test]
    fn analysis_load_is_bounded_and_reports_truncation() {
        let storage = Storage::open_in_memory(retention()).unwrap();
        storage.apply_batch(&complete_batch()).unwrap();
        let full = storage
            .load_session_for_analysis("session", 10)
            .unwrap()
            .expect("session");
        assert_eq!(full.events.len(), 2);
        assert!(!full.truncated);
        assert!(full.closed);
        assert!(full.session_end.is_some());
        assert_eq!(full.end_reason.as_deref(), Some("completed"));

        let truncated = storage
            .load_session_for_analysis("session", 1)
            .unwrap()
            .expect("session");
        assert_eq!(truncated.events.len(), 1);
        assert!(truncated.truncated);
    }

    #[test]
    fn retention_removes_old_events() {
        let storage = Storage::open_in_memory(retention()).unwrap();
        let mut old = event(1, ProcessEventType::Started, 100, PairingStatus::Waiting);
        old.etw.occurred_at = Some("2000-01-01T00:00:00.000Z".to_owned());
        old.received_at = Some("2000-01-01T00:00:00.000Z".to_owned());
        old.normalized_at = Some("2000-01-01T00:00:00.000Z".to_owned());
        storage
            .apply_batch(&batch(vec![PipelineRecord::ProcessEvent(old)]))
            .unwrap();
        let stats = storage.run_retention().unwrap();
        assert_eq!(stats.deleted_events, 1);
        assert_eq!(storage.status().unwrap().event_count, 0);
    }

    #[test]
    fn schema_version_is_recorded() {
        let storage = Storage::open_in_memory(retention()).unwrap();
        assert_eq!(storage.status().unwrap().schema_version, SCHEMA_VERSION);
    }

    #[test]
    fn write_failure_does_not_advance_the_cursor_and_recovery_replays() {
        let storage = Storage::open_in_memory(retention()).unwrap();
        let batch = complete_batch();
        storage.set_query_only_for_test(true).unwrap();
        let error = storage.apply_batch(&batch).unwrap_err();
        assert!(matches!(error, StorageError::Query(_)), "{error}");
        assert_eq!(storage.ingest_cursor().unwrap(), None);
        assert_eq!(storage.status().unwrap().event_count, 0);

        storage.set_query_only_for_test(false).unwrap();
        let stats = storage.apply_batch(&batch).unwrap();
        assert_eq!(stats.inserted_events, 2);
        assert_eq!(
            storage.ingest_cursor().unwrap(),
            Some(("session".to_owned(), 4))
        );
        assert_eq!(storage.status().unwrap().event_count, 2);
    }

    #[test]
    fn reopening_a_file_database_preserves_cursor_and_final_stats() {
        let path = std::env::temp_dir().join(format!(
            "sentinelite-storage-faults-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        {
            let storage = Storage::open(&path, retention()).unwrap();
            let mut batch = complete_batch();
            let end = batch
                .records
                .iter_mut()
                .find_map(|envelope| match &mut envelope.record {
                    PipelineRecord::SessionEnd(end) => Some(end),
                    _ => None,
                })
                .expect("complete batch contains SessionEnd");
            end.final_stats = Some(sentinel_events::SessionEndStats {
                events_lost: Some(0),
                realtime_buffers_lost: Some(0),
                pipeline_received: 2,
                pipeline_consumed: 2,
                pipeline_normalized: 2,
                ..sentinel_events::SessionEndStats::default()
            });
            storage.apply_batch(&batch).unwrap();
            assert_eq!(
                storage.ingest_cursor().unwrap(),
                Some(("session".to_owned(), 4))
            );
        }
        {
            let storage = Storage::open(&path, retention()).unwrap();
            assert_eq!(
                storage.ingest_cursor().unwrap(),
                Some(("session".to_owned(), 4))
            );
            let set = storage
                .load_session_for_analysis("session", 10)
                .unwrap()
                .expect("session must survive reopen");
            let stats = set
                .session_end
                .expect("SessionEnd must survive reopen")
                .final_stats
                .expect("final stats must survive reopen");
            assert_eq!(stats.events_lost, Some(0));
            assert_eq!(stats.pipeline_normalized, 2);
        }
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }

    #[test]
    // The test restores the Windows read-only attribute on a private temp file.
    #[allow(clippy::permissions_set_readonly_false)]
    fn a_read_only_database_is_fail_closed_for_writes() {
        let path = std::env::temp_dir().join(format!(
            "sentinelite-storage-readonly-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        {
            let storage = Storage::open(&path, retention()).unwrap();
            storage.apply_batch(&complete_batch()).unwrap();
        }
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&path, permissions).unwrap();

        let result = Storage::open(&path, retention());
        match result {
            Err(_) => {}
            Ok(storage) => {
                let error = storage.apply_batch(&complete_batch()).unwrap_err();
                assert!(matches!(error, StorageError::Query(_)), "{error}");
                assert_eq!(
                    storage.ingest_cursor().unwrap(),
                    Some(("session".to_owned(), 4))
                );
            }
        }

        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(false);
        std::fs::set_permissions(&path, permissions).unwrap();
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }

    #[test]
    fn a_non_database_file_is_rejected_without_replacement() {
        let path = std::env::temp_dir().join(format!(
            "sentinelite-storage-corrupt-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let original = b"this is not a sqlite database and must not be replaced";
        std::fs::write(&path, original).unwrap();
        let error = Storage::open(&path, retention()).unwrap_err();
        let message = error.to_string();
        assert!(
            message.contains("not a database") || message.contains("corrupt"),
            "unexpected error: {message}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), original);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }

    #[test]
    fn reopening_a_current_schema_database_succeeds() {
        let path = std::env::temp_dir().join(format!(
            "sentinelite-storage-reopen-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        {
            let storage = Storage::open(&path, retention()).unwrap();
            assert_eq!(storage.status().unwrap().schema_version, SCHEMA_VERSION);
        }
        {
            let storage = Storage::open(&path, retention()).unwrap();
            assert_eq!(storage.status().unwrap().schema_version, SCHEMA_VERSION);
        }
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }
}
