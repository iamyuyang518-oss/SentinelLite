//! Non-admin read-only software inventory probe used for real-machine verification.
//!
//! This example only calls `scan_installed_software`; it does not create, modify,
//! uninstall, or execute any software or uninstall command.

use std::time::Instant;

use serde_json::json;

fn main() {
    let started = Instant::now();
    let result = sentinel_core::software::scan_installed_software();
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
    let text = serde_json::to_string_pretty(&payload).expect("serialize software probe");
    if let Some(path) = output_path() {
        std::fs::write(&path, text.as_bytes()).expect("write software probe output");
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
