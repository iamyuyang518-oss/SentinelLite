//! Optional read-only smoke test against the real host.
//!
//! Ignored by default because it depends on the machine's installed software and process state.
//! It performs no write, uninstall, process control, or network operation.
//!
//! Run with:
//! `cargo test -p sentinel-core --test readonly_host_smoke -- --ignored --nocapture`

#[test]
#[ignore = "reads the real host; run manually for read-only smoke verification"]
fn read_only_scans_complete_and_serialize() {
    let software = sentinel_core::software::scan_installed_software();
    assert!(!software.checked_at.is_empty());
    assert!(!software.limitations.is_empty());
    serde_json::to_string(&software).expect("software result must serialize");
    println!(
        "software observation: {:?}, sources: {}, entries: {}, issues: {}, truncated: {}",
        software.observation,
        software.sources.len(),
        software.items.len(),
        software.issues.len(),
        software.truncated
    );

    let startup = sentinel_core::startup::scan_startup_items();
    assert!(!startup.checked_at.is_empty());
    assert!(!startup.limitations.is_empty());
    serde_json::to_string(&startup).expect("startup result must serialize");
    println!(
        "startup observation: {:?}, sources: {}, entries: {}, issues: {}, truncated: {}",
        startup.observation,
        startup.sources.len(),
        startup.items.len(),
        startup.issues.len(),
        startup.truncated
    );
    for issue in &startup.issues {
        println!(
            "startup issue: {} [{}] {}",
            issue.code, issue.source, issue.message
        );
    }

    let processes = sentinel_core::process::list_processes().expect("process snapshot");
    assert!(!processes.limitations.is_empty());
    serde_json::to_string(&processes).expect("process result must serialize");
    println!(
        "processes: {}, path access denied: {}, truncated: {}, issues: {}",
        processes.processes.len(),
        processes
            .processes
            .iter()
            .filter(|process| process.path_access
                == sentinel_core::model::ProcessPathAccess::Denied)
            .count(),
        processes.truncated,
        processes.issues.len()
    );

    let self_pid = std::process::id();
    let self_summary = processes
        .processes
        .iter()
        .find(|process| process.pid == self_pid)
        .expect("self process must be visible");
    let identity = self_summary
        .identity
        .clone()
        .expect("self process must have a creation-time identity");
    let inspection = sentinel_core::process::inspect_process_affinity(self_pid, &identity)
        .expect("self inspection");
    assert_eq!(inspection.pid, self_pid);
    assert!(!inspection.limitations.is_empty());
    serde_json::to_string(&inspection).expect("affinity inspection must serialize");
    println!(
        "self inspection state: {:?}, affinity restricted: {:?}, affinity risk: {:?}, overall risk: {:?}, signature: {:?}, hash: {:?}, cpu: {:?}",
        inspection.inspection_state,
        inspection.affinity.as_ref().map(|value| value.restricted),
        inspection.affinity_risk.level,
        inspection.overall_risk.level,
        inspection.signature.state,
        inspection.signature.file_evidence.hash_state,
        inspection
            .cpu_usage
            .as_ref()
            .map(|value| value.percent_of_total_capacity)
    );
}
