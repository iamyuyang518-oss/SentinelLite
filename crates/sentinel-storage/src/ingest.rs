//! Application-side ingestion worker.
//!
//! The worker pulls bounded batches from the authenticated sensor event pipe
//! and writes them to SQLite. It never runs inside the sensor: a slow disk, a
//! locked database or a UI query cannot delay ETW collection. Cursor state is
//! persisted so an application restart resumes where it stopped; a sensor
//! restart is detected by a new session ID and the old session is marked as
//! having ended without an observed `SessionEnd` instead of being invented as
//! normal completion.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use sentinel_ipc::client::{ClientOptions, query_events};
use sentinel_ipc::events::EventBatch;
use serde::Serialize;

use crate::db::Storage;

pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(500);
pub const DEFAULT_EMPTY_INTERVAL: Duration = Duration::from_millis(750);
pub const DEFAULT_ERROR_BACKOFF: Duration = Duration::from_secs(1);
// The sensor is a local named pipe. A long backoff while the sensor is absent
// can miss a short-lived acquisition session entirely, so the retry ceiling
// stays small even after repeated failures.
pub const MAX_ERROR_BACKOFF: Duration = Duration::from_secs(2);
pub const DEFAULT_BATCH_LIMIT: u16 = 128;
pub const DEFAULT_RETENTION_EVERY_BATCHES: u64 = 100;

#[derive(Debug, Clone)]
pub struct IngestionConfig {
    pub poll_interval: Duration,
    pub empty_interval: Duration,
    pub error_backoff: Duration,
    pub batch_limit: u16,
    pub retention_every_batches: u64,
}

impl Default for IngestionConfig {
    fn default() -> Self {
        Self {
            poll_interval: DEFAULT_POLL_INTERVAL,
            empty_interval: DEFAULT_EMPTY_INTERVAL,
            error_backoff: DEFAULT_ERROR_BACKOFF,
            batch_limit: DEFAULT_BATCH_LIMIT,
            retention_every_batches: DEFAULT_RETENTION_EVERY_BATCHES,
        }
    }
}

impl IngestionConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.poll_interval.is_zero() || self.empty_interval.is_zero() {
            return Err("ingestion intervals must be greater than zero".to_owned());
        }
        if self.error_backoff.is_zero() {
            return Err("ingestion error backoff must be greater than zero".to_owned());
        }
        if !(1..=sentinel_ipc::events::MAX_EVENT_BATCH_LIMIT).contains(&self.batch_limit) {
            return Err(format!(
                "ingestion batch limit must be between 1 and {}",
                sentinel_ipc::events::MAX_EVENT_BATCH_LIMIT
            ));
        }
        if self.retention_every_batches == 0 {
            return Err("retention_every_batches must be greater than zero".to_owned());
        }
        Ok(())
    }
}

pub trait EventSource: Send + Sync + 'static {
    fn poll(&self, after: u64, limit: u16) -> Result<EventBatch, String>;
}

pub struct IpcEventSource {
    pipe_name: String,
    options: ClientOptions,
}

impl IpcEventSource {
    pub fn new(pipe_name: impl Into<String>, options: ClientOptions) -> Self {
        Self {
            pipe_name: pipe_name.into(),
            options,
        }
    }

    pub fn production() -> Self {
        Self::new(
            sentinel_ipc::DEFAULT_EVENT_PIPE_NAME,
            ClientOptions::secure(),
        )
    }
}

