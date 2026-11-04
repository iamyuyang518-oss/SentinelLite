//! Read-only real-machine probe for services and scheduled tasks.
//!
//! Usage:
//!   cargo run -p sentinel-core --example inventory_probe
//!   cargo run -p sentinel-core --example inventory_probe -- --output <path>

use std::time::Instant;

use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InventoryProbe {
    services_elapsed_ms: u128,
    tasks_elapsed_ms: u128,
    services: sentinel_core::model::ServiceInventoryResult,
    tasks: sentinel_core::model::ScheduledTaskInventoryResult,
}

fn main() {
    let services_started = Instant::now();
    let services = sentinel_core::services::list_services();
    let services_elapsed_ms = services_started.elapsed().as_millis();

    let tasks_started = Instant::now();
    let tasks = sentinel_core::scheduled_tasks::list_scheduled_tasks();
    let tasks_elapsed_ms = tasks_started.elapsed().as_millis();

    let probe = InventoryProbe {
        services_elapsed_ms,
        tasks_elapsed_ms,
        services,
        tasks,
    };
    let json = serde_json::to_string_pretty(&probe).expect("inventory is serializable");

    let mut args = std::env::args().skip(1);
    if let Some(flag) = args.next() {
        if flag == "--output" {
            let path = args.next().expect("--output requires a path");
            std::fs::write(&path, format!("{json}\n")).expect("write probe output");
            println!("wrote {path}");
            return;
        }
    }

    println!("{json}");
}
