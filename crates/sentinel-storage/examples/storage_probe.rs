//! Read-side probe for the local telemetry database.
//!
//! It opens the same database the application uses and prints a JSON summary.
//! It does not inject synthetic events.

use std::path::PathBuf;

use sentinel_storage::{
    EventFilter, RetentionPolicy, Storage, default_database_path, path::DATABASE_FILE_NAME,
};
use serde_json::json;

fn main() {
    let mut database: Option<PathBuf> = None;
    let mut output_path: Option<PathBuf> = None;
    let mut sessions = false;
    let mut limit = 20u32;
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
            "--output" => {
                let Some(path) = arguments.next() else {
                    eprintln!("--output requires a path");
                    std::process::exit(2);
                };
                output_path = Some(PathBuf::from(path));
            }
            "--sessions" => sessions = true,
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

    let path = match database {
        Some(path) => path,
        None => match default_database_path() {
            Ok(path) => path,
            Err(error) => {
                eprintln!("database path unavailable: {error}");
                std::process::exit(1);
            }
        },
    };
    if path.file_name().and_then(|name| name.to_str()) != Some(DATABASE_FILE_NAME) {
        eprintln!("warning: database file is not {DATABASE_FILE_NAME}");
    }

    let storage = match Storage::open(&path, RetentionPolicy::default()) {
        Ok(storage) => storage,
        Err(error) => {
            eprintln!("storage open failed: {error}");
            std::process::exit(1);
        }
    };
    let status = storage.status();
    let events = storage.query_events(&EventFilter {
        limit: Some(limit.clamp(1, 200)),
        ..EventFilter::default()
    });
    let session_list = if sessions {
        storage.list_sessions(50)
    } else {
        Ok(Vec::new())
    };
    let output = json!({
        "database": path.to_string_lossy(),
        "status": status.as_ref().ok(),
        "statusError": status.as_ref().err().map(ToString::to_string),
        "events": events.as_ref().ok(),
        "eventsError": events.as_ref().err().map(ToString::to_string),
        "sessions": session_list.as_ref().ok(),
        "sessionsError": session_list.as_ref().err().map(ToString::to_string),
    });
    let json = match serde_json::to_string_pretty(&output) {
        Ok(json) => json,
        Err(error) => {
            eprintln!("serialization failed: {error}");
            std::process::exit(1);
        }
    };
    if let Some(path) = output_path {
        if let Err(error) = std::fs::write(&path, format!("{json}\n")) {
            eprintln!("output write failed: {error}");
            std::process::exit(1);
        }
    }
    println!("{json}");
}
