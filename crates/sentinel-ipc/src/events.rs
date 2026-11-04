//! Versioned, bounded read-only process-event delivery protocol.
//!
//! Delivery is cursor based. Every record in the sensor's bounded delivery
//! window receives a monotonically increasing cursor. A client asks for
//! records after its last cursor and receives a batch. Repeating a request
//! while the window still holds the records is idempotent; when the window has
//! moved past the requested cursor the response explicitly says so instead of
//! silently returning a gap.
//!
//! This protocol is intentionally separate from the status protocol so a slow
//! event reader cannot delay health queries.

use serde::{Deserialize, Serialize};

use sentinel_events::PipelineRecord;

use crate::protocol::{MAX_ERROR_CHARS, ProtocolError, ProtocolErrorCode};

pub const EVENT_PROTOCOL_VERSION: u16 = 1;
pub const MAX_EVENT_REQUEST_BYTES: usize = 1024;
pub const MAX_EVENT_RESPONSE_BYTES: usize = 256 * 1024;
pub const MIN_EVENT_BATCH_LIMIT: u16 = 1;
pub const MAX_EVENT_BATCH_LIMIT: u16 = 128;
pub const DEFAULT_EVENT_BATCH_LIMIT: u16 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventRequestKind {
    Events,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventRequest {
    pub version: u16,
    pub request: EventRequestKind,
    /// Return records with a cursor strictly greater than this value.
    pub after: u64,
    pub limit: u16,
}

impl EventRequest {
    pub fn new(after: u64, limit: u16) -> Self {
        Self {
            version: EVENT_PROTOCOL_VERSION,
            request: EventRequestKind::Events,
            after,
            limit,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub cursor: u64,
    pub record: PipelineRecord,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventBatch {
    pub session_id: String,
    /// Cursor of the oldest record still retained, or 0 when the window is
    /// empty.
    pub oldest_cursor: u64,
    /// Cursor of the newest record appended, or 0 when the window is empty.
    pub latest_cursor: u64,
    /// Cursor the client should send as `after` on its next request.
    pub next_cursor: u64,
    /// The requested cursor is older than the retained window; records were
    /// evicted before this read.
    pub cursor_expired: bool,
    /// Number of cursors skipped because of eviction for this request. Zero
    /// unless `cursor_expired` is true.
    pub missed_records: u64,
    /// The client supplied a cursor ahead of this session. This normally means
    /// the client is reusing a cursor from a previous sensor session.
    pub cursor_ahead: bool,
    /// Total records evicted from the bounded delivery window in this session.
    pub evictions: u64,
    /// The pipeline has ended and `SessionEnd` has been appended (when it was
    /// not itself evicted, which cannot happen after the pipeline stops).
    pub session_closed: bool,
    /// More retained records exist after `next_cursor`.
    pub has_more: bool,
    pub records: Vec<EventEnvelope>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventResponse {
    pub version: u16,
    pub ok: bool,
    pub batch: Option<EventBatch>,
    pub error: Option<ProtocolError>,
}

impl EventResponse {
    pub fn success(batch: EventBatch) -> Self {
        Self {
            version: EVENT_PROTOCOL_VERSION,
            ok: true,
            batch: Some(batch),
            error: None,
        }
    }

    pub fn failure(code: ProtocolErrorCode, message: impl Into<String>) -> Self {
        let mut message: String = message.into().chars().take(MAX_ERROR_CHARS).collect();
        if message.is_empty() {
            message = "event delivery failed".to_owned();
        }
        Self {
            version: EVENT_PROTOCOL_VERSION,
            ok: false,
            batch: None,
            error: Some(ProtocolError::new(code, message)),
        }
    }
}

pub fn encode_event_request(request: &EventRequest) -> Result<Vec<u8>, ProtocolError> {
    validate_event_request(request)?;
    serde_json::to_vec(request)
        .map_err(|error| ProtocolError::new(ProtocolErrorCode::InvalidRequest, error.to_string()))
}

pub fn decode_event_request(bytes: &[u8]) -> Result<EventRequest, ProtocolError> {
    if bytes.len() > MAX_EVENT_REQUEST_BYTES {
        return Err(ProtocolError::new(
            ProtocolErrorCode::RequestTooLarge,
            "event request exceeds limit",
        ));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| {
        ProtocolError::new(
            ProtocolErrorCode::InvalidRequest,
            "event request is not valid UTF-8",
        )
    })?;
    let request: EventRequest = serde_json::from_str(text).map_err(|_| {
        ProtocolError::new(
            ProtocolErrorCode::InvalidRequest,
            "event request is not valid protocol JSON",
        )
    })?;
    validate_event_request(&request)?;
    Ok(request)
}

fn validate_event_request(request: &EventRequest) -> Result<(), ProtocolError> {
    if request.version != EVENT_PROTOCOL_VERSION {
        return Err(ProtocolError::new(
            ProtocolErrorCode::UnsupportedVersion,
            "unsupported event protocol version",
        ));
    }
    if request.limit < MIN_EVENT_BATCH_LIMIT || request.limit > MAX_EVENT_BATCH_LIMIT {
        return Err(ProtocolError::new(
            ProtocolErrorCode::InvalidRequest,
            format!(
                "event batch limit must be between {MIN_EVENT_BATCH_LIMIT} and \
                 {MAX_EVENT_BATCH_LIMIT}"
            ),
        ));
    }
    Ok(())
}

pub fn encode_event_response(response: &EventResponse) -> Result<Vec<u8>, ProtocolError> {
    if response.version != EVENT_PROTOCOL_VERSION {
        return Err(ProtocolError::new(
            ProtocolErrorCode::UnsupportedVersion,
            "unsupported event protocol version",
        ));
    }
    validate_event_response_shape(response)?;
    let bytes = serde_json::to_vec(response)
        .map_err(|error| ProtocolError::new(ProtocolErrorCode::Internal, error.to_string()))?;
    if bytes.len() > MAX_EVENT_RESPONSE_BYTES {
        return Err(ProtocolError::new(
            ProtocolErrorCode::ResponseTooLarge,
            "event response exceeds limit",
        ));
    }
    Ok(bytes)
}

/// Encode a batch response, dropping trailing records if the JSON would exceed
/// the response limit. A single oversized record is still reported as
/// `ResponseTooLarge`; records themselves are bounded by the event model.
pub fn encode_event_batch(batch: EventBatch) -> Result<Vec<u8>, ProtocolError> {
    let mut batch = batch;
    loop {
        let response = EventResponse::success(batch.clone());
        match encode_event_response(&response) {
            Ok(bytes) => return Ok(bytes),
            Err(error) if error.code == ProtocolErrorCode::ResponseTooLarge => {
                if batch.records.len() <= 1 {
                    return Err(error);
                }
                let keep = batch.records.len() / 2;
                batch.records.truncate(keep);
                recompute_batch_tail(&mut batch);
            }
            Err(error) => return Err(error),
        }
    }
}

fn recompute_batch_tail(batch: &mut EventBatch) {
    let last = batch.records.last().map(|envelope| envelope.cursor);
    if let Some(cursor) = last {
        batch.next_cursor = cursor;
    }
    batch.has_more = batch.latest_cursor > batch.next_cursor;
}

pub fn decode_event_response(bytes: &[u8]) -> Result<EventResponse, ProtocolError> {
    if bytes.len() > MAX_EVENT_RESPONSE_BYTES {
        return Err(ProtocolError::new(
            ProtocolErrorCode::ResponseTooLarge,
            "event response exceeds limit",
        ));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| {
        ProtocolError::new(
            ProtocolErrorCode::InvalidRequest,
            "event response is not valid UTF-8",
        )
    })?;
    let response: EventResponse = serde_json::from_str(text).map_err(|_| {
        ProtocolError::new(
            ProtocolErrorCode::InvalidRequest,
            "event response is not valid protocol JSON",
        )
    })?;
    if response.version != EVENT_PROTOCOL_VERSION {
        return Err(ProtocolError::new(
            ProtocolErrorCode::UnsupportedVersion,
            "unsupported event protocol version",
        ));
    }
    validate_event_response_shape(&response)?;
    Ok(response)
}

fn validate_event_response_shape(response: &EventResponse) -> Result<(), ProtocolError> {
    let valid = matches!(
        (
            response.ok,
            response.batch.is_some(),
            response.error.is_some()
        ),
        (true, true, false) | (false, false, true)
    );
    if valid {
        Ok(())
    } else {
        Err(ProtocolError::new(
            ProtocolErrorCode::InvalidRequest,
            "event response batch/error fields are inconsistent",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentinel_events::{
        DataCompleteness, EtwEventInfo, EventSource, FieldState, IdentityCertainty, PairingStatus,
        ProcessEvent, ProcessEventType, ProcessIdentity, SessionEnd, SessionEndReason,
    };

    fn sample_event(sequence: u64) -> ProcessEvent {
        ProcessEvent {
            event_id: format!("session:{sequence:016x}"),
            sequence,
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
            pairing: PairingStatus::Waiting,
        }
    }

    fn sample_batch(count: usize) -> EventBatch {
        let records: Vec<EventEnvelope> = (1..=count as u64)
            .map(|cursor| EventEnvelope {
                cursor,
                record: PipelineRecord::ProcessEvent(sample_event(cursor)),
            })
            .collect();
        EventBatch {
            session_id: "session".to_owned(),
            oldest_cursor: 1,
            latest_cursor: count as u64,
            next_cursor: count as u64,
            cursor_expired: false,
            missed_records: 0,
            cursor_ahead: false,
            evictions: 0,
            session_closed: false,
            has_more: false,
            records,
        }
    }

    #[test]
    fn event_request_round_trips() {
        let request = EventRequest::new(9, 32);
        let bytes = encode_event_request(&request).expect("request should encode");
        assert_eq!(
            decode_event_request(&bytes).expect("request should decode"),
            request
        );
    }

    #[test]
    fn unsupported_event_version_is_rejected() {
        let bytes = serde_json::to_vec(&serde_json::json!({
            "version": EVENT_PROTOCOL_VERSION + 1,
            "request": "events",
            "after": 0,
            "limit": 16
        }))
        .unwrap();
        let error = decode_event_request(&bytes).expect_err("wrong version must fail");
        assert_eq!(error.code, ProtocolErrorCode::UnsupportedVersion);
    }

    #[test]
    fn out_of_range_batch_limit_is_rejected() {
        for limit in [0, MAX_EVENT_BATCH_LIMIT + 1] {
            let request = EventRequest::new(0, limit);
            let error = encode_event_request(&request).expect_err("limit must be rejected");
            assert_eq!(error.code, ProtocolErrorCode::InvalidRequest);
        }
    }

    #[test]
    fn event_response_round_trips() {
        let batch = sample_batch(2);
        let bytes = encode_event_response(&EventResponse::success(batch.clone())).unwrap();
        let decoded = decode_event_response(&bytes).unwrap();
        assert!(decoded.ok);
        assert_eq!(decoded.batch, Some(batch));
    }

    #[test]
    fn event_batch_is_truncated_to_fit_the_response_limit() {
        let mut batch = sample_batch(MAX_EVENT_BATCH_LIMIT as usize);
        for envelope in &mut batch.records {
            if let PipelineRecord::ProcessEvent(event) = &mut envelope.record {
                event.image_name = Some("测".repeat(1024));
            }
        }
        let bytes = encode_event_batch(batch).expect("batch should fit after truncation");
        assert!(bytes.len() <= MAX_EVENT_RESPONSE_BYTES);
        let decoded = decode_event_response(&bytes).unwrap();
        let batch = decoded.batch.unwrap();
        assert!(!batch.records.is_empty());
        assert!(batch.has_more);
        assert_eq!(batch.next_cursor, batch.records.last().unwrap().cursor);
    }

    #[test]
    fn session_end_record_round_trips() {
        let record = PipelineRecord::SessionEnd(SessionEnd {
            session_id: "session".to_owned(),
            ended_at: Some("2026-10-31T00:00:01.000Z".to_owned()),
            reason: SessionEndReason::Completed,
            error: None,
            open_start_events: Vec::new(),
            final_stats: None,
            pairing: sentinel_events::PairingSnapshot {
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
                stops_seen: 0,
                observed_start_image_names: Vec::new(),
                observed_start_image_names_truncated: false,
            },
        });
        let batch = EventBatch {
            session_id: "session".to_owned(),
            oldest_cursor: 1,
            latest_cursor: 1,
            next_cursor: 1,
            cursor_expired: false,
            missed_records: 0,
            cursor_ahead: false,
            evictions: 0,
            session_closed: true,
            has_more: false,
            records: vec![EventEnvelope { cursor: 1, record }],
        };
        let bytes = encode_event_response(&EventResponse::success(batch.clone())).unwrap();
        let decoded = decode_event_response(&bytes).unwrap();
        assert_eq!(decoded.batch, Some(batch));
    }
}
