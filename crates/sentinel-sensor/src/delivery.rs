//! Bounded, cursor-based delivery window for normalized pipeline records.
//!
//! The pipeline consumer appends records here. The event IPC server reads
//! bounded batches from this window. The window is independent from the ETW
//! intake queue: if no application is reading, old delivery records are
//! evicted with an explicit count, but ETW collection and health accounting
//! continue normally. A client whose cursor fell behind receives
//! `cursor_expired` and `missed_records`; it never receives a silent gap.
//!
//! The final `SessionEnd` is appended once, and after that no more records are
//! produced. The sensor keeps the event endpoint alive for a bounded grace
//! period so a polling client can read the tail.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

use sentinel_events::PipelineRecord;
use sentinel_ipc::event_server::EventProvider;
use sentinel_ipc::events::{
    EventBatch, EventEnvelope, MAX_EVENT_BATCH_LIMIT, MIN_EVENT_BATCH_LIMIT,
};

use crate::pipeline::EventSink;

pub const DEFAULT_DELIVERY_CAPACITY: usize = 4096;
pub const MIN_DELIVERY_CAPACITY: usize = 64;
pub const MAX_DELIVERY_CAPACITY: usize = 65_536;
pub const SESSION_END_DELIVERY_GRACE: Duration = Duration::from_secs(2);
/// After any client confirms `SessionEnd`, keep the endpoint alive briefly so
/// a second consumer (for example the UI and the persistence worker) can read
/// the tail before the sensor exits.
pub const SESSION_END_DRAIN_LINGER: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy)]
pub struct DeliveryConfig {
    pub capacity: usize,
}

impl DeliveryConfig {
    pub fn new(capacity: usize) -> Self {
        Self { capacity }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.capacity < MIN_DELIVERY_CAPACITY {
            return Err(format!(
                "delivery buffer must be at least {MIN_DELIVERY_CAPACITY} records"
            ));
        }
        if self.capacity > MAX_DELIVERY_CAPACITY {
            return Err(format!(
                "delivery buffer must be at most {MAX_DELIVERY_CAPACITY} records"
            ));
        }
        Ok(())
    }
}

