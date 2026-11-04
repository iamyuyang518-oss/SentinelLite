//! Bounded process-event pipeline.
//!
//! The ETW callback is the producer. It performs only bounded decoding and a
//! non-blocking `try_send`; normalization, pairing, health accounting and the
//! sink run on a dedicated consumer thread. A full queue drops the newest
//! event instead of blocking the ETW collection thread, and every drop is
//! counted separately from ETW session loss.
//!
//! On normal shutdown the producer is disconnected and the consumer drains the
//! remaining queue before emitting a session-end record. If a slow consumer
//! cannot drain within the configured timeout, shutdown reports the backlog
//! observed at the timeout instead of claiming the queue was empty.

use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::decode::{DecodedProcessEvent, EVENT_PROCESS_START, EVENT_PROCESS_STOP, FieldState};
use crate::event::{
    DataCompleteness, DataIssue, EtwEventInfo, EventSource, IdentityCertainty, IndeterminateReason,
    InvalidProcessEvent, MAX_SESSION_END_START_SAMPLES, PairingOrder, PairingRevision,
    PairingStatus, PipelineRecord, ProcessEvent, ProcessEventType, ProcessIdentity, SessionEnd,
    SessionEndReason, SessionEndStats, UnpairedReason, format_event_id, generate_session_id,
    truncate_image_name,
};
use crate::health::SensorHealth;
use crate::pair::{OpenStartSample, PairingOutcome, PairingState};
use crate::time::{filetime_to_unix_ms, now_unix_ms, unix_ms_to_rfc3339};

pub const DEFAULT_QUEUE_CAPACITY: usize = 4096;
pub const MIN_QUEUE_CAPACITY: usize = 64;
pub const MAX_QUEUE_CAPACITY: usize = 65_536;
pub const DEFAULT_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
pub const MAX_ERROR_CHARS: usize = 512;

const RECENT_EVENT_LIMIT: usize = 8;
const SHUTDOWN_POLL: Duration = Duration::from_millis(10);

type EndReason = Option<(SessionEndReason, Option<String>)>;
type SharedEndReason = Arc<Mutex<EndReason>>;

#[derive(Debug, Clone, Copy)]
pub struct PipelineConfig {
    pub capacity: usize,
    pub shutdown_timeout: Duration,
}

impl PipelineConfig {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            shutdown_timeout: DEFAULT_SHUTDOWN_TIMEOUT,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.capacity < MIN_QUEUE_CAPACITY {
            return Err(format!(
                "queue capacity must be at least {MIN_QUEUE_CAPACITY} events"
            ));
        }
        if self.capacity > MAX_QUEUE_CAPACITY {
            return Err(format!(
                "queue capacity must be at most {MAX_QUEUE_CAPACITY} events"
            ));
        }
        if self.shutdown_timeout.is_zero() {
            return Err("pipeline shutdown timeout must be greater than zero".to_owned());
        }
        Ok(())
    }
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            capacity: DEFAULT_QUEUE_CAPACITY,
            shutdown_timeout: DEFAULT_SHUTDOWN_TIMEOUT,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsumerState {
    Running,
    Stopped,
    Failed,
}

impl ConsumerState {
    fn from_u8(value: u8) -> Self {
        match value {
            2 => Self::Stopped,
            3 => Self::Failed,
            _ => Self::Running,
        }
    }

