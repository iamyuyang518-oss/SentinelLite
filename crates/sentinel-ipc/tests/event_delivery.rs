//! Windows integration tests for the event-delivery named pipe.

use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sentinel_ipc::client::{ClientOptions, query_events};
use sentinel_ipc::event_server::{EventProvider, EventServer, EventServerConfig};
use sentinel_ipc::events::EventBatch;
use sentinel_ipc::server::ServerError;

struct FakeProvider {
    batches: Mutex<Vec<EventBatch>>,
    acknowledgements: Mutex<Vec<u64>>,
}

impl EventProvider for FakeProvider {
    fn batch(&self, after: u64, _limit: u16) -> Result<EventBatch, String> {
        let batches = self.batches.lock().unwrap();
        let batch = batches
            .iter()
            .find(|batch| batch.next_cursor >= after)
            .cloned()
            .unwrap_or(EventBatch {
                session_id: "session".to_owned(),
                oldest_cursor: 0,
                latest_cursor: 0,
                next_cursor: after,
                cursor_expired: false,
                missed_records: 0,
                cursor_ahead: false,
                evictions: 0,
                session_closed: false,
                has_more: false,
                records: Vec::new(),
            });
        Ok(batch)
    }

    fn acknowledge(&self, cursor: u64) {
        self.acknowledgements.lock().unwrap().push(cursor);
    }
}

fn unique_pipe_name() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!(
        r"\\.\pipe\SentinelLite.Test.Events.{}.{}",
        std::process::id(),
        nanos
    )
}

fn empty_batch(next_cursor: u64) -> EventBatch {
    EventBatch {
        session_id: "session".to_owned(),
        oldest_cursor: 0,
        latest_cursor: 0,
        next_cursor,
        cursor_expired: false,
        missed_records: 0,
        cursor_ahead: false,
        evictions: 0,
        session_closed: false,
        has_more: false,
        records: Vec::new(),
    }
}

#[test]
fn event_pipe_serves_sequential_clients_and_acknowledges() {
    let pipe_name = unique_pipe_name();
    let provider = Arc::new(FakeProvider {
        batches: Mutex::new(vec![empty_batch(0), empty_batch(7)]),
        acknowledgements: Mutex::new(Vec::new()),
    });
    let mut server =
        EventServer::start(EventServerConfig::for_testing(&pipe_name), provider.clone())
            .expect("event server should start");

    let options = ClientOptions::for_testing();
    let first = query_events(&pipe_name, &options, 0, 16).expect("first read");
    assert_eq!(first.next_cursor, 0);
    let second = query_events(&pipe_name, &options, 7, 16).expect("second read");
    assert_eq!(second.next_cursor, 7);

    server.stop().expect("event server should stop");
    assert!(provider.acknowledgements.lock().unwrap().contains(&7));
}

#[test]
fn missing_event_endpoint_reports_not_running() {
    let error = query_events(
        &format!(
            r"\\.\pipe\SentinelLite.Test.Events.Missing.{}",
            std::process::id()
        ),
        &ClientOptions::for_testing(),
        0,
        16,
    )
    .expect_err("missing endpoint must fail");
    assert!(matches!(
        error,
        sentinel_ipc::client::ClientError::NotRunning
    ));
}

#[test]
fn event_server_startup_error_is_reported() {
    let error = EventServer::start(
        EventServerConfig::for_testing(unique_pipe_name()).with_startup_error_for_testing(
            ServerError::Config("injected event startup failure".to_owned()),
        ),
        Arc::new(FakeProvider {
            batches: Mutex::new(Vec::new()),
            acknowledgements: Mutex::new(Vec::new()),
        }),
    )
    .expect_err("startup error must fail");
    assert!(error.to_string().contains("injected event startup failure"));
}

#[test]
fn event_server_stop_is_prompt() {
    let pipe_name = unique_pipe_name();
    let provider = Arc::new(FakeProvider {
        batches: Mutex::new(Vec::new()),
        acknowledgements: Mutex::new(Vec::new()),
    });
    let mut server =
        EventServer::start(EventServerConfig::for_testing(&pipe_name), provider).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    server.stop().expect("stop should succeed");
}
