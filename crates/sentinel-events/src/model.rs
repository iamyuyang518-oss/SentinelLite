//! Standardized process event model.
//!
//! The model deliberately keeps the three clocks separate:
//!
//! - `etw.occurred_at`: the timestamp carried by the ETW record.
//! - `received_at`: when the sensor callback copied the record into the
//!   bounded pipeline.
//! - `normalized_at`: when the pipeline consumer produced this model.
//!
//! Optional fields stay `None` (and are listed in `completeness.issues`);
//! nothing here invents a default for missing ETW data.

use serde::{Deserialize, Serialize};

pub const MAX_IMAGE_NAME_CHARS: usize = 1024;
pub const MAX_FIELD_ISSUES: usize = 8;
pub const MAX_SESSION_END_START_SAMPLES: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldState {
    Present,
    Partial,
    Missing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessEventType {
    Started,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventSource {
    MicrosoftWindowsKernelProcess,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessIdentity {
    pub pid: u32,
    /// FILETIME identity token from the ETW CreateTime field. A value of zero
    /// is not a usable identity and is reported as indeterminate.
    pub create_time: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityCertainty {
    Exact,
    Indeterminate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataIssue {
    MissingParentPid,
    MissingImageName,
    PartialImageName,
    ImageNameTruncated,
    MissingEventTimestamp,
    EventTimestampUnrepresentable,
    MissingReceiveTime,
    MissingNormalizeTime,
    IndeterminateProcessCreateTime,
    ProcessStartTimeUnrepresentable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataCompleteness {
    pub identity_certain: bool,
    pub event_time_known: bool,
    pub received_time_known: bool,
    pub normalized_time_known: bool,
    pub parent_pid_available: bool,
    pub image_name: FieldState,
    pub etw_sequence_available: bool,
    pub issues: Vec<DataIssue>,
}

impl DataCompleteness {
    pub fn push_issue(&mut self, issue: DataIssue) {
        if self.issues.len() < MAX_FIELD_ISSUES && !self.issues.contains(&issue) {
            self.issues.push(issue);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EtwEventInfo {
    pub event_id: u16,
    pub version: u8,
    pub provider_sequence: Option<u64>,
    pub timestamp_100ns: Option<u64>,
    pub occurred_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PairingOrder {
    InOrder,
    OutOfOrder,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum UnpairedReason {
    StopWithoutObservedStart,
    OrphanStopNotRetained,
    DuplicateStart { first_event_id: String },
    DuplicateStop { first_event_id: String },
    IdentityMismatch { open_create_time: u64 },
    StartNotRetained,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndeterminateReason {
    MissingProcessCreateTime,
    UnsupportedEventType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PairingStatus {
    Paired {
        start_event_id: String,
        stop_event_id: String,
        order: PairingOrder,
        sequence_match: Option<bool>,
    },
    Waiting,
    Unpaired {
        reason: UnpairedReason,
    },
    Indeterminate {
        reason: IndeterminateReason,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessEvent {
    pub event_id: String,
    pub sequence: u64,
    pub session_id: String,
    pub event_type: ProcessEventType,
    pub source: EventSource,
    pub identity: ProcessIdentity,
    pub identity_certainty: IdentityCertainty,
    pub parent_pid: Option<u32>,
    pub image_name: Option<String>,
    pub etw: EtwEventInfo,
    pub process_started_at: Option<String>,
    pub received_at: Option<String>,
    pub normalized_at: Option<String>,
    pub completeness: DataCompleteness,
    pub pairing: PairingStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvalidProcessEvent {
    pub event_id: String,
    pub sequence: u64,
    pub session_id: String,
    pub etw_event_id: u16,
    pub etw_version: u8,
    pub payload_len: usize,
    pub error: String,
    pub etw_timestamp_100ns: Option<u64>,
    pub occurred_at: Option<String>,
    pub received_at: Option<String>,
    pub normalized_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingRevision {
    pub session_id: String,
    pub stop_event_id: String,
    pub start_event_id: String,
    pub sequence_match: Option<bool>,
    pub reason: String,
    pub normalized_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionEndReason {
    Completed,
    Interrupted,
    Error,
    Shutdown,
}

/// Final pairing coverage for a session. This is retained in `SessionEnd` and
/// the run summary. Counter fields only; no raw ETW payloads are copied here.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PairingSnapshot {
    pub paired: u64,
    pub unpaired_starts: u64,
    pub orphan_stops: u64,
    pub duplicate_starts: u64,
    pub duplicate_stops: u64,
    pub pid_reuse_detected: u64,
    pub sequence_mismatches: u64,
    pub start_overflow: u64,
    pub orphan_stop_overflow: u64,
    pub identity_mismatched_stops: u64,
    pub out_of_order_pairs: u64,
    pub indeterminate_identities: u64,
    pub closed_identity_evictions: u64,
    pub pending_orphan_stops: u64,
    pub starts_seen: u64,
    pub stops_seen: u64,
    pub observed_start_image_names: Vec<String>,
    pub observed_start_image_names_truncated: bool,
}

impl PairingSnapshot {
    /// Anomalies that mean the session data itself is not fully trustworthy.
    /// Orphan stops, identity-mismatched stops and starts still open at the
    /// end of a window are normal lifecycle boundaries, not integrity errors.
    pub fn has_integrity_issue(&self) -> bool {
        self.duplicate_starts > 0
            || self.duplicate_stops > 0
            || self.pid_reuse_detected > 0
            || self.sequence_mismatches > 0
            || self.start_overflow > 0
            || self.orphan_stop_overflow > 0
            || self.out_of_order_pairs > 0
            || self.indeterminate_identities > 0
            || self.closed_identity_evictions > 0
    }

    /// A window with no ProcessStart does not prove start coverage even if it
    /// contains ProcessStop events.
    pub fn coverage_issue(&self) -> Option<&'static str> {
        (self.starts_seen == 0).then_some("no process start events observed")
    }
}

/// Final statistics captured while the session is closing.
///
/// The whole block stays absent for older stored sessions. Inside the block,
/// `events_lost` and `realtime_buffers_lost` stay `None` when the ETW stop
/// query did not produce a final value. A missing value is never encoded as
/// zero. Pipeline counters are internal sensor counters and are always known
/// when this block is present.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionEndStats {
    pub events_lost: Option<u64>,
    pub realtime_buffers_lost: Option<u64>,
    pub pipeline_capacity: u64,
    pub pipeline_received: u64,
    pub pipeline_enqueued: u64,
    pub pipeline_consumed: u64,
    pub pipeline_normalized: u64,
    pub pipeline_invalid: u64,
    pub pipeline_dropped_queue_full: u64,
    pub pipeline_dropped_consumer_gone: u64,
    pub pipeline_filtered_ignored: u64,
    pub pipeline_backlog: u64,
    pub pipeline_peak_backlog: u64,
    pub pipeline_backlog_at_shutdown_timeout: Option<u64>,
    pub pipeline_shutdown_timed_out: bool,
    pub pipeline_sink_errors: u64,
}

impl SessionEndStats {
    pub fn pipeline_dropped_total(&self) -> u64 {
        self.pipeline_dropped_queue_full
            .saturating_add(self.pipeline_dropped_consumer_gone)
    }

    pub fn has_pipeline_gap(&self) -> bool {
        self.pipeline_dropped_total() > 0
            || self.pipeline_backlog > 0
            || self.pipeline_shutdown_timed_out
            || self.pipeline_sink_errors > 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionEnd {
    pub session_id: String,
    pub ended_at: Option<String>,
    pub reason: SessionEndReason,
    pub error: Option<String>,
    pub open_start_events: Vec<String>,
    pub pairing: PairingSnapshot,
    /// Final ETW and pipeline statistics. `None` means an older record or a
    /// close that never produced final statistics; it is not zero.
    #[serde(default)]
    pub final_stats: Option<SessionEndStats>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "record", rename_all = "snake_case")]
pub enum PipelineRecord {
    ProcessEvent(ProcessEvent),
    InvalidEvent(InvalidProcessEvent),
    PairingRevision(PairingRevision),
    SessionEnd(SessionEnd),
}

pub fn format_event_id(session_id: &str, sequence: u64) -> String {
    format!("{session_id}:{sequence:016x}")
}

/// Generate a session identity that changes when the sensor process restarts.
/// The timestamp is part of the identity so a reused PID cannot make two runs
/// look like the same acquisition session.
pub fn generate_session_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("{:x}-{:x}", std::process::id(), nanos)
}

pub fn truncate_image_name(value: &mut String) -> bool {
    let Some((index, _)) = value.char_indices().nth(MAX_IMAGE_NAME_CHARS) else {
        return false;
    };
    value.truncate(index);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_ids_are_session_scoped_and_ordered() {
        let first = format_event_id("session-a", 1);
        let second = format_event_id("session-a", 2);
        let other = format_event_id("session-b", 1);

        assert_ne!(first, second);
        assert_ne!(first, other);
        assert!(first.ends_with("0000000000000001"));
        assert!(second.ends_with("0000000000000002"));
    }

    #[test]
    fn session_ids_distinguish_restarts() {
        let first = generate_session_id();
        let second = generate_session_id();
        assert_ne!(first, second);
    }

    #[test]
    fn image_names_are_truncated_on_a_character_boundary() {
        let mut value = "测".repeat(MAX_IMAGE_NAME_CHARS + 5);
        assert!(truncate_image_name(&mut value));
        assert_eq!(value.chars().count(), MAX_IMAGE_NAME_CHARS);

        let mut short = "cmd.exe".to_owned();
        assert!(!truncate_image_name(&mut short));
        assert_eq!(short, "cmd.exe");
    }

    #[test]
    fn completeness_issues_are_bounded_and_deduplicated() {
        let mut completeness = DataCompleteness {
            identity_certain: false,
            event_time_known: false,
            received_time_known: false,
            normalized_time_known: false,
            parent_pid_available: false,
            image_name: FieldState::Missing,
            etw_sequence_available: false,
            issues: Vec::new(),
        };
        for _ in 0..MAX_FIELD_ISSUES * 2 {
            completeness.push_issue(DataIssue::MissingImageName);
            completeness.push_issue(DataIssue::MissingEventTimestamp);
        }
        assert_eq!(completeness.issues.len(), 2);
    }

    #[test]
    fn pipeline_records_round_trip_through_json() {
        let event = ProcessEvent {
            event_id: "session:0000000000000001".to_owned(),
            sequence: 1,
            session_id: "session".to_owned(),
            event_type: ProcessEventType::Started,
            source: EventSource::MicrosoftWindowsKernelProcess,
            identity: ProcessIdentity {
                pid: 42,
                create_time: 133_000_000_000_000_000,
            },
            identity_certainty: IdentityCertainty::Exact,
            parent_pid: Some(4),
            image_name: Some(r"C:\Windows\System32\notepad.exe".to_owned()),
            etw: EtwEventInfo {
                event_id: 1,
                version: 4,
                provider_sequence: Some(7),
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
            pairing: PairingStatus::Waiting,
        };
        let record = PipelineRecord::ProcessEvent(event);
        let json = serde_json::to_string(&record).expect("record should serialize");
        let decoded: PipelineRecord =
            serde_json::from_str(&json).expect("record should deserialize");
        assert_eq!(decoded, record);
    }

    #[test]
    fn session_end_final_stats_round_trip_and_old_records_stay_unknown() {
        let stats = SessionEndStats {
            events_lost: Some(2),
            realtime_buffers_lost: None,
            pipeline_capacity: 4096,
            pipeline_received: 10,
            pipeline_enqueued: 9,
            pipeline_consumed: 9,
            pipeline_normalized: 8,
            pipeline_invalid: 1,
            pipeline_dropped_queue_full: 0,
            pipeline_dropped_consumer_gone: 0,
            pipeline_filtered_ignored: 1,
            pipeline_backlog: 0,
            pipeline_peak_backlog: 3,
            pipeline_backlog_at_shutdown_timeout: None,
            pipeline_shutdown_timed_out: false,
            pipeline_sink_errors: 0,
        };
        let end = SessionEnd {
            session_id: "session".to_owned(),
            ended_at: None,
            reason: SessionEndReason::Completed,
            error: None,
            open_start_events: Vec::new(),
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
                observed_start_image_names: Vec::new(),
                observed_start_image_names_truncated: false,
            },
            final_stats: Some(stats.clone()),
        };
        let json = serde_json::to_string(&end).expect("session end serializes");
        let decoded: SessionEnd = serde_json::from_str(&json).expect("session end decodes");
        assert_eq!(decoded.final_stats, Some(stats));
        assert_eq!(
            decoded
                .final_stats
                .as_ref()
                .map(SessionEndStats::pipeline_dropped_total),
            Some(0)
        );

        let old = serde_json::json!({
            "session_id": "old-session",
            "ended_at": null,
            "reason": "completed",
            "error": null,
            "open_start_events": [],
            "pairing": {
                "paired": 0, "unpaired_starts": 0, "orphan_stops": 0,
                "duplicate_starts": 0, "duplicate_stops": 0,
                "pid_reuse_detected": 0, "sequence_mismatches": 0,
                "start_overflow": 0, "orphan_stop_overflow": 0,
                "identity_mismatched_stops": 0, "out_of_order_pairs": 0,
                "indeterminate_identities": 0, "closed_identity_evictions": 0,
                "pending_orphan_stops": 0, "starts_seen": 0, "stops_seen": 0,
                "observed_start_image_names": [],
                "observed_start_image_names_truncated": false
            }
        });
        let old: SessionEnd = serde_json::from_value(old).expect("old session end decodes");
        assert_eq!(old.final_stats, None);
    }

    #[test]
    fn pairing_snapshot_round_trips_through_json() {
        let snapshot = PairingSnapshot {
            paired: 1,
            unpaired_starts: 0,
            orphan_stops: 1,
            duplicate_starts: 0,
            duplicate_stops: 0,
            pid_reuse_detected: 0,
            sequence_mismatches: 0,
            start_overflow: 0,
            orphan_stop_overflow: 0,
            identity_mismatched_stops: 0,
            out_of_order_pairs: 1,
            indeterminate_identities: 0,
            closed_identity_evictions: 0,
            pending_orphan_stops: 0,
            starts_seen: 1,
            stops_seen: 1,
            observed_start_image_names: vec!["notepad.exe".to_owned()],
            observed_start_image_names_truncated: false,
        };
        let json = serde_json::to_string(&snapshot).expect("snapshot should serialize");
        let decoded: PairingSnapshot =
            serde_json::from_str(&json).expect("snapshot should deserialize");
        assert_eq!(decoded, snapshot);
    }
}
