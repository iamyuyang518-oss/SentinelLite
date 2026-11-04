//! Non-admin read-only asset association probe used for real-machine verification.
//!
//! The probe calls the five existing read-only inventory implementations and the
//! pure association engine. It does not start, stop, modify, delete, or execute
//! any process, service, task, startup item, or software.

use std::time::Instant;

use serde_json::json;

fn main() {
    let started = Instant::now();
    let result = sentinel_core::association::scan_asset_associations();
    let elapsed_ms = started.elapsed().as_millis();
    let payload = json!({
        "elapsedMs": elapsed_ms,
        "observation": result.observation,
        "truncated": result.truncated,
        "sourceCount": result.sources.len(),
        "softwareCount": result.software.len(),
        "relationCount": result.relations.len(),
        "issueCount": result.issues.len(),
        "sources": result.sources,
        "software": result.software,
        "relations": result.relations,
        "issues": result.issues,
        "limitations": result.limitations,
    });
    let text = serde_json::to_string_pretty(&payload).expect("serialize association probe");
    if let Some(path) = output_path() {
        std::fs::write(&path, text.as_bytes()).expect("write association probe output");
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