    fn as_u8(self) -> u8 {
        match self {
            Self::Running => 1,
            Self::Stopped => 2,
            Self::Failed => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushOutcome {
    Enqueued,
    DroppedQueueFull,
    DroppedConsumerGone,
    PipelineClosed,
}

#[derive(Debug, Clone)]
pub struct PipelineHealth {
    pub consumer_state: ConsumerState,
    pub consumer_error: Option<String>,
    pub dropped_total: u64,
    pub sink_errors: u64,
    pub shutdown_timed_out: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineSnapshot {
    pub session_id: String,
    pub capacity: usize,
    pub consumer_state: ConsumerState,
    pub consumer_error: Option<String>,
    pub shutdown_timed_out: bool,
    pub received: u64,
    pub enqueued: u64,
    pub consumed: u64,
    pub normalized: u64,
    pub invalid: u64,
    pub dropped_queue_full: u64,
    pub dropped_consumer_gone: u64,
    pub filtered_ignored: u64,
    pub backlog: u64,
    pub peak_backlog: u64,
    pub backlog_at_shutdown_timeout: Option<u64>,
    pub normalized_started: u64,
    pub normalized_stopped: u64,
    pub pairing_revisions: u64,
    pub sink_errors: u64,
    pub recent_events: Vec<ProcessEvent>,
    pub first_paired_stop: Option<ProcessEvent>,
    pub first_unpaired_event: Option<ProcessEvent>,
    pub first_revision: Option<PairingRevision>,
    pub first_invalid_event: Option<InvalidProcessEvent>,
    pub session_end: Option<SessionEnd>,
}

impl PipelineSnapshot {
    pub fn dropped_total(&self) -> u64 {
        self.dropped_queue_full + self.dropped_consumer_gone
    }
}

#[derive(Debug, Clone)]
pub struct CloseOutcome {
    pub drained: bool,
    pub timed_out: bool,
    pub backlog_at_timeout: Option<u64>,
}

pub trait EventSink: Send {
    fn emit(&mut self, record: &PipelineRecord) -> Result<(), String>;

    fn finish(&mut self) -> Result<(), String> {
        Ok(())
    }
}

struct CountingSink;

impl EventSink for CountingSink {
    fn emit(&mut self, _record: &PipelineRecord) -> Result<(), String> {
        Ok(())
    }
}

struct RawProcessEvent {
    decoded: DecodedProcessEvent,
    image_name_truncated: bool,
    etw_timestamp: Option<u64>,
    received_at_ms: Option<u64>,
}

struct RawInvalidEvent {
    event_id: u16,
    version: u8,
    payload_len: usize,
    error: String,
    etw_timestamp: Option<u64>,
    received_at_ms: Option<u64>,
}

enum RawPipelineEvent {
    Process(RawProcessEvent),
    Invalid(RawInvalidEvent),
    #[cfg(test)]
    TestStop,
}

#[derive(Default)]
struct PipelineCounters {
    received: AtomicU64,
    enqueued: AtomicU64,
    consumed: AtomicU64,
    normalized: AtomicU64,
    invalid: AtomicU64,
    dropped_queue_full: AtomicU64,
    dropped_consumer_gone: AtomicU64,
    filtered_ignored: AtomicU64,
    backlog: AtomicU64,
    peak_backlog: AtomicU64,
    normalized_started: AtomicU64,
    normalized_stopped: AtomicU64,
    pairing_revisions: AtomicU64,
    sink_errors: AtomicU64,
}

#[derive(Default)]
struct Diagnostics {
    recent_events: VecDeque<ProcessEvent>,
    first_paired_stop: Option<ProcessEvent>,
    first_unpaired_event: Option<ProcessEvent>,
    first_revision: Option<PairingRevision>,
    first_invalid_event: Option<InvalidProcessEvent>,
    session_end: Option<SessionEnd>,
}

#[derive(Debug, Clone, Copy, Default)]
struct FinalEtwStats {
    events_lost: Option<u64>,
    realtime_buffers_lost: Option<u64>,
}

struct ConsumerContext {
    counters: Arc<PipelineCounters>,
    diagnostics: Arc<Mutex<Diagnostics>>,
    health: Arc<SensorHealth>,
    pairing: Arc<Mutex<PairingState>>,
    consumer_error: Arc<Mutex<Option<String>>>,
    final_etw_stats: Arc<Mutex<FinalEtwStats>>,
    shutdown_timed_out: Arc<AtomicBool>,
    backlog_at_shutdown_timeout: Arc<Mutex<Option<u64>>>,
    capacity: u64,
    session_id: String,
}

pub struct EventPipeline {
    sender: Mutex<Option<SyncSender<RawPipelineEvent>>>,
    counters: Arc<PipelineCounters>,
    diagnostics: Arc<Mutex<Diagnostics>>,
    health: Arc<SensorHealth>,
    consumer: Mutex<Option<JoinHandle<()>>>,
    consumer_state: Arc<AtomicU8>,
    consumer_error: Arc<Mutex<Option<String>>>,
    session_id: String,
    end_reason: SharedEndReason,
    shutdown_timed_out: Arc<AtomicBool>,
    backlog_at_shutdown_timeout: Arc<Mutex<Option<u64>>>,
    final_etw_stats: Arc<Mutex<FinalEtwStats>>,
    capacity: usize,
    shutdown_timeout: Duration,
    close_outcome: Mutex<Option<CloseOutcome>>,
    startup_error: Option<String>,
}

impl EventPipeline {
    pub fn start(
        config: PipelineConfig,
        health: Arc<SensorHealth>,
        pairing: Arc<Mutex<PairingState>>,
    ) -> Self {
        Self::start_with_sink(config, health, pairing, Box::new(CountingSink))
    }

    pub fn start_with_sink(
        config: PipelineConfig,
        health: Arc<SensorHealth>,
        pairing: Arc<Mutex<PairingState>>,
        sink: Box<dyn EventSink>,
    ) -> Self {
        Self::start_with_sink_and_session(config, health, pairing, generate_session_id(), sink)
    }

    /// Start the pipeline with a caller-provided session identity. The
    /// delivery broker uses the same identity so its session and the event
    /// IDs cannot disagree.
    pub fn start_with_sink_and_session(
        config: PipelineConfig,
        health: Arc<SensorHealth>,
        pairing: Arc<Mutex<PairingState>>,
        session_id: String,
        sink: Box<dyn EventSink>,
    ) -> Self {
        let counters = Arc::new(PipelineCounters::default());
        let diagnostics = Arc::new(Mutex::new(Diagnostics::default()));
        let consumer_state = Arc::new(AtomicU8::new(ConsumerState::Running.as_u8()));
        let consumer_error = Arc::new(Mutex::new(None));
        let end_reason = Arc::new(Mutex::new(None));
        let shutdown_timed_out = Arc::new(AtomicBool::new(false));
        let backlog_at_shutdown_timeout = Arc::new(Mutex::new(None));
        let final_etw_stats = Arc::new(Mutex::new(FinalEtwStats::default()));
        let capacity = config.capacity.max(1);
        let (sender, receiver) = mpsc::sync_channel::<RawPipelineEvent>(capacity);

        let context = ConsumerContext {
            counters: Arc::clone(&counters),
            diagnostics: Arc::clone(&diagnostics),
            health: Arc::clone(&health),
            pairing,
            consumer_error: Arc::clone(&consumer_error),
            final_etw_stats: Arc::clone(&final_etw_stats),
            shutdown_timed_out: Arc::clone(&shutdown_timed_out),
            backlog_at_shutdown_timeout: Arc::clone(&backlog_at_shutdown_timeout),
            capacity: capacity as u64,
            session_id: session_id.clone(),
        };
        let thread_state = Arc::clone(&consumer_state);
        let thread_error = Arc::clone(&consumer_error);
        let thread_end_reason = Arc::clone(&end_reason);

        let consumer = thread::Builder::new()
            .name("sentinel-sensor-normalizer".to_owned())
            .spawn(move || {
                run_consumer(
                    receiver,
                    sink,
                    context,
                    thread_end_reason,
                    thread_state,
                    thread_error,
                );
            });
        let (consumer, startup_error) = match consumer {
            Ok(handle) => (Some(handle), None),
            Err(error) => {
                let message = format!("failed to spawn event pipeline consumer: {error}");
                consumer_state.store(ConsumerState::Failed.as_u8(), Ordering::Relaxed);
                *consumer_error
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(message.clone());
                (None, Some(message))
            }
        };

        Self {
            sender: Mutex::new(Some(sender)),
            counters,
            diagnostics,
            health,
            consumer: Mutex::new(consumer),
            consumer_state,
            consumer_error,
            session_id,
            end_reason,
            shutdown_timed_out,
            backlog_at_shutdown_timeout,
            final_etw_stats,
            capacity,
            shutdown_timeout: config.shutdown_timeout,
            close_outcome: Mutex::new(None),
            startup_error,
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn startup_error(&self) -> Option<&str> {
        self.startup_error.as_deref()
    }

    /// Supply the final ETW loss counters after the session has stopped.
    /// `None` values stay unknown; the consumer must not substitute zero.
    pub fn set_final_etw_stats(
        &self,
        events_lost: Option<u64>,
        realtime_buffers_lost: Option<u64>,
    ) {
        *self
            .final_etw_stats
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = FinalEtwStats {
            events_lost,
            realtime_buffers_lost,
        };
    }

    pub fn push_process(&self, mut decoded: DecodedProcessEvent, etw_timestamp: Option<u64>) {
        let image_name_truncated = decoded.image_name.as_mut().is_some_and(truncate_image_name);
        if image_name_truncated {
            decoded.image_name_state = FieldState::Partial;
            decoded.partial = true;
        }
        let event = RawPipelineEvent::Process(RawProcessEvent {
            decoded,
            image_name_truncated,
            etw_timestamp,
            received_at_ms: now_unix_ms(),
        });
        let _ = self.push(event);
    }

    pub fn push_invalid(
        &self,
        event_id: u16,
        version: u8,
        payload_len: usize,
        error: &str,
        etw_timestamp: Option<u64>,
    ) {
        let event = RawPipelineEvent::Invalid(RawInvalidEvent {
            event_id,
            version,
            payload_len,
            error: bounded_error(error),
            etw_timestamp,
            received_at_ms: now_unix_ms(),
        });
        let _ = self.push(event);
    }

    pub fn record_filtered_ignored(&self) {
        self.counters
            .filtered_ignored
            .fetch_add(1, Ordering::Relaxed);
        self.health.record_ignored();
    }

    fn push(&self, event: RawPipelineEvent) -> PushOutcome {
        self.counters.received.fetch_add(1, Ordering::Relaxed);
        let guard = self
            .sender
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(sender) = guard.as_ref() else {
            drop(guard);
            self.counters
                .dropped_consumer_gone
                .fetch_add(1, Ordering::Relaxed);
            self.health.record_queue_dropped();
            return PushOutcome::PipelineClosed;
        };

        // The counter tracks "enqueued but not yet consumed", including the
        // event currently held by the consumer. The channel itself is strictly
        // bounded by `capacity`; the counter can briefly read capacity + 1.
        let queued = self.counters.backlog.fetch_add(1, Ordering::Relaxed) + 1;
        match sender.try_send(event) {
            Ok(()) => {
                self.counters.enqueued.fetch_add(1, Ordering::Relaxed);
                self.counters
                    .peak_backlog
                    .fetch_max(queued, Ordering::Relaxed);
                PushOutcome::Enqueued
            }
            Err(TrySendError::Full(_)) => {
                self.counters.backlog.fetch_sub(1, Ordering::Relaxed);
                self.counters
                    .dropped_queue_full
                    .fetch_add(1, Ordering::Relaxed);
                self.health.record_queue_dropped();
                PushOutcome::DroppedQueueFull
            }
            Err(TrySendError::Disconnected(_)) => {
                self.counters.backlog.fetch_sub(1, Ordering::Relaxed);
                self.counters
                    .dropped_consumer_gone
                    .fetch_add(1, Ordering::Relaxed);
                self.health.record_queue_dropped();
                self.mark_consumer_gone();
                PushOutcome::DroppedConsumerGone
            }
        }
    }

    fn mark_consumer_gone(&self) {
        if self.consumer_state() == ConsumerState::Running {
            self.consumer_state
                .store(ConsumerState::Failed.as_u8(), Ordering::Relaxed);
            let mut error = self
                .consumer_error
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if error.is_none() {
                *error = Some("event pipeline consumer disconnected".to_owned());
            }
        }
    }

    pub fn close(&self, reason: SessionEndReason) -> CloseOutcome {
        self.close_inner(reason, None)
    }

    pub fn close_with_error(&self, message: impl Into<String>) -> CloseOutcome {
        self.close_inner(SessionEndReason::Error, Some(message.into()))
    }

    fn close_inner(&self, reason: SessionEndReason, error: Option<String>) -> CloseOutcome {
        let mut close_outcome = self
            .close_outcome
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(outcome) = close_outcome.as_ref() {
            return outcome.clone();
        }

        {
            let mut end_reason = self
                .end_reason
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if end_reason.is_none() {
                *end_reason = Some((reason, error));
            }
        }

        let sender = self
            .sender
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        drop(sender);

        let handle = self
            .consumer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        let outcome = match handle {
            None => CloseOutcome {
                drained: self.consumer_state() == ConsumerState::Stopped,
                timed_out: false,
                backlog_at_timeout: None,
            },
            Some(handle) => {
                let deadline = Instant::now() + self.shutdown_timeout;
                while !handle.is_finished() && Instant::now() < deadline {
                    thread::sleep(SHUTDOWN_POLL);
                }
                if handle.is_finished() {
                    let _ = handle.join();
                    CloseOutcome {
                        drained: true,
                        timed_out: false,
                        backlog_at_timeout: None,
                    }
                } else {
                    let backlog = self.counters.backlog.load(Ordering::Relaxed);
                    *self
                        .backlog_at_shutdown_timeout
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(backlog);
                    self.shutdown_timed_out.store(true, Ordering::Relaxed);
                    CloseOutcome {
                        drained: false,
                        timed_out: true,
                        backlog_at_timeout: Some(backlog),
                    }
                }
            }
        };
        *close_outcome = Some(outcome.clone());
        outcome
    }

    pub fn health_snapshot(&self) -> PipelineHealth {
        PipelineHealth {
            consumer_state: self.consumer_state(),
            consumer_error: self
                .consumer_error
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone(),
            dropped_total: self.counters.dropped_queue_full.load(Ordering::Relaxed)
                + self.counters.dropped_consumer_gone.load(Ordering::Relaxed),
            sink_errors: self.counters.sink_errors.load(Ordering::Relaxed),
            shutdown_timed_out: self.shutdown_timed_out.load(Ordering::Relaxed),
        }
    }

    pub fn last_error(&self) -> Option<String> {
        let health = self.health_snapshot();
        if let Some(error) = health.consumer_error {
            return Some(error);
        }
        if health.dropped_total > 0 {
            return Some(format!(
                "event pipeline dropped {} events (bounded queue)",
                health.dropped_total
            ));
        }
        if health.shutdown_timed_out {
            return Some("event pipeline did not drain within the shutdown timeout".to_owned());
        }
        if health.sink_errors > 0 {
            return Some("event pipeline sink reported errors".to_owned());
        }
        None
    }

    pub fn snapshot(&self) -> PipelineSnapshot {
        let diagnostics = self
            .diagnostics
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        PipelineSnapshot {
            session_id: self.session_id.clone(),
            capacity: self.capacity,
            consumer_state: self.consumer_state(),
            consumer_error: self
                .consumer_error
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone(),
            shutdown_timed_out: self.shutdown_timed_out.load(Ordering::Relaxed),
            received: self.counters.received.load(Ordering::Relaxed),
            enqueued: self.counters.enqueued.load(Ordering::Relaxed),
            consumed: self.counters.consumed.load(Ordering::Relaxed),
            normalized: self.counters.normalized.load(Ordering::Relaxed),
            invalid: self.counters.invalid.load(Ordering::Relaxed),
            dropped_queue_full: self.counters.dropped_queue_full.load(Ordering::Relaxed),
            dropped_consumer_gone: self.counters.dropped_consumer_gone.load(Ordering::Relaxed),
            filtered_ignored: self.counters.filtered_ignored.load(Ordering::Relaxed),
            backlog: self.counters.backlog.load(Ordering::Relaxed),
            peak_backlog: self.counters.peak_backlog.load(Ordering::Relaxed),
            backlog_at_shutdown_timeout: *self
                .backlog_at_shutdown_timeout
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
            normalized_started: self.counters.normalized_started.load(Ordering::Relaxed),
            normalized_stopped: self.counters.normalized_stopped.load(Ordering::Relaxed),
            pairing_revisions: self.counters.pairing_revisions.load(Ordering::Relaxed),
            sink_errors: self.counters.sink_errors.load(Ordering::Relaxed),
            recent_events: diagnostics.recent_events.iter().cloned().collect(),
            first_paired_stop: diagnostics.first_paired_stop.clone(),
            first_unpaired_event: diagnostics.first_unpaired_event.clone(),
            first_revision: diagnostics.first_revision.clone(),
            first_invalid_event: diagnostics.first_invalid_event.clone(),
            session_end: diagnostics.session_end.clone(),
        }
    }

    fn consumer_state(&self) -> ConsumerState {
        ConsumerState::from_u8(self.consumer_state.load(Ordering::Relaxed))
    }

    #[cfg(test)]
    pub fn stop_consumer_for_test(&self) {
        let guard = self
            .sender
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(sender) = guard.as_ref() {
            let _ = sender.send(RawPipelineEvent::TestStop);
        }
    }
}

impl Drop for EventPipeline {
    fn drop(&mut self) {
        // Detach teardown: never block a test or the process exit path on a
        // slow consumer. Normal production shutdown uses close() first.
        let _ = self
            .sender
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
    }
}

fn run_consumer(
    receiver: Receiver<RawPipelineEvent>,
    mut sink: Box<dyn EventSink>,
    context: ConsumerContext,
    end_reason: SharedEndReason,
    consumer_state: Arc<AtomicU8>,
    consumer_error: Arc<Mutex<Option<String>>>,
) {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let mut normalizer = Normalizer::new();
        let mut sink_failed = false;

        while let Ok(raw) = receiver.recv() {
            match raw {
                #[cfg(test)]
                RawPipelineEvent::TestStop => break,
                RawPipelineEvent::Process(raw) => {
                    context.counters.backlog.fetch_sub(1, Ordering::Relaxed);
                    context.counters.consumed.fetch_add(1, Ordering::Relaxed);
                    handle_process(&context, &mut normalizer, &mut *sink, &mut sink_failed, raw);
                }
                RawPipelineEvent::Invalid(raw) => {
                    context.counters.backlog.fetch_sub(1, Ordering::Relaxed);
                    context.counters.consumed.fetch_add(1, Ordering::Relaxed);
                    handle_invalid(&context, &mut normalizer, &mut *sink, &mut sink_failed, raw);
                }
            }
        }

        let session_end = build_session_end(&context, &end_reason);
        if !sink_failed {
            if let Err(error) = sink.emit(&PipelineRecord::SessionEnd(session_end.clone())) {
                record_sink_error(&context, &mut sink_failed, &error);
            }
        }
        if let Err(error) = sink.finish() {
            record_sink_error(&context, &mut sink_failed, &error);
        }
        context
            .diagnostics
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .session_end = Some(session_end);
    }));

    match result {
        Ok(()) => {
            consumer_state.store(ConsumerState::Stopped.as_u8(), Ordering::Relaxed);
        }
        Err(_) => {
            consumer_state.store(ConsumerState::Failed.as_u8(), Ordering::Relaxed);
            let mut error = consumer_error
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if error.is_none() {
                *error = Some("event pipeline consumer panicked".to_owned());
            }
        }
    }
}

struct Normalizer {
    next_sequence: u64,
}

impl Normalizer {
    fn new() -> Self {
        Self { next_sequence: 0 }
    }

    fn next_sequence(&mut self) -> u64 {
        self.next_sequence = self.next_sequence.saturating_add(1);
        self.next_sequence
    }
}

fn handle_process(
    context: &ConsumerContext,
    normalizer: &mut Normalizer,
    sink: &mut dyn EventSink,
    sink_failed: &mut bool,
    raw: RawProcessEvent,
) {
    let event_type = match raw.decoded.event_id {
        EVENT_PROCESS_START => ProcessEventType::Started,
        EVENT_PROCESS_STOP => ProcessEventType::Stopped,
        _ => {
            handle_invalid(
                context,
                normalizer,
                sink,
                sink_failed,
                RawInvalidEvent {
                    event_id: raw.decoded.event_id,
                    version: raw.decoded.version,
                    payload_len: 0,
                    error: "decoded event id is not process start/stop".to_owned(),
                    etw_timestamp: raw.etw_timestamp,
                    received_at_ms: raw.received_at_ms,
                },
            );
            return;
        }
    };

    let sequence = normalizer.next_sequence();
    let event_id = format_event_id(&context.session_id, sequence);
    let normalized_at = now_unix_ms().and_then(unix_ms_to_rfc3339);
    let received_at = raw.received_at_ms.and_then(unix_ms_to_rfc3339);
    let occurred_at = raw
        .etw_timestamp
        .and_then(filetime_to_unix_ms)
        .and_then(unix_ms_to_rfc3339);
    let process_started_at = if raw.decoded.create_time == 0 {
        None
    } else {
        filetime_to_unix_ms(raw.decoded.create_time).and_then(unix_ms_to_rfc3339)
    };
    let identity_certainty = if raw.decoded.create_time == 0 {
        IdentityCertainty::Indeterminate
    } else {
        IdentityCertainty::Exact
    };
    let image_state = if raw.image_name_truncated {
        FieldState::Partial
    } else {
        raw.decoded.image_name_state
    };

    let outcome = {
        let mut pairing = context
            .pairing
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        pairing.observe(&raw.decoded, sequence)
    };
    let (pairing_status, revision) =
        map_pairing(outcome, &event_id, &context.session_id, &normalized_at);

    let mut completeness = DataCompleteness {
        identity_certain: raw.decoded.create_time != 0,
        event_time_known: occurred_at.is_some(),
        received_time_known: received_at.is_some(),
        normalized_time_known: normalized_at.is_some(),
        parent_pid_available: raw.decoded.parent_pid.is_some(),
        image_name: image_state,
        etw_sequence_available: raw.decoded.sequence.is_some(),
        issues: Vec::new(),
    };
    if raw.decoded.create_time == 0 {
        completeness.push_issue(DataIssue::IndeterminateProcessCreateTime);
    } else if process_started_at.is_none() {
        completeness.push_issue(DataIssue::ProcessStartTimeUnrepresentable);
    }
    if raw.etw_timestamp.is_none() {
        completeness.push_issue(DataIssue::MissingEventTimestamp);
    } else if occurred_at.is_none() {
        completeness.push_issue(DataIssue::EventTimestampUnrepresentable);
    }
    if received_at.is_none() {
        completeness.push_issue(DataIssue::MissingReceiveTime);
    }
    if normalized_at.is_none() {
        completeness.push_issue(DataIssue::MissingNormalizeTime);
    }
    match image_state {
        FieldState::Missing => completeness.push_issue(DataIssue::MissingImageName),
        FieldState::Partial => completeness.push_issue(DataIssue::PartialImageName),
        FieldState::Present => {}
    }
    if raw.image_name_truncated {
        completeness.push_issue(DataIssue::ImageNameTruncated);
    }
    if event_type == ProcessEventType::Started && raw.decoded.parent_pid.is_none() {
        completeness.push_issue(DataIssue::MissingParentPid);
    }

    if let Some(timestamp) = raw.etw_timestamp {
        context.health.record_event_time(timestamp);
    } else {
        context.health.record_partial("event timestamp missing");
    }
    context.health.record_processed();
    if raw.image_name_truncated || raw.decoded.image_name_state == FieldState::Partial {
        context.health.record_partial("partial image name");
    }
    if raw.decoded.create_time == 0 {
        context
            .health
            .record_integrity_issue("process create time missing; identity indeterminate");
    }
    if received_at.is_none() || normalized_at.is_none() {
        context
            .health
            .record_integrity_issue("sensor event time unavailable");
    }

    context.counters.normalized.fetch_add(1, Ordering::Relaxed);
    match event_type {
        ProcessEventType::Started => {
            context
                .counters
                .normalized_started
                .fetch_add(1, Ordering::Relaxed);
        }
        ProcessEventType::Stopped => {
            context
                .counters
                .normalized_stopped
                .fetch_add(1, Ordering::Relaxed);
        }
    }
    if revision.is_some() {
        context
            .counters
            .pairing_revisions
            .fetch_add(1, Ordering::Relaxed);
    }

    let event = ProcessEvent {
        event_id,
        sequence,
        session_id: context.session_id.clone(),
        event_type,
        source: EventSource::MicrosoftWindowsKernelProcess,
        identity: ProcessIdentity {
            pid: raw.decoded.pid,
            create_time: raw.decoded.create_time,
        },
        identity_certainty,
        parent_pid: raw.decoded.parent_pid,
        image_name: raw.decoded.image_name,
        etw: EtwEventInfo {
            event_id: raw.decoded.event_id,
            version: raw.decoded.version,
            provider_sequence: raw.decoded.sequence,
            timestamp_100ns: raw.etw_timestamp,
            occurred_at,
        },
        process_started_at,
        received_at,
        normalized_at,
        completeness,
        pairing: pairing_status,
    };

    let record = PipelineRecord::ProcessEvent(event);
    emit_record(context, sink, sink_failed, &record);
    if let PipelineRecord::ProcessEvent(event) = record {
        store_event_diagnostics(context, event);
    }

    if let Some(revision) = revision {
        let record = PipelineRecord::PairingRevision(revision);
        emit_record(context, sink, sink_failed, &record);
        if let PipelineRecord::PairingRevision(revision) = record {
            let mut diagnostics = context
                .diagnostics
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if diagnostics.first_revision.is_none() {
                diagnostics.first_revision = Some(revision);
            }
        }
    }
}

fn handle_invalid(
    context: &ConsumerContext,
    normalizer: &mut Normalizer,
    sink: &mut dyn EventSink,
    sink_failed: &mut bool,
    raw: RawInvalidEvent,
) {
    let sequence = normalizer.next_sequence();
    let event_id = format_event_id(&context.session_id, sequence);
    let normalized_at = now_unix_ms().and_then(unix_ms_to_rfc3339);
    let occurred_at = raw
        .etw_timestamp
        .and_then(filetime_to_unix_ms)
        .and_then(unix_ms_to_rfc3339);
    let received_at = raw.received_at_ms.and_then(unix_ms_to_rfc3339);

    context.counters.invalid.fetch_add(1, Ordering::Relaxed);
    context.health.record_decode_error(&format!(
        "event {} v{}: {}",
        raw.event_id, raw.version, raw.error
    ));

    let event = InvalidProcessEvent {
        event_id,
        sequence,
        session_id: context.session_id.clone(),
        etw_event_id: raw.event_id,
        etw_version: raw.version,
        payload_len: raw.payload_len,
        error: raw.error,
        etw_timestamp_100ns: raw.etw_timestamp,
        occurred_at,
        received_at,
        normalized_at,
    };
    let record = PipelineRecord::InvalidEvent(event);
    emit_record(context, sink, sink_failed, &record);
    if let PipelineRecord::InvalidEvent(event) = record {
        let mut diagnostics = context
            .diagnostics
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if diagnostics.first_invalid_event.is_none() {
            diagnostics.first_invalid_event = Some(event);
        }
    }
}

fn store_event_diagnostics(context: &ConsumerContext, event: ProcessEvent) {
    let mut diagnostics = context
        .diagnostics
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if diagnostics.recent_events.len() == RECENT_EVENT_LIMIT {
        diagnostics.recent_events.pop_front();
    }
    if diagnostics.first_paired_stop.is_none()
        && event.event_type == ProcessEventType::Stopped
        && matches!(&event.pairing, PairingStatus::Paired { .. })
    {
        diagnostics.first_paired_stop = Some(event.clone());
    }
    if diagnostics.first_unpaired_event.is_none()
        && matches!(
            &event.pairing,
            PairingStatus::Unpaired { .. } | PairingStatus::Indeterminate { .. }
        )
    {
        diagnostics.first_unpaired_event = Some(event.clone());
    }
    diagnostics.recent_events.push_back(event);
}

fn emit_record(
    context: &ConsumerContext,
    sink: &mut dyn EventSink,
    sink_failed: &mut bool,
    record: &PipelineRecord,
) {
    if *sink_failed {
        return;
    }
    if let Err(error) = sink.emit(record) {
        record_sink_error(context, sink_failed, &error);
    }
}

fn record_sink_error(context: &ConsumerContext, sink_failed: &mut bool, error: &str) {
    context.counters.sink_errors.fetch_add(1, Ordering::Relaxed);
    *sink_failed = true;
    let message = format!("event pipeline sink error: {}", bounded_error(error));
    let mut slot = context
        .consumer_error
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if slot.is_none() {
        *slot = Some(message);
    }
}

fn build_session_end(context: &ConsumerContext, end_reason: &Mutex<EndReason>) -> SessionEnd {
    let (reason, error) = end_reason
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take()
        .unwrap_or((SessionEndReason::Shutdown, None));
    let (pairing, open_starts) = {
        let state = context
            .pairing
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        (
            state.snapshot(),
            state.open_start_samples(MAX_SESSION_END_START_SAMPLES),
        )
    };
    let open_start_events: Vec<String> = open_starts
        .into_iter()
        .map(|sample: OpenStartSample| format_event_id(&context.session_id, sample.event_sequence))
        .collect();
    let final_etw = *context
        .final_etw_stats
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let backlog_at_shutdown_timeout = *context
        .backlog_at_shutdown_timeout
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let final_stats = SessionEndStats {
        events_lost: final_etw.events_lost,
        realtime_buffers_lost: final_etw.realtime_buffers_lost,
        pipeline_capacity: context.capacity,
        pipeline_received: context.counters.received.load(Ordering::Relaxed),
        pipeline_enqueued: context.counters.enqueued.load(Ordering::Relaxed),
        pipeline_consumed: context.counters.consumed.load(Ordering::Relaxed),
        pipeline_normalized: context.counters.normalized.load(Ordering::Relaxed),
        pipeline_invalid: context.counters.invalid.load(Ordering::Relaxed),
        pipeline_dropped_queue_full: context.counters.dropped_queue_full.load(Ordering::Relaxed),
        pipeline_dropped_consumer_gone: context
            .counters
            .dropped_consumer_gone
            .load(Ordering::Relaxed),
        pipeline_filtered_ignored: context.counters.filtered_ignored.load(Ordering::Relaxed),
        pipeline_backlog: context.counters.backlog.load(Ordering::Relaxed),
        pipeline_peak_backlog: context.counters.peak_backlog.load(Ordering::Relaxed),
        pipeline_backlog_at_shutdown_timeout: backlog_at_shutdown_timeout,
        pipeline_shutdown_timed_out: context.shutdown_timed_out.load(Ordering::Relaxed),
        pipeline_sink_errors: context.counters.sink_errors.load(Ordering::Relaxed),
    };

    SessionEnd {
        session_id: context.session_id.clone(),
        ended_at: now_unix_ms().and_then(unix_ms_to_rfc3339),
        reason,
        error,
        open_start_events,
        pairing,
        final_stats: Some(final_stats),
    }
}

fn map_pairing(
    outcome: PairingOutcome,
    event_id: &str,
    session_id: &str,
    normalized_at: &Option<String>,
) -> (PairingStatus, Option<PairingRevision>) {
    match outcome {
        PairingOutcome::StartOpened => (PairingStatus::Waiting, None),
        PairingOutcome::StartDuplicate {
            first_event_sequence,
        } => (
            PairingStatus::Unpaired {
                reason: UnpairedReason::DuplicateStart {
                    first_event_id: format_event_id(session_id, first_event_sequence),
                },
            },
            None,
        ),
        PairingOutcome::StartPairedEarlierStop {
            stop_event_sequence,
            sequence_match,
        } => {
            let stop_event_id = format_event_id(session_id, stop_event_sequence);
            let status = PairingStatus::Paired {
                start_event_id: event_id.to_owned(),
                stop_event_id: stop_event_id.clone(),
                order: PairingOrder::OutOfOrder,
                sequence_match,
            };
            let revision = PairingRevision {
                session_id: session_id.to_owned(),
                stop_event_id,
                start_event_id: event_id.to_owned(),
                sequence_match,
                reason: "start arrived after an orphan stop".to_owned(),
                normalized_at: normalized_at.clone(),
            };
            (status, Some(revision))
        }
        PairingOutcome::StartOverflow => (
            PairingStatus::Unpaired {
                reason: UnpairedReason::StartNotRetained,
            },
            None,
        ),
        PairingOutcome::StopPaired {
            start_event_sequence,
            sequence_match,
        } => (
            PairingStatus::Paired {
                start_event_id: format_event_id(session_id, start_event_sequence),
                stop_event_id: event_id.to_owned(),
                order: PairingOrder::InOrder,
                sequence_match,
            },
            None,
        ),
        PairingOutcome::StopOrphan { retained } => (
            PairingStatus::Unpaired {
                reason: if retained {
                    UnpairedReason::StopWithoutObservedStart
                } else {
                    UnpairedReason::OrphanStopNotRetained
                },
            },
            None,
        ),
        PairingOutcome::StopDuplicate {
            first_event_sequence,
        } => (
            PairingStatus::Unpaired {
                reason: UnpairedReason::DuplicateStop {
                    first_event_id: format_event_id(session_id, first_event_sequence),
                },
            },
            None,
        ),
        PairingOutcome::StopIdentityMismatch {
            open_create_time, ..
        } => (
            PairingStatus::Unpaired {
                reason: UnpairedReason::IdentityMismatch { open_create_time },
            },
            None,
        ),
        PairingOutcome::IdentityIndeterminate => (
            PairingStatus::Indeterminate {
                reason: IndeterminateReason::MissingProcessCreateTime,
            },
            None,
        ),
        PairingOutcome::Ignored => (
            PairingStatus::Indeterminate {
                reason: IndeterminateReason::UnsupportedEventType,
            },
            None,
        ),
    }
}

fn bounded_error(message: &str) -> String {
    let mut output = String::new();
    for (count, character) in message.chars().enumerate() {
        if count == MAX_ERROR_CHARS {
            break;
        }
        output.push(if character.is_control() {
            ' '
        } else {
            character
        });
    }
    output.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;
    use std::sync::{Condvar, Mutex};

    use super::*;
    use crate::decode::decode_process_event;
    use crate::event::MAX_IMAGE_NAME_CHARS;
    use crate::time::FILETIME_UNIX_EPOCH_DIFF_100NS;

    fn filetime(unix_ms: u64) -> u64 {
        FILETIME_UNIX_EPOCH_DIFF_100NS + unix_ms * 10_000
    }

    struct RecordingSink {
        records: Arc<Mutex<Vec<PipelineRecord>>>,
    }

    impl EventSink for RecordingSink {
        fn emit(&mut self, record: &PipelineRecord) -> Result<(), String> {
            self.records
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(record.clone());
            Ok(())
        }
    }

    struct BlockingSink {
        entered: Arc<AtomicUsize>,
        gate: Arc<(Mutex<bool>, Condvar)>,
    }

    impl EventSink for BlockingSink {
        fn emit(&mut self, _record: &PipelineRecord) -> Result<(), String> {
            self.entered.fetch_add(1, Ordering::SeqCst);
            let (lock, condvar) = &*self.gate;
            let mut open = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            while !*open {
                open = condvar
                    .wait(open)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            }
            Ok(())
        }
    }

    struct FailingSink {
        calls: Arc<AtomicUsize>,
    }

    impl EventSink for FailingSink {
        fn emit(&mut self, _record: &PipelineRecord) -> Result<(), String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Err("disk full".to_owned())
        }
    }

    type RecordingPipeline = (
        EventPipeline,
        Arc<SensorHealth>,
        Arc<Mutex<PairingState>>,
        Arc<Mutex<Vec<PipelineRecord>>>,
    );

    fn recording_pipeline(capacity: usize) -> RecordingPipeline {
        let health = Arc::new(SensorHealth::new());
        health.mark_starting();
        health.mark_active();
        let pairing = Arc::new(Mutex::new(PairingState::new()));
        let records = Arc::new(Mutex::new(Vec::new()));
        let pipeline = EventPipeline::start_with_sink(
            PipelineConfig {
                capacity,
                shutdown_timeout: Duration::from_secs(2),
            },
            Arc::clone(&health),
            Arc::clone(&pairing),
            Box::new(RecordingSink {
                records: Arc::clone(&records),
            }),
        );
        (pipeline, health, pairing, records)
    }

    fn decoded(
        event_id: u16,
        pid: u32,
        create_time: u64,
        sequence: Option<u64>,
        image: Option<&str>,
    ) -> DecodedProcessEvent {
        DecodedProcessEvent {
            event_id,
            version: 2,
            pid,
            create_time,
            parent_pid: if event_id == EVENT_PROCESS_START {
                Some(1)
            } else {
                None
            },
            sequence,
            image_name: image.map(ToOwned::to_owned),
            image_name_state: if image.is_some() {
                FieldState::Present
            } else {
                FieldState::Missing
            },
            partial: false,
        }
    }

    fn start(pid: u32, create_time: u64) -> DecodedProcessEvent {
        decoded(
            EVENT_PROCESS_START,
            pid,
            create_time,
            Some(7),
            Some("cmd.exe"),
        )
    }

    fn stop(pid: u32, create_time: u64) -> DecodedProcessEvent {
        decoded(
            EVENT_PROCESS_STOP,
            pid,
            create_time,
            Some(7),
            Some("cmd.exe"),
        )
    }

    fn wait_until(predicate: impl Fn() -> bool) {
        for _ in 0..400 {
            if predicate() {
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }
        panic!("condition was not reached before the test deadline");
    }

    fn records_of(records: &Arc<Mutex<Vec<PipelineRecord>>>) -> Vec<PipelineRecord> {
        records
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    #[test]
    fn session_end_contains_final_pipeline_and_etw_stats() {
        let (pipeline, health, _pairing, _records) = recording_pipeline(64);
        health.record_received();
        pipeline.push_process(start(10, filetime(1_000)), Some(filetime(2_000)));
        health.record_received();
        pipeline.push_process(stop(10, filetime(1_000)), Some(filetime(3_000)));
        let outcome = pipeline.close(SessionEndReason::Completed);
        assert!(outcome.drained);

        let session_end = pipeline
            .snapshot()
            .session_end
            .expect("session end must be retained");
        let value = serde_json::to_value(&session_end).expect("session end serializes");
        let stats = value
            .get("final_stats")
            .expect("session end must carry final stats");
        assert_eq!(stats["pipeline_received"], 2);
        assert_eq!(stats["pipeline_consumed"], 2);
        assert_eq!(stats["pipeline_normalized"], 2);
        assert_eq!(stats["pipeline_invalid"], 0);
        assert_eq!(stats["pipeline_dropped_queue_full"], 0);
        assert_eq!(stats["pipeline_dropped_consumer_gone"], 0);
        assert_eq!(stats["pipeline_filtered_ignored"], 0);
        assert_eq!(stats["pipeline_backlog"], 0);
        assert!(stats["events_lost"].is_null());
        assert!(stats["realtime_buffers_lost"].is_null());
    }

    #[test]
    fn session_end_preserves_known_and_unknown_etw_loss_values() {
        let (pipeline, health, _pairing, _records) = recording_pipeline(64);
        health.record_received();
        pipeline.push_process(start(10, filetime(1_000)), Some(filetime(2_000)));
        pipeline.set_final_etw_stats(Some(3), None);
        let outcome = pipeline.close(SessionEndReason::Completed);
        assert!(outcome.drained);

        let session_end = pipeline
            .snapshot()
            .session_end
            .expect("session end must be retained");
        let stats = session_end.final_stats.expect("final stats");
        assert_eq!(stats.events_lost, Some(3));
        assert_eq!(stats.realtime_buffers_lost, None);
    }

    #[test]
    fn normal_start_stop_is_normalized_and_paired() {
        let (pipeline, health, _pairing, records) = recording_pipeline(64);
        let create_time = filetime(1_000);
        health.record_received();
        pipeline.push_process(
            decoded(
                EVENT_PROCESS_START,
                10,
                create_time,
                Some(7),
                Some("cmd.exe"),
            ),
            Some(filetime(2_000)),
        );
        health.record_received();
        pipeline.push_process(
            decoded(
                EVENT_PROCESS_STOP,
                10,
                create_time,
                Some(7),
                Some("cmd.exe"),
            ),
            Some(filetime(3_000)),
        );
        let outcome = pipeline.close(SessionEndReason::Completed);
        assert!(outcome.drained);
        assert!(!outcome.timed_out);

        let snapshot = pipeline.snapshot();
        assert_eq!(snapshot.consumer_state, ConsumerState::Stopped);
        assert_eq!(snapshot.normalized, 2);
        assert_eq!(snapshot.normalized_started, 1);
        assert_eq!(snapshot.normalized_stopped, 1);
        assert_eq!(snapshot.invalid, 0);
        assert_eq!(snapshot.backlog, 0);
        assert!(snapshot.session_end.is_some());
        assert!(snapshot.first_paired_stop.is_some());

        let events: Vec<ProcessEvent> = records_of(&records)
            .into_iter()
            .filter_map(|record| match record {
                PipelineRecord::ProcessEvent(event) => Some(event),
                _ => None,
            })
            .collect();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_type, ProcessEventType::Started);
        assert_eq!(events[0].pairing, PairingStatus::Waiting);
        assert_eq!(events[1].event_type, ProcessEventType::Stopped);
        match &events[1].pairing {
            PairingStatus::Paired {
                start_event_id,
                stop_event_id,
                order,
                sequence_match,
            } => {
                assert_eq!(start_event_id, &events[0].event_id);
                assert_eq!(stop_event_id, &events[1].event_id);
                assert_eq!(*order, PairingOrder::InOrder);
                assert_eq!(*sequence_match, Some(true));
            }
            other => panic!("expected paired stop, got {other:?}"),
        }
        assert_eq!(events[0].session_id, events[1].session_id);
        assert_ne!(events[0].event_id, events[1].event_id);
        assert!(events[0].etw.occurred_at.is_some());
        assert!(events[0].received_at.is_some());
        assert!(events[0].normalized_at.is_some());
        assert!(events[0].process_started_at.is_some());
        assert!(events[0].completeness.identity_certain);
        assert_eq!(events[0].identity.create_time, create_time);
        assert_eq!(events[0].image_name.as_deref(), Some("cmd.exe"));

        let health = health.snapshot();
        assert_eq!(health.events_received, 2);
        assert_eq!(health.events_processed, 2);
        assert_eq!(health.events_dropped, 0);
    }

    #[test]
    fn orphan_stop_is_retained_and_reported() {
        let (pipeline, _health, _pairing, records) = recording_pipeline(64);
        pipeline.push_process(stop(10, 100), None);
        pipeline.close(SessionEndReason::Completed);

        let events: Vec<ProcessEvent> = records_of(&records)
            .into_iter()
            .filter_map(|record| match record {
                PipelineRecord::ProcessEvent(event) => Some(event),
                _ => None,
            })
            .collect();
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].pairing,
            PairingStatus::Unpaired {
                reason: UnpairedReason::StopWithoutObservedStart
            }
        );
        assert!(
            events[0]
                .completeness
                .issues
                .contains(&DataIssue::MissingEventTimestamp)
        );
        assert!(pipeline.snapshot().first_unpaired_event.is_some());
    }

    #[test]
    fn late_start_emits_a_pairing_revision() {
        let (pipeline, _health, _pairing, records) = recording_pipeline(64);
        pipeline.push_process(stop(10, 100), Some(1_000_000));
        pipeline.push_process(start(10, 100), Some(2_000_000));
        pipeline.close(SessionEndReason::Completed);

        let records = records_of(&records);
        let events: Vec<ProcessEvent> = records
            .iter()
            .filter_map(|record| match record {
                PipelineRecord::ProcessEvent(event) => Some(event.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(events.len(), 2);
        match &events[1].pairing {
            PairingStatus::Paired {
                order,
                stop_event_id,
                ..
            } => {
                assert_eq!(*order, PairingOrder::OutOfOrder);
                assert_eq!(stop_event_id, &events[0].event_id);
            }
            other => panic!("expected out-of-order pair, got {other:?}"),
        }
        let revisions: Vec<&PipelineRecord> = records
            .iter()
            .filter(|record| matches!(record, PipelineRecord::PairingRevision(_)))
            .collect();
        assert_eq!(revisions.len(), 1);
        assert_eq!(pipeline.snapshot().pairing_revisions, 1);
    }

    #[test]
    fn invalid_event_is_retained_as_an_anomaly() {
        let (pipeline, health, _pairing, records) = recording_pipeline(64);
        health.record_received();
        pipeline.push_invalid(1, 99, 42, "unsupported version", Some(1_000_000));
        pipeline.close(SessionEndReason::Completed);

        let snapshot = pipeline.snapshot();
        assert_eq!(snapshot.invalid, 1);
        assert_eq!(snapshot.normalized, 0);
        assert!(snapshot.first_invalid_event.is_some());
        let records = records_of(&records);
        assert!(records.iter().any(|record| matches!(
            record,
            PipelineRecord::InvalidEvent(event)
                if event.etw_version == 99 && event.payload_len == 42
        )));

        let health = health.snapshot();
        assert_eq!(health.events_received, 1);
        assert_eq!(health.events_processed, 0);
        assert_eq!(health.events_dropped, 1);
        assert_eq!(health.decode_errors, 1);
        assert_eq!(health.state, crate::health::SensorState::Degraded);
    }

    #[test]
    fn full_queue_drops_without_blocking_the_producer() {
        let health = Arc::new(SensorHealth::new());
        health.mark_starting();
        health.mark_active();
        let pairing = Arc::new(Mutex::new(PairingState::new()));
        let entered = Arc::new(AtomicUsize::new(0));
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let pipeline = EventPipeline::start_with_sink(
            PipelineConfig {
                capacity: 2,
                shutdown_timeout: Duration::from_secs(2),
            },
            Arc::clone(&health),
            Arc::clone(&pairing),
            Box::new(BlockingSink {
                entered: Arc::clone(&entered),
                gate: Arc::clone(&gate),
            }),
        );

        health.record_received();
        pipeline.push_process(start(1, 100), Some(1));
        wait_until(|| entered.load(Ordering::SeqCst) == 1);

        health.record_received();
        pipeline.push_process(start(2, 100), Some(2));
        health.record_received();
        pipeline.push_process(start(3, 100), Some(3));
        health.record_received();
        pipeline.push_process(start(4, 100), Some(4));

        let snapshot = pipeline.snapshot();
        assert_eq!(snapshot.received, 4);
        assert_eq!(snapshot.enqueued, 3);
        assert_eq!(snapshot.dropped_queue_full, 1);
        assert_eq!(snapshot.backlog, 2);
        assert_eq!(snapshot.peak_backlog, 2);
        assert!(snapshot.peak_backlog <= snapshot.capacity as u64);
        assert_eq!(health.snapshot().events_dropped, 1);

        {
            let (lock, condvar) = &*gate;
            *lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = true;
            condvar.notify_all();
        }
        wait_until(|| pipeline.snapshot().normalized == 3);
        let outcome = pipeline.close(SessionEndReason::Completed);
        assert!(outcome.drained);
        assert_eq!(pipeline.snapshot().dropped_queue_full, 1);
        assert_eq!(pipeline.snapshot().backlog, 0);
    }

    #[test]
    fn consumer_exit_is_reported_to_the_producer() {
        let (pipeline, _health, _pairing, _records) = recording_pipeline(64);
        pipeline.stop_consumer_for_test();
        wait_until(|| pipeline.snapshot().consumer_state == ConsumerState::Stopped);

        pipeline.push_process(start(1, 100), Some(1));
        let snapshot = pipeline.snapshot();
        assert_eq!(snapshot.dropped_consumer_gone, 1);
        assert_eq!(snapshot.consumer_state, ConsumerState::Stopped);

        let outcome = pipeline.close(SessionEndReason::Shutdown);
        assert!(outcome.drained);
    }

    #[test]
    fn normal_shutdown_drains_the_queue() {
        let (pipeline, health, _pairing, _records) = recording_pipeline(512);
        for index in 0..200 {
            health.record_received();
            pipeline.push_process(start(index + 1, 100), Some(index as u64 + 1));
        }
        let outcome = pipeline.close(SessionEndReason::Completed);
        assert!(outcome.drained);
        assert!(!outcome.timed_out);

        let snapshot = pipeline.snapshot();
        assert_eq!(snapshot.normalized, 200);
        assert_eq!(snapshot.consumed, 200);
        assert_eq!(snapshot.backlog, 0);
        assert_eq!(snapshot.dropped_total(), 0);
        assert_eq!(snapshot.consumer_state, ConsumerState::Stopped);
        let session_end = snapshot.session_end.expect("session end");
        assert_eq!(session_end.reason, SessionEndReason::Completed);
        assert_eq!(session_end.open_start_events.len(), 32);
        assert_eq!(session_end.pairing.unpaired_starts, 200);
    }

    #[test]
    fn shutdown_timeout_reports_the_backlog_at_timeout() {
        let health = Arc::new(SensorHealth::new());
        let pairing = Arc::new(Mutex::new(PairingState::new()));
        let entered = Arc::new(AtomicUsize::new(0));
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let pipeline = EventPipeline::start_with_sink(
            PipelineConfig {
                capacity: 8,
                shutdown_timeout: Duration::from_millis(50),
            },
            Arc::clone(&health),
            Arc::clone(&pairing),
            Box::new(BlockingSink {
                entered: Arc::clone(&entered),
                gate: Arc::clone(&gate),
            }),
        );

        pipeline.push_process(start(1, 100), Some(1));
        wait_until(|| entered.load(Ordering::SeqCst) == 1);
        pipeline.push_process(start(2, 100), Some(2));

        let outcome = pipeline.close(SessionEndReason::Completed);
        assert!(!outcome.drained);
        assert!(outcome.timed_out);
        assert_eq!(outcome.backlog_at_timeout, Some(1));
        let snapshot = pipeline.snapshot();
        assert!(snapshot.shutdown_timed_out);
        assert_eq!(snapshot.backlog_at_shutdown_timeout, Some(1));
        assert!(pipeline.last_error().is_some());

        {
            let (lock, condvar) = &*gate;
            *lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = true;
            condvar.notify_all();
        }
        wait_until(|| pipeline.snapshot().consumer_state == ConsumerState::Stopped);
    }

    #[test]
    fn sink_errors_do_not_stop_queue_draining() {
        let health = Arc::new(SensorHealth::new());
        let pairing = Arc::new(Mutex::new(PairingState::new()));
        let calls = Arc::new(AtomicUsize::new(0));
        let pipeline = EventPipeline::start_with_sink(
            PipelineConfig::new(64),
            Arc::clone(&health),
            Arc::clone(&pairing),
            Box::new(FailingSink {
                calls: Arc::clone(&calls),
            }),
        );

        pipeline.push_process(start(1, 100), Some(1));
        pipeline.push_process(stop(1, 100), Some(2));
        pipeline.close(SessionEndReason::Completed);

        let snapshot = pipeline.snapshot();
        assert_eq!(snapshot.normalized, 2);
        assert_eq!(snapshot.consumed, 2);
        assert_eq!(snapshot.sink_errors, 1);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(snapshot.consumer_error.is_some());
        assert_eq!(snapshot.consumer_state, ConsumerState::Stopped);
        assert!(pipeline.health_snapshot().sink_errors > 0);
    }

    #[test]
    fn push_after_close_is_counted_as_dropped() {
        let (pipeline, health, _pairing, _records) = recording_pipeline(64);
        pipeline.close(SessionEndReason::Completed);
        health.record_received();
        pipeline.push_process(start(1, 100), Some(1));

        let snapshot = pipeline.snapshot();
        assert_eq!(snapshot.dropped_consumer_gone, 1);
        assert_eq!(health.snapshot().events_dropped, 1);
    }

    #[test]
    fn sessions_do_not_share_event_ids_or_pairing_state() {
        let (first, _health, _pairing, first_records) = recording_pipeline(64);
        first.push_process(start(10, 100), Some(1));
        first.close(SessionEndReason::Completed);

        let (second, _health, _pairing, second_records) = recording_pipeline(64);
        second.push_process(start(10, 100), Some(2));
        second.close(SessionEndReason::Completed);

        let first_events: Vec<ProcessEvent> = records_of(&first_records)
            .into_iter()
            .filter_map(|record| match record {
                PipelineRecord::ProcessEvent(event) => Some(event),
                _ => None,
            })
            .collect();
        let second_events: Vec<ProcessEvent> = records_of(&second_records)
            .into_iter()
            .filter_map(|record| match record {
                PipelineRecord::ProcessEvent(event) => Some(event),
                _ => None,
            })
            .collect();
        assert_eq!(first_events.len(), 1);
        assert_eq!(second_events.len(), 1);
        assert_ne!(first_events[0].session_id, second_events[0].session_id);
        assert_ne!(first_events[0].event_id, second_events[0].event_id);
        assert_eq!(second_events[0].pairing, PairingStatus::Waiting);
    }

    #[test]
    fn decode_chain_reaches_the_normalized_model() {
        let create_time = filetime(5_000);
        let mut payload = Vec::new();
        payload.extend_from_slice(&0x1234u32.to_le_bytes());
        payload.extend_from_slice(&create_time.to_le_bytes());
        payload.extend_from_slice(&0x42u32.to_le_bytes());
        payload.extend_from_slice(&2u32.to_le_bytes());
        payload.extend_from_slice(&0x10u32.to_le_bytes());
        // Start v2 has no SID and no ProcessSequenceNumber; ImageName is UTF-16.
        for unit in r"C:\Windows\System32\cmd.exe".encode_utf16() {
            payload.extend_from_slice(&unit.to_le_bytes());
        }
        payload.extend_from_slice(&0u16.to_le_bytes());
        let decoded = decode_process_event(1, 2, &payload).expect("decode start v2");

        let (pipeline, _health, _pairing, records) = recording_pipeline(64);
        pipeline.push_process(decoded, Some(filetime(6_000)));
        pipeline.close(SessionEndReason::Completed);

        let event = records_of(&records)
            .into_iter()
            .find_map(|record| match record {
                PipelineRecord::ProcessEvent(event) => Some(event),
                _ => None,
            })
            .expect("normalized event");
        assert_eq!(event.event_type, ProcessEventType::Started);
        assert_eq!(event.identity.pid, 0x1234);
        assert_eq!(event.parent_pid, Some(0x42));
        assert_eq!(
            event.image_name.as_deref(),
            Some(r"C:\Windows\System32\cmd.exe")
        );
        assert_eq!(event.etw.event_id, 1);
        assert_eq!(event.etw.version, 2);
        assert!(event.etw.occurred_at.is_some());
        assert!(event.received_at.is_some());
        assert!(event.normalized_at.is_some());
        assert!(event.process_started_at.is_some());
    }

    #[test]
    fn sustained_load_stays_bounded_and_fully_accounted() {
        const EVENTS: usize = 20_000;
        let health = Arc::new(SensorHealth::new());
        health.mark_starting();
        health.mark_active();
        let pairing = Arc::new(Mutex::new(PairingState::new()));
        let pipeline = EventPipeline::start(
            PipelineConfig {
                capacity: 1024,
                shutdown_timeout: Duration::from_secs(2),
            },
            Arc::clone(&health),
            Arc::clone(&pairing),
        );

        for index in 0..EVENTS {
            health.record_received();
            pipeline.push_process(
                decoded(
                    EVENT_PROCESS_START,
                    index as u32 + 1,
                    100,
                    Some(index as u64),
                    Some("cmd.exe"),
                ),
                Some(filetime(index as u64 + 1)),
            );
        }
        let outcome = pipeline.close(SessionEndReason::Completed);
        assert!(outcome.drained);

        let snapshot = pipeline.snapshot();
        assert_eq!(snapshot.received as usize, EVENTS);
        assert_eq!(
            snapshot.normalized as usize + snapshot.dropped_total() as usize,
            EVENTS
        );
        assert_eq!(snapshot.backlog, 0);
        // `backlog` counts the event currently held by the consumer, so its
        // peak can briefly be capacity + 1; the channel itself never exceeds
        // `capacity`.
        assert!(snapshot.peak_backlog <= snapshot.capacity as u64 + 1);
        assert_eq!(health.snapshot().events_dropped, snapshot.dropped_total());
        assert_eq!(snapshot.consumed, snapshot.normalized);
    }

    #[test]
    fn health_accounting_matches_the_pipeline_counters() {
        let (pipeline, health, _pairing, _records) = recording_pipeline(64);
        health.record_received();
        pipeline.push_process(start(1, 100), Some(1));
        health.record_received();
        pipeline.push_process(stop(1, 100), Some(2));
        health.record_received();
        pipeline.push_invalid(1, 99, 1, "bad", Some(3));
        pipeline.close(SessionEndReason::Completed);

        let snapshot = pipeline.snapshot();
        let health = health.snapshot();
        assert_eq!(health.events_received, 3);
        assert_eq!(health.events_processed, snapshot.normalized);
        assert_eq!(
            health.events_dropped,
            snapshot.invalid + snapshot.dropped_total()
        );
        assert_eq!(
            health.events_received,
            health.events_processed + health.events_dropped + health.ignored_events
        );
        assert_eq!(snapshot.consumed, snapshot.normalized + snapshot.invalid);
        assert_eq!(snapshot.backlog, 0);
    }

    #[test]
    fn missing_parent_and_image_are_explicit_without_blocking_normalization() {
        let (pipeline, _health, _pairing, records) = recording_pipeline(64);
        let mut incomplete = decoded(EVENT_PROCESS_START, 5, filetime(1_000), None, None);
        incomplete.parent_pid = None;
        pipeline.push_process(incomplete, Some(filetime(2_000)));
        pipeline.close(SessionEndReason::Completed);

        let event = records_of(&records)
            .into_iter()
            .find_map(|record| match record {
                PipelineRecord::ProcessEvent(event) => Some(event),
                _ => None,
            })
            .expect("normalized event");
        assert_eq!(event.image_name, None);
        assert_eq!(event.completeness.image_name, FieldState::Missing);
        assert!(!event.completeness.parent_pid_available);
        assert!(!event.completeness.etw_sequence_available);
        assert!(
            event
                .completeness
                .issues
                .contains(&DataIssue::MissingImageName)
        );
        assert!(
            event
                .completeness
                .issues
                .contains(&DataIssue::MissingParentPid)
        );
        assert_eq!(event.pairing, PairingStatus::Waiting);
        assert!(event.completeness.identity_certain);
    }

    #[test]
    fn truncated_image_names_are_explicit_and_bounded() {
        let (pipeline, _health, _pairing, records) = recording_pipeline(64);
        let image = format!(r"C:\Temp\{}", "x".repeat(MAX_IMAGE_NAME_CHARS + 50));
        pipeline.push_process(
            decoded(EVENT_PROCESS_START, 1, 100, Some(1), Some(&image)),
            Some(1),
        );
        pipeline.close(SessionEndReason::Completed);

        let event = records_of(&records)
            .into_iter()
            .find_map(|record| match record {
                PipelineRecord::ProcessEvent(event) => Some(event),
                _ => None,
            })
            .expect("normalized event");
        assert_eq!(
            event.image_name.as_deref().unwrap().chars().count(),
            MAX_IMAGE_NAME_CHARS
        );
        assert_eq!(event.completeness.image_name, FieldState::Partial);
        assert!(
            event
                .completeness
                .issues
                .contains(&DataIssue::ImageNameTruncated)
        );
    }
}