impl EventSource for IpcEventSource {
    fn poll(&self, after: u64, limit: u16) -> Result<EventBatch, String> {
        query_events(&self.pipe_name, &self.options, after, limit)
            .map_err(|error| error.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IngestionState {
    Running,
    Degraded,
    Stopped,
}

#[derive(Debug, Clone, Serialize)]
pub struct IngestionStatus {
    pub state: IngestionState,
    pub source_available: bool,
    pub last_error: Option<String>,
    pub last_success_at: Option<String>,
    pub poll_count: u64,
    pub batch_count: u64,
    pub record_count: u64,
    pub sessions_observed: u64,
    pub session_id: Option<String>,
    pub session_closed: bool,
    pub last_cursor: u64,
    pub last_gap_records: u64,
}

impl Default for IngestionStatus {
    fn default() -> Self {
        Self {
            state: IngestionState::Stopped,
            source_available: false,
            last_error: None,
            last_success_at: None,
            poll_count: 0,
            batch_count: 0,
            record_count: 0,
            sessions_observed: 0,
            session_id: None,
            session_closed: false,
            last_cursor: 0,
            last_gap_records: 0,
        }
    }
}

pub struct IngestionHandle {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    status: Arc<Mutex<IngestionStatus>>,
}

impl IngestionHandle {
    pub fn start(
        storage: Arc<Storage>,
        source: Arc<dyn EventSource>,
        config: IngestionConfig,
    ) -> Result<Self, String> {
        config.validate()?;
        let stop = Arc::new(AtomicBool::new(false));
        let status = Arc::new(Mutex::new(IngestionStatus {
            state: IngestionState::Running,
            ..IngestionStatus::default()
        }));
        let thread_stop = Arc::clone(&stop);
        let thread_status = Arc::clone(&status);
        let thread = thread::Builder::new()
            .name("sentinel-storage-ingest".to_owned())
            .spawn(move || {
                run(storage, source, config, thread_stop, thread_status);
            })
            .map_err(|error| format!("failed to spawn ingestion worker: {error}"))?;
        Ok(Self {
            stop,
            thread: Some(thread),
            status,
        })
    }

    pub fn status(&self) -> IngestionStatus {
        self.status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for IngestionHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Do not join in Drop: application teardown must not hang. The thread
        // observes the flag at its next poll boundary.
    }
}

fn run(
    storage: Arc<Storage>,
    source: Arc<dyn EventSource>,
    config: IngestionConfig,
    stop: Arc<AtomicBool>,
    status: Arc<Mutex<IngestionStatus>>,
) {
    let (mut session_id, mut cursor) = match storage.ingest_cursor() {
        Ok(Some((session_id, cursor))) => (session_id, cursor),
        Ok(None) => (String::new(), 0),
        Err(error) => {
            set_error(&status, &format!("storage cursor unavailable: {error}"));
            (String::new(), 0)
        }
    };
    let mut active_closed = false;
    let mut consecutive_errors = 0u32;
    let mut batches_since_retention = 0u64;

    while !stop.load(Ordering::SeqCst) {
        match source.poll(cursor, config.batch_limit) {
            Ok(batch) => {
                consecutive_errors = 0;
                {
                    let mut state = status
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    state.poll_count = state.poll_count.saturating_add(1);
                    state.source_available = true;
                }

                let session_changed = !session_id.is_empty() && batch.session_id != session_id;
                if session_changed {
                    if !active_closed {
                        let _ = storage.mark_session_discontinued(&session_id);
                    }
                    session_id.clone_from(&batch.session_id);
                    cursor = 0;
                    active_closed = false;
                    let mut state = status
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    state.session_id = Some(session_id.clone());
                    state.session_closed = false;
                    state.sessions_observed = state.sessions_observed.saturating_add(1);
                    drop(state);
                    // Re-read the new session from its oldest retained cursor
                    // instead of applying a batch requested with the old
                    // session's cursor.
                    sleep_or_stop(&stop, config.empty_interval);
                    continue;
                }
                if batch.cursor_ahead {
                    // A cursor ahead of this session is not reinterpreted.
                    cursor = 0;
                    sleep_or_stop(&stop, config.empty_interval);
                    continue;
                }
                if session_id.is_empty() {
                    session_id.clone_from(&batch.session_id);
                    let mut state = status
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    state.session_id = Some(session_id.clone());
                    state.sessions_observed = state.sessions_observed.saturating_add(1);
                }

                let record_count = batch.records.len() as u64;
                match storage.apply_batch(&batch) {
                    Ok(stats) => {
                        cursor = batch.next_cursor;
                        active_closed = batch.session_closed;
                        {
                            let mut state = status
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner());
                            state.state = IngestionState::Running;
                            state.last_error = None;
                            state.last_success_at = now_rfc3339();
                            state.batch_count = state.batch_count.saturating_add(1);
                            let unique_records = stats
                                .inserted_events
                                .saturating_add(stats.inserted_invalid)
                                .saturating_add(stats.inserted_revisions)
                                .saturating_add(stats.inserted_session_ends);
                            state.record_count = state.record_count.saturating_add(unique_records);
                            state.session_id = Some(batch.session_id.clone());
                            state.session_closed = batch.session_closed;
                            state.last_cursor = cursor;
                            state.last_gap_records = batch.missed_records;
                        }
                        batches_since_retention += 1;
                        if batches_since_retention >= config.retention_every_batches {
                            batches_since_retention = 0;
                            if let Err(error) = storage.run_retention() {
                                set_error(&status, &format!("retention failed: {error}"));
                            }
                        }
                        let interval = if record_count == 0 {
                            config.empty_interval
                        } else {
                            config.poll_interval
                        };
                        sleep_or_stop(&stop, interval);
                    }
                    Err(error) => {
                        consecutive_errors = consecutive_errors.saturating_add(1);
                        set_degraded(&status, &format!("storage write failed: {error}"));
                        sleep_or_stop(&stop, backoff(&config, consecutive_errors));
                    }
                }
            }
            Err(error) => {
                consecutive_errors = consecutive_errors.saturating_add(1);
                let mut state = status
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                state.poll_count = state.poll_count.saturating_add(1);
                state.source_available = false;
                state.last_error = Some(bounded(&error));
                // A stopped sensor is an expected absence, not a storage
                // failure. Keep the worker running and retry.
                if state.state != IngestionState::Degraded {
                    state.state = IngestionState::Running;
                }
                drop(state);
                sleep_or_stop(&stop, backoff(&config, consecutive_errors));
            }
        }
    }

    let mut state = status
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    state.state = IngestionState::Stopped;
    state.source_available = false;
}

fn set_error(status: &Mutex<IngestionStatus>, message: &str) {
    let mut state = status
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    state.last_error = Some(bounded(message));
}

fn set_degraded(status: &Mutex<IngestionStatus>, message: &str) {
    let mut state = status
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    state.state = IngestionState::Degraded;
    state.last_error = Some(bounded(message));
}

fn bounded(message: &str) -> String {
    message.chars().take(512).collect()
}

fn backoff(config: &IngestionConfig, consecutive_errors: u32) -> Duration {
    let multiplier = 1u32 << consecutive_errors.saturating_sub(1).min(4);
    (config.error_backoff * multiplier).min(MAX_ERROR_BACKOFF)
}

fn sleep_or_stop(stop: &AtomicBool, duration: Duration) {
    let deadline = std::time::Instant::now() + duration;
    while !stop.load(Ordering::SeqCst) {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        std::thread::sleep(remaining.min(Duration::from_millis(50)));
    }
}

fn now_rfc3339() -> Option<String> {
    sentinel_events::time::now_unix_ms().and_then(sentinel_events::time::unix_ms_to_rfc3339)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::time::Instant;

    use sentinel_events::{
        DataCompleteness, EtwEventInfo, EventSource as EventSourceKind, FieldState,
        IdentityCertainty, PairingSnapshot, PairingStatus, PipelineRecord, ProcessEvent,
        ProcessEventType, ProcessIdentity, SessionEnd, SessionEndReason,
    };
    use sentinel_ipc::events::EventEnvelope;

    use crate::retention::RetentionPolicy;

    struct FakeSource {
        batches: Mutex<VecDeque<Result<EventBatch, String>>>,
        seen: Mutex<Vec<u64>>,
    }

    impl EventSource for FakeSource {
        fn poll(&self, after: u64, _limit: u16) -> Result<EventBatch, String> {
            self.seen.lock().unwrap().push(after);
            let next = self.batches.lock().unwrap().pop_front();
            match next {
                Some(result) => result,
                None => Err("no batch".to_owned()),
            }
        }
    }

    struct RepeatingSource {
        batch: EventBatch,
        seen: Mutex<Vec<u64>>,
    }

    impl EventSource for RepeatingSource {
        fn poll(&self, after: u64, _limit: u16) -> Result<EventBatch, String> {
            self.seen.lock().unwrap().push(after);
            Ok(self.batch.clone())
        }
    }

    fn storage() -> Arc<Storage> {
        Arc::new(
            Storage::open_in_memory(RetentionPolicy {
                max_age_days: 30,
                max_events: 1_000,
                max_bytes: 16 * 1024 * 1024,
            })
            .unwrap(),
        )
    }

    fn event(sequence: u64) -> ProcessEvent {
        ProcessEvent {
            event_id: format!("session:{sequence:016x}"),
            sequence,
            session_id: "session".to_owned(),
            event_type: ProcessEventType::Started,
            source: EventSourceKind::MicrosoftWindowsKernelProcess,
            identity: ProcessIdentity {
                pid: 100,
                create_time: 1,
            },
            identity_certainty: IdentityCertainty::Exact,
            parent_pid: Some(4),
            image_name: Some("test.exe".to_owned()),
            etw: EtwEventInfo {
                event_id: 1,
                version: 4,
                provider_sequence: Some(sequence),
                timestamp_100ns: None,
                occurred_at: Some("2026-10-31T00:00:00.000Z".to_owned()),
            },
            process_started_at: None,
            received_at: None,
            normalized_at: None,
            completeness: DataCompleteness {
                identity_certain: true,
                event_time_known: true,
                received_time_known: false,
                normalized_time_known: false,
                parent_pid_available: true,
                image_name: FieldState::Present,
                etw_sequence_available: true,
                issues: Vec::new(),
            },
            pairing: PairingStatus::Waiting,
        }
    }

    fn batch(session: &str, cursor: u64, records: Vec<PipelineRecord>) -> EventBatch {
        EventBatch {
            session_id: session.to_owned(),
            oldest_cursor: cursor,
            latest_cursor: cursor,
            next_cursor: cursor,
            cursor_expired: false,
            missed_records: 0,
            cursor_ahead: false,
            evictions: 0,
            session_closed: false,
            has_more: false,
            records: records
                .into_iter()
                .enumerate()
                .map(|(index, record)| EventEnvelope {
                    cursor: cursor + index as u64,
                    record,
                })
                .collect(),
        }
    }

    fn session_end(session: &str) -> SessionEnd {
        SessionEnd {
            session_id: session.to_owned(),
            ended_at: Some("2026-10-31T00:00:01.000Z".to_owned()),
            reason: SessionEndReason::Completed,
            error: None,
            open_start_events: Vec::new(),
            final_stats: None,
            pairing: PairingSnapshot {
                paired: 0,
                unpaired_starts: 1,
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
                stops_seen: 0,
                observed_start_image_names: Vec::new(),
                observed_start_image_names_truncated: false,
            },
        }
    }

    fn wait_until(timeout: Duration, mut condition: impl FnMut() -> bool) {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if condition() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("condition was not reached before timeout");
    }

    fn config() -> IngestionConfig {
        IngestionConfig {
            poll_interval: Duration::from_millis(20),
            empty_interval: Duration::from_millis(20),
            error_backoff: Duration::from_millis(20),
            batch_limit: 64,
            retention_every_batches: 100,
        }
    }

    #[test]
    fn ingests_batches_and_persists_the_cursor() {
        let storage = storage();
        let source = Arc::new(FakeSource {
            batches: Mutex::new(VecDeque::from([
                Ok(batch(
                    "session",
                    10,
                    vec![PipelineRecord::ProcessEvent(event(10))],
                )),
                Ok(EventBatch {
                    session_closed: true,
                    records: vec![EventEnvelope {
                        cursor: 11,
                        record: PipelineRecord::SessionEnd(session_end("session")),
                    }],
                    next_cursor: 11,
                    latest_cursor: 11,
                    ..batch("session", 11, Vec::new())
                }),
            ])),
            seen: Mutex::new(Vec::new()),
        });
        let handle = IngestionHandle::start(storage.clone(), source.clone(), config()).unwrap();
        wait_until(Duration::from_secs(3), || handle.status().batch_count >= 2);
        handle.stop();
        assert!(storage.status().unwrap().event_count >= 1);
        let sessions = storage.list_sessions(10).unwrap();
        assert_eq!(sessions[0].end_reason.as_deref(), Some("completed"));
        let seen = source.seen.lock().unwrap().clone();
        assert!(seen.contains(&0));
        assert!(seen.contains(&10));
    }

    #[test]
    fn a_write_failure_leaves_the_cursor_unchanged_and_replays_after_recovery() {
        let storage = storage();
        storage.set_query_only_for_test(true).unwrap();
        let source = Arc::new(RepeatingSource {
            batch: batch("session", 10, vec![PipelineRecord::ProcessEvent(event(10))]),
            seen: Mutex::new(Vec::new()),
        });
        let handle = IngestionHandle::start(storage.clone(), source.clone(), config()).unwrap();
        wait_until(Duration::from_secs(10), || {
            let status = handle.status();
            status.state == IngestionState::Degraded && status.last_cursor == 0
        });
        assert_eq!(storage.ingest_cursor().unwrap(), None);

        storage.set_query_only_for_test(false).unwrap();
        wait_until(Duration::from_secs(10), || {
            storage.ingest_cursor().unwrap() == Some(("session".to_owned(), 10))
        });
        let status = handle.status();
        assert_eq!(status.state, IngestionState::Running);
        assert_eq!(status.last_cursor, 10);
        assert_eq!(storage.status().unwrap().event_count, 1);
        handle.stop();
    }

    #[test]
    fn sensor_absence_is_not_a_storage_failure() {
        let storage = storage();
        let source = Arc::new(FakeSource {
            batches: Mutex::new(VecDeque::from([Err("sensor not running".to_owned())])),
            seen: Mutex::new(Vec::new()),
        });
        let handle = IngestionHandle::start(storage, source, config()).unwrap();
        wait_until(Duration::from_secs(2), || {
            let status = handle.status();
            status.poll_count >= 1 && !status.source_available
        });
        let status = handle.status();
        assert_eq!(status.state, IngestionState::Running);
        assert!(status.last_error.is_some());
        handle.stop();
    }

    #[test]
    fn a_new_sensor_session_discontinues_the_old_one() {
        let storage = storage();
        let source = Arc::new(FakeSource {
            batches: Mutex::new(VecDeque::from([
                Ok(batch(
                    "session-a",
                    1,
                    vec![PipelineRecord::ProcessEvent(event(1))],
                )),
                Ok(batch("session-b", 1, Vec::new())),
            ])),
            seen: Mutex::new(Vec::new()),
        });
        let handle = IngestionHandle::start(storage.clone(), source, config()).unwrap();
        wait_until(Duration::from_secs(3), || {
            handle.status().sessions_observed >= 2
        });
        handle.stop();
        let sessions = storage.list_sessions(10).unwrap();
        let old = sessions
            .iter()
            .find(|session| session.session_id == "session-a")
            .expect("old session");
        assert!(old.closed);
        assert_eq!(old.end_reason.as_deref(), Some("unobserved"));
        assert!(!old.has_session_end);
    }
}
