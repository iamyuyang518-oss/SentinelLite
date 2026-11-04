//! Non-admin read-only startup inventory probe used for real-machine verification.
//!
//! This example only calls `scan_startup_items`; it does not create, modify,
//! disable, or execute any startup entry.

use std::time::Instant;

use serde_json::json;

fn main() {
    let started = Instant::now();
    let result = sentinel_core::startup::scan_startup_items();
    let elapsed_ms = started.elapsed().as_millis();
    let payload = json!({
        "elapsedMs": elapsed_ms,
        "observation": result.observation,
        "truncated": result.truncated,
        "sourceCount": result.sources.len(),
        "itemCount": result.items.len(),
        "issueCount": result.issues.len(),
        "sources": result.sources,
        "items": result.items,
        "issues": result.issues,
        "limitations": result.limitations,
    });
    let text = serde_json::to_string_pretty(&payload).expect("serialize startup probe");
    if let Some(path) = output_path() {
        std::fs::write(&path, text.as_bytes()).expect("write startup probe output");
        eprintln!("wrote {}", path.display());
    } else {
        println!("{text}");
    }
}

fn output_path() -> Option<std::path::PathBuf> {
    let mut arguments = std::env::args_os().skip(1);
    while let Some(argument) = arguments.next() {
        if argument == "--output" {
            return arguments.next().map(std::path::PathBuf::from);
        }
    }
    None
}
