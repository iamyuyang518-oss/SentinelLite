//! Verification probe for the stored-event analysis chain.
//!
//! It opens the application database, loads one session, and runs the same
//! conservative rules used by the Tauri backend. Sensor health is treated as
//! unavailable unless the caller supplies it; that is intentional for a
//! historical session.

use std::path::PathBuf;

use sentinel_detection::{AnalysisConfig, DetectionContext, SessionDataInput, analyze};
use sentinel_storage::{RetentionPolicy, Storage};
use serde_json::json;

fn main() {
    let mut database: Option<PathBuf> = None;
    let mut session_id: Option<String> = None;
    let mut output_path: Option<PathBuf> = None;
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--database" => {
                let Some(path) = arguments.next() else {
                    eprintln!("--database requires a path");
                    std::process::exit(2);
                };
                database = Some(PathBuf::from(path));
            }
            "--session" => {
                let Some(value) = arguments.next() else {
                    eprintln!("--session requires an id");
                    std::process::exit(2);
                };
                session_id = Some(value);
            }
            "--output" => {
                let Some(path) = arguments.next() else {
                    eprintln!("--output requires a path");
                    std::process::exit(2);
                };
                output_path = Some(PathBuf::from(path));
            }
            other => {
                eprintln!("unknown argument: {other}");
                std::process::exit(2);
            }
        }
    }

    let path = database.unwrap_or_else(|| {
        eprintln!("--database is required");
        std::process::exit(2);
    });
    let storage = match Storage::open(&path, RetentionPolicy::default()) {
        Ok(storage) => storage,
        Err(error) => {
            eprintln!("storage open failed: {error}");
            std::process::exit(1);
        }
    };
    let session_id = match session_id {
        Some(session_id) => session_id,
        None => match storage.list_sessions(1) {
            Ok(sessions) if !sessions.is_empty() => sessions[0].session_id.clone(),
            Ok(_) => {
                eprintln!("no persisted sessions");
                std::process::exit(1);
            }
            Err(error) => {
                eprintln!("session list failed: {error}");
                std::process::exit(1);
            }
        },
    };
    let set = match storage.load_session_for_analysis(&session_id, 20_000) {
        Ok(Some(set)) => set,
        Ok(None) => {
            eprintln!("session not found: {session_id}");
            std::process::exit(1);
        }
        Err(error) => {
            eprintln!("session load failed: {error}");
            std::process::exit(1);
        }
    };
    let session_data = set
        .session_end
        .as_ref()
        .and_then(|end| end.final_stats.as_ref())
        .map(|stats| SessionDataInput {
            etw_events_lost: stats.events_lost,
            realtime_buffers_lost: stats.realtime_buffers_lost,
            pipeline_dropped: stats.pipeline_dropped_total(),
            pipeline_invalid: stats.pipeline_invalid,
            pipeline_backlog: stats.pipeline_backlog,
            pipeline_shutdown_timed_out: stats.pipeline_shutdown_timed_out,
        });
    let context = DetectionContext {
        session_id: set.session_id.clone(),
        session_closed: set.closed,
        session_end_reason: set.end_reason.clone(),
        invalid_records: set.invalid_records,
        ingestion_gaps: set.ingestion_gaps,
        truncated: set.truncated,
        // Historical probe: the sensor may no longer be running. Final
        // SessionEnd statistics are used when present; otherwise the engine
        // reports unknown ETW loss instead of inventing zero.
        sensor: None,
        session_data,
    };
    let analysis = analyze(&set.events, &context, &AnalysisConfig::default());
    let output_value = json!({
        "session_id": analysis.session_id,
        "event_count": set.events.len(),
        "analysis": analysis,
    });
    let json = match serde_json::to_string_pretty(&output_value) {
        Ok(json) => json,
        Err(error) => {
            eprintln!("serialization failed: {error}");
            std::process::exit(1);
        }
    };
    if let Some(path) = output_path {
        if let Err(error) = std::fs::write(&path, format!("{json}\n")) {
            eprintln!("analysis output write failed: {error}");
            std::process::exit(1);
        }
    }
    println!("{json}");
}
