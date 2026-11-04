//! End-to-end Windows test: ETW decode model -> bounded pipeline -> delivery
//! broker -> event named pipe -> production client policy.

use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sentinel_events::{
    PairingOrder, PairingStatus, PipelineRecord, ProcessEventType, SessionEndReason,
};
use sentinel_ipc::client::{ClientOptions, query_events};
use sentinel_ipc::event_server::{EventProvider, EventServer, EventServerConfig};
use sentinel_sensor::decode::{
    DecodedProcessEvent, EVENT_PROCESS_START, EVENT_PROCESS_STOP, FieldState,
};
use sentinel_sensor::delivery::{BrokerSink, EventBroker, SESSION_END_DELIVERY_GRACE};
use sentinel_sensor::event::SessionEndReason as SensorSessionEndReason;
use sentinel_sensor::health::SensorHealth;
use sentinel_sensor::pair::PairingState;
use sentinel_sensor::pipeline::{EventPipeline, PipelineConfig};

fn unique_pipe_name() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!(
        r"\\.\pipe\SentinelLite.Test.SensorEvents.{}.{}",
        std::process::id(),
        nanos
    )
}

fn decoded(event_id: u16, pid: u32, create_time: u64, sequence: u64) -> DecodedProcessEvent {
    DecodedProcessEvent {
        event_id,
        version: 4,
        pid,
        create_time,
        parent_pid: Some(4),
        sequence: Some(sequence),
        image_name: Some(format!(r"C:\Windows\System32\test-{pid}.exe")),
        image_name_state: FieldState::Present,
        partial: false,
    }
}

#[test]
fn normalized_events_revisions_and_session_end_cross_the_event_pipe() {
    let session_id = "sensor-session-a".to_owned();
    let broker = EventBroker::new(session_id.clone(), 256);
    let health = Arc::new(SensorHealth::new());
    let pairing = Arc::new(Mutex::new(PairingState::new()));
    let pipeline = EventPipeline::start_with_sink_and_session(
        PipelineConfig::new(1024),
        Arc::clone(&health),
        Arc::clone(&pairing),
        session_id.clone(),
        Box::new(BrokerSink::new(Arc::clone(&broker))),
    );

    // A normal in-order pair.
    pipeline.push_process(decoded(EVENT_PROCESS_START, 100, 1_000, 1), Some(1));
    pipeline.push_process(decoded(EVENT_PROCESS_STOP, 100, 1_000, 2), Some(2));
    // A stop before its start produces an out-of-order pair and a revision.
    pipeline.push_process(decoded(EVENT_PROCESS_STOP, 200, 2_000, 3), Some(3));
    pipeline.push_process(decoded(EVENT_PROCESS_START, 200, 2_000, 4), Some(4));

    let outcome = pipeline.close(SensorSessionEndReason::Completed);
    assert!(outcome.drained, "pipeline should drain before close");

    let provider: Arc<dyn EventProvider> = broker.clone();
    let pipe_name = unique_pipe_name();
    let mut server = EventServer::start(EventServerConfig::for_testing(&pipe_name), provider)
        .expect("event server should start");

    let batch =
        query_events(&pipe_name, &ClientOptions::for_testing(), 0, 128).expect("event read");
    assert_eq!(batch.session_id, session_id);
    assert!(batch.session_closed);
    assert!(!batch.cursor_expired);
    assert_eq!(
        batch.records.len(),
        6,
        "4 process events + 1 pairing revision + session end"
    );

    let mut start_pair = None;
    let mut out_of_order = None;
    let mut revision = None;
    let mut session_end = None;
    for envelope in &batch.records {
        match &envelope.record {
            PipelineRecord::ProcessEvent(event) => {
                if event.identity.pid == 100 && event.event_type == ProcessEventType::Stopped {
                    start_pair = Some(event.pairing.clone());
                }
                if event.identity.pid == 200 && event.event_type == ProcessEventType::Started {
                    out_of_order = Some(event.pairing.clone());
                }
            }
            PipelineRecord::PairingRevision(value) => revision = Some(value.clone()),
            PipelineRecord::SessionEnd(value) => session_end = Some(value.clone()),
            PipelineRecord::InvalidEvent(_) => panic!("unexpected invalid event"),
        }
    }
    assert!(matches!(
        start_pair,
        Some(PairingStatus::Paired {
            order: PairingOrder::InOrder,
            ..
        })
    ));
    assert!(matches!(
        out_of_order,
        Some(PairingStatus::Paired {
            order: PairingOrder::OutOfOrder,
            ..
        })
    ));
    let revision = revision.expect("out-of-order pair must deliver a revision");
    assert!(revision.stop_event_id.ends_with("0000000000000003"));
    assert!(revision.start_event_id.ends_with("0000000000000004"));
    let session_end = session_end.expect("session end must be delivered");
    assert_eq!(session_end.reason, SessionEndReason::Completed);
    assert_eq!(session_end.session_id, session_id);

    broker.acknowledge(batch.next_cursor);
    assert!(broker.wait_for_session_end_delivery(SESSION_END_DELIVERY_GRACE));
    server.stop().expect("event server should stop");

    // A restarted sensor has a new session and cursor space; a stale cursor is
    // explicitly ahead rather than being silently reinterpreted.
    let restarted = EventBroker::new("sensor-session-b", 256);
    restarted.append(&sentinel_events::PipelineRecord::SessionEnd(
        sentinel_events::SessionEnd {
            session_id: "sensor-session-b".to_owned(),
            ended_at: None,
            reason: SessionEndReason::Interrupted,
            error: None,
            open_start_events: Vec::new(),
            final_stats: None,
            pairing: sentinel_events::PairingSnapshot {
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
        },
    ));
    let stale = <EventBroker as EventProvider>::batch(&restarted, batch.next_cursor, 64)
        .expect("restart batch");
    assert_eq!(stale.session_id, "sensor-session-b");
    assert!(stale.cursor_ahead);
    assert!(stale.records.is_empty());
    assert_eq!(stale.next_cursor, 1);
    std::thread::sleep(Duration::from_millis(10));
}