impl Default for DeliveryConfig {
    fn default() -> Self {
        Self {
            capacity: DEFAULT_DELIVERY_CAPACITY,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DeliverySnapshot {
    pub session_id: String,
    pub capacity: usize,
    pub buffered: usize,
    pub oldest_cursor: u64,
    pub latest_cursor: u64,
    pub evictions: u64,
    pub batches_served: u64,
    pub session_closed: bool,
    pub session_end_delivered: bool,
}

struct BrokerState {
    session_id: String,
    records: VecDeque<EventEnvelope>,
    next_cursor: u64,
    evictions: u64,
    batches_served: u64,
    closed: bool,
    session_end_cursor: Option<u64>,
    session_end_delivered: bool,
}

pub struct EventBroker {
    capacity: usize,
    state: Mutex<BrokerState>,
    changed: Condvar,
}

impl EventBroker {
    pub fn new(session_id: impl Into<String>, capacity: usize) -> Arc<Self> {
        let capacity = capacity.clamp(MIN_DELIVERY_CAPACITY, MAX_DELIVERY_CAPACITY);
        Arc::new(Self {
            capacity,
            state: Mutex::new(BrokerState {
                session_id: session_id.into(),
                records: VecDeque::with_capacity(capacity),
                next_cursor: 0,
                evictions: 0,
                batches_served: 0,
                closed: false,
                session_end_cursor: None,
                session_end_delivered: false,
            }),
            changed: Condvar::new(),
        })
    }

    pub fn session_id(&self) -> String {
        self.lock().session_id.clone()
    }

    pub fn append(&self, record: &PipelineRecord) {
        let mut state = self.lock();
        state.next_cursor = state.next_cursor.saturating_add(1);
        let cursor = state.next_cursor;
        if matches!(record, PipelineRecord::SessionEnd(_)) {
            state.closed = true;
            state.session_end_cursor = Some(cursor);
        }
        state.records.push_back(EventEnvelope {
            cursor,
            record: record.clone(),
        });
        while state.records.len() > self.capacity {
            state.records.pop_front();
            state.evictions = state.evictions.saturating_add(1);
        }
        if state.closed {
            self.changed.notify_all();
        }
    }

    pub fn acknowledge(&self, cursor: u64) {
        let mut state = self.lock();
        if state
            .session_end_cursor
            .is_some_and(|session_end| cursor >= session_end)
        {
            state.session_end_delivered = true;
            self.changed.notify_all();
        }
    }

    pub fn wait_for_session_end_delivery(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut state = self.lock();
        loop {
            if state.session_end_delivered {
                return true;
            }
            if !state.closed {
                return false;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return state.session_end_delivered;
            }
            let (next, wait) = self
                .changed
                .wait_timeout(state, remaining)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state = next;
            if wait.timed_out() && !state.session_end_delivered {
                return false;
            }
        }
    }

    pub fn snapshot(&self) -> DeliverySnapshot {
        let state = self.lock();
        DeliverySnapshot {
            session_id: state.session_id.clone(),
            capacity: self.capacity,
            buffered: state.records.len(),
            oldest_cursor: state.records.front().map_or(0, |record| record.cursor),
            latest_cursor: state.next_cursor,
            evictions: state.evictions,
            batches_served: state.batches_served,
            session_closed: state.closed,
            session_end_delivered: state.session_end_delivered,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BrokerState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn batch_from_state(state: &BrokerState, after: u64, limit: u16) -> EventBatch {
        let limit = usize::from(limit.clamp(MIN_EVENT_BATCH_LIMIT, MAX_EVENT_BATCH_LIMIT));
        let oldest = state.records.front().map_or(0, |record| record.cursor);
        let latest = state.next_cursor;
        let cursor_ahead = after > latest;
        let cursor_expired = cursor_expired_for(after, oldest, latest);
        let missed_records = if cursor_expired {
            oldest.saturating_sub(1).saturating_sub(after)
        } else {
            0
        };

        let mut records = Vec::new();
        if !cursor_ahead {
            for record in state.records.iter() {
                if record.cursor <= after {
                    continue;
                }
                if records.len() == limit {
                    break;
                }
                records.push(record.clone());
            }
        }

        let next_cursor = records.last().map_or_else(
            || {
                if cursor_ahead { latest } else { after }
            },
            |record| record.cursor,
        );
        EventBatch {
            session_id: state.session_id.clone(),
            oldest_cursor: oldest,
            latest_cursor: latest,
            next_cursor,
            cursor_expired,
            missed_records,
            cursor_ahead,
            evictions: state.evictions,
            session_closed: state.closed,
            has_more: latest > next_cursor,
            records,
        }
    }
}

fn cursor_expired_for(after: u64, oldest: u64, latest: u64) -> bool {
    latest > 0 && oldest > 0 && after < oldest.saturating_sub(1)
}

impl EventProvider for EventBroker {
    fn batch(&self, after: u64, limit: u16) -> Result<EventBatch, String> {
        let mut state = self.lock();
        state.batches_served = state.batches_served.saturating_add(1);
        Ok(Self::batch_from_state(&state, after, limit))
    }

    fn acknowledge(&self, cursor: u64) {
        EventBroker::acknowledge(self, cursor);
    }
}

pub struct BrokerSink {
    broker: Arc<EventBroker>,
}

impl BrokerSink {
    pub fn new(broker: Arc<EventBroker>) -> Self {
        Self { broker }
    }
}

impl EventSink for BrokerSink {
    fn emit(&mut self, record: &PipelineRecord) -> Result<(), String> {
        self.broker.append(record);
        Ok(())
    }
}

/// A synthetic test record used only to prove the broker's bounds without
/// constructing a full process event.
#[cfg(test)]
pub(crate) fn synthetic_record(session_id: &str, sequence: u64) -> PipelineRecord {
    use crate::event::EtwEventInfo;
    use sentinel_events::format_event_id;
    use sentinel_events::{
        DataCompleteness, EventSource, FieldState, IdentityCertainty, PairingStatus, ProcessEvent,
        ProcessEventType, ProcessIdentity,
    };
    PipelineRecord::ProcessEvent(ProcessEvent {
        event_id: format_event_id(session_id, sequence),
        sequence,
        session_id: session_id.to_owned(),
        event_type: ProcessEventType::Started,
        source: EventSource::MicrosoftWindowsKernelProcess,
        identity: ProcessIdentity {
            pid: sequence as u32,
            create_time: 1,
        },
        identity_certainty: IdentityCertainty::Exact,
        parent_pid: None,
        image_name: Some("test.exe".to_owned()),
        etw: EtwEventInfo {
            event_id: 1,
            version: 4,
            provider_sequence: Some(sequence),
            timestamp_100ns: None,
            occurred_at: None,
        },
        process_started_at: None,
        received_at: None,
        normalized_at: None,
        completeness: DataCompleteness {
            identity_certain: true,
            event_time_known: false,
            received_time_known: false,
            normalized_time_known: false,
            parent_pid_available: false,
            image_name: FieldState::Present,
            etw_sequence_available: true,
            issues: Vec::new(),
        },
        pairing: PairingStatus::Waiting,
    })
}

#[cfg(test)]
pub(crate) fn synthetic_session_end(session_id: &str) -> PipelineRecord {
    use sentinel_events::{PairingSnapshot, SessionEnd, SessionEndReason};
    PipelineRecord::SessionEnd(SessionEnd {
        session_id: session_id.to_owned(),
        ended_at: None,
        reason: SessionEndReason::Completed,
        error: None,
        open_start_events: Vec::new(),
        final_stats: None,
        pairing: PairingSnapshot {
            paired: 0,
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
            starts_seen: 0,
            stops_seen: 0,
            observed_start_image_names: Vec::new(),
            observed_start_image_names_truncated: false,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn broker() -> Arc<EventBroker> {
        EventBroker::new("session", 64)
    }

    #[test]
    fn batches_are_cursor_ordered_and_idempotent() {
        let broker = broker();
        for sequence in 1..=3 {
            broker.append(&synthetic_record("session", sequence));
        }
        let first = broker.batch(0, 2).unwrap();
        assert_eq!(first.records.len(), 2);
        assert_eq!(first.records[0].cursor, 1);
        assert!(first.has_more);
        assert_eq!(first.next_cursor, 2);

        let repeat = broker.batch(0, 2).unwrap();
        assert_eq!(repeat.records, first.records);

        let second = broker.batch(first.next_cursor, 2).unwrap();
        assert_eq!(second.records.len(), 1);
        assert_eq!(second.records[0].cursor, 3);
        assert!(!second.has_more);
        assert_eq!(second.next_cursor, 3);
    }

    #[test]
    fn evictions_are_explicit_for_a_stale_cursor() {
        let broker = broker();
        for sequence in 1..=70 {
            broker.append(&synthetic_record("session", sequence));
        }
        let batch = broker.batch(0, 128).unwrap();
        assert!(batch.cursor_expired);
        assert_eq!(batch.oldest_cursor, 7);
        assert_eq!(batch.missed_records, 6);
        assert_eq!(batch.records.len(), 64);
        assert_eq!(batch.evictions, 6);
    }

    #[test]
    fn a_cursor_ahead_resets_without_fabricating_records() {
        let broker = broker();
        broker.append(&synthetic_record("session", 1));
        let batch = broker.batch(99, 10).unwrap();
        assert!(batch.cursor_ahead);
        assert!(batch.records.is_empty());
        assert_eq!(batch.next_cursor, 1);
        assert!(!batch.has_more);
    }

    #[test]
    fn session_end_is_retained_and_acknowledged() {
        let broker = broker();
        broker.append(&synthetic_record("session", 1));
        broker.append(&synthetic_session_end("session"));
        let batch = broker.batch(0, 10).unwrap();
        assert!(batch.session_closed);
        assert_eq!(batch.records.len(), 2);
        assert!(matches!(
            batch.records[1].record,
            PipelineRecord::SessionEnd(_)
        ));
        broker.acknowledge(batch.next_cursor);
        assert!(broker.wait_for_session_end_delivery(Duration::from_millis(10)));
        assert!(broker.snapshot().session_end_delivered);
    }

    #[test]
    fn session_end_wait_is_bounded_without_a_reader() {
        let broker = broker();
        broker.append(&synthetic_session_end("session"));
        assert!(!broker.wait_for_session_end_delivery(Duration::from_millis(20)));
    }

    #[test]
    fn eviction_never_exceeds_the_configured_capacity() {
        let broker = broker();
        for sequence in 1..=100 {
            broker.append(&synthetic_record("session", sequence));
        }
        let snapshot = broker.snapshot();
        assert_eq!(snapshot.buffered, 64);
        assert_eq!(snapshot.evictions, 36);
        assert_eq!(snapshot.latest_cursor, 100);
        assert_eq!(snapshot.oldest_cursor, 37);
    }
}
