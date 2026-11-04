//! Manual probe for a real administrator-run sensor. This uses the same
//! production client policy as the Tauri backend.
//!
//! Default mode prints the sensor status, preserving the Phase 2.2/2.3 probe
//! contract. `--events` reads one event batch; `--watch-events SECONDS` polls
//! until `SessionEnd` is observed or the watch window expires.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use sentinel_events::PipelineRecord;
use sentinel_ipc::client::{ClientOptions, query_events, query_status};
use sentinel_ipc::{DEFAULT_EVENT_PIPE_NAME, DEFAULT_PIPE_NAME};
use serde::Serialize;

const WATCH_POLL: Duration = Duration::from_millis(200);

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct EventWatch {
    session_id: Option<String>,
    queries: u64,
    process_events: u64,
    pairing_revisions: u64,
    invalid_events: u64,
    session_end_records: u64,
    session_end_reason: Option<String>,
    records: u64,
    next_cursor: u64,
    cursor_expired: u64,
    missed_records: u64,
    evictions: u64,
    session_closed: bool,
    session_restarts: u64,
    cursor_resets: u64,
    transient_errors: u64,
    last_error: Option<String>,
    paired_started: u64,
    paired_stopped: u64,
    unpaired_started: u64,
    unpaired_stopped: u64,
}

impl EventWatch {
    fn observe(&mut self, batch: &sentinel_ipc::events::EventBatch) {
        self.session_id = Some(batch.session_id.clone());
        self.cursor_expired += u64::from(batch.cursor_expired);
        self.missed_records = self.missed_records.saturating_add(batch.missed_records);
        self.evictions = batch.evictions;
        self.session_closed = batch.session_closed;
        self.records += batch.records.len() as u64;
        self.next_cursor = batch.next_cursor;
        for envelope in &batch.records {
            match &envelope.record {
                PipelineRecord::ProcessEvent(event) => {
                    self.process_events += 1;
                    let paired =
                        matches!(event.pairing, sentinel_events::PairingStatus::Paired { .. });
                    match (event.event_type, paired) {
                        (sentinel_events::ProcessEventType::Started, true) => {
                            self.paired_started += 1;
                        }
                        (sentinel_events::ProcessEventType::Stopped, true) => {
                            self.paired_stopped += 1;
                        }
                        (sentinel_events::ProcessEventType::Started, false) => {
                            self.unpaired_started += 1;
                        }
                        (sentinel_events::ProcessEventType::Stopped, false) => {
                            self.unpaired_stopped += 1;
                        }
                    }
                }
                PipelineRecord::PairingRevision(_) => self.pairing_revisions += 1,
                PipelineRecord::InvalidEvent(_) => self.invalid_events += 1,
                PipelineRecord::SessionEnd(end) => {
                    self.session_end_records += 1;
                    self.session_end_reason = serde_json::to_value(end.reason)
                        .ok()
                        .and_then(|value| value.as_str().map(ToOwned::to_owned));
                }
            }
        }
    }
}

fn main() {
    let mut output: Option<PathBuf> = None;
    let mut watch_seconds: Option<u64> = None;
    let mut one_batch = false;
    let mut after = 0u64;
    let mut limit = sentinel_ipc::events::DEFAULT_EVENT_BATCH_LIMIT;
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--output" => {
                let Some(path) = arguments.next() else {
                    eprintln!("--output requires a path");
                    std::process::exit(2);
                };
                output = Some(PathBuf::from(path));
            }
            "--events" => one_batch = true,
            "--watch-events" => {
                let Some(value) = arguments.next() else {
                    eprintln!("--watch-events requires seconds");
                    std::process::exit(2);
                };
                watch_seconds = Some(value.parse().unwrap_or_else(|_| {
                    eprintln!("--watch-events must be an integer");
                    std::process::exit(2);
                }));
            }
            "--after" => {
                let Some(value) = arguments.next() else {
                    eprintln!("--after requires a cursor");
                    std::process::exit(2);
                };
                after = value.parse().unwrap_or_else(|_| {
                    eprintln!("--after must be an integer");
                    std::process::exit(2);
                });
            }
            "--limit" => {
                let Some(value) = arguments.next() else {
                    eprintln!("--limit requires a count");
                    std::process::exit(2);
                };
                limit = value.parse().unwrap_or_else(|_| {
                    eprintln!("--limit must be an integer");
                    std::process::exit(2);
                });
            }
            other => {
                eprintln!("unknown argument: {other}");
                std::process::exit(2);
            }
        }
    }

    let options = ClientOptions::secure();
    let status = match query_status(DEFAULT_PIPE_NAME, &options) {
        Ok(status) => status,
        Err(error) => {
            eprintln!("probe failed: {error}");
            std::process::exit(1);
        }
    };

    let watch = if let Some(seconds) = watch_seconds {
        Some(watch_events(seconds, after, limit, &options))
    } else if one_batch {
        match query_events(DEFAULT_EVENT_PIPE_NAME, &options, after, limit) {
            Ok(batch) => {
                let mut watch = EventWatch {
                    queries: 1,
                    ..EventWatch::default()
                };
                watch.observe(&batch);
                Some(watch)
            }
            Err(error) => {
                eprintln!("event probe failed: {error}");
                std::process::exit(1);
            }
        }
    } else {
        None
    };

    let output_value = match watch {
        Some(watch) => serde_json::json!({ "status": status, "events": watch }),
        None => serde_json::json!({ "status": status }),
    };
    let json = match serde_json::to_string_pretty(&output_value) {
        Ok(json) => json,
        Err(error) => {
            eprintln!("probe serialization failed: {error}");
            std::process::exit(1);
        }
    };
    if let Some(path) = output {
        if let Err(error) = std::fs::write(&path, format!("{json}\n")) {
            eprintln!("probe output write failed: {error}");
            std::process::exit(1);
        }
    }
    println!("{json}");
}

fn watch_events(seconds: u64, after: u64, limit: u16, options: &ClientOptions) -> EventWatch {
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let mut watch = EventWatch::default();
    let mut cursor = after;
    let mut session_id: Option<String> = None;
    while Instant::now() < deadline {
        match query_events(DEFAULT_EVENT_PIPE_NAME, options, cursor, limit) {
            Ok(batch) => {
                watch.queries += 1;
                if session_id
                    .as_ref()
                    .is_some_and(|known| known != &batch.session_id)
                {
                    watch.session_restarts += 1;
                    cursor = 0;
                }
                if batch.cursor_ahead {
                    watch.cursor_resets += 1;
                    cursor = 0;
                }
                session_id = Some(batch.session_id.clone());
                let ended = batch
                    .records
                    .iter()
                    .any(|envelope| matches!(envelope.record, PipelineRecord::SessionEnd(_)));
                watch.observe(&batch);
                if !batch.cursor_ahead {
                    cursor = batch.next_cursor;
                }
                if ended && batch.session_closed {
                    break;
                }
            }
            Err(error) => {
                watch.transient_errors += 1;
                watch.last_error = Some(error.to_string());
                if matches!(error, sentinel_ipc::client::ClientError::NotRunning)
                    && watch.session_closed
                {
                    break;
                }
            }
        }
        std::thread::sleep(WATCH_POLL);
    }
    watch
}
