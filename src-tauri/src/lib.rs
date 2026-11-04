//! SentinelLite desktop backend.
//!
//! Phase 1 is intentionally read-only. The application does not uninstall software, stop
//! processes, modify the registry, or configure Windows security features.

mod commands;
mod storage_state;

#[cfg(test)]
mod command_names;

use tauri::Manager;

/// Starts the Tauri application.
pub fn run() {
    let app = tauri::Builder::default()
        .setup(|app| {
            let state = storage_state::StorageState::initialize(app.handle());
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_protection_overview,
            commands::get_native_protection_status,
            commands::get_sensor_status,
            commands::get_sensor_events,
            commands::get_storage_status,
            commands::query_stored_events,
            commands::get_stored_event,
            commands::list_sensor_sessions,
            commands::analyze_session,
            commands::scan_installed_software,
            commands::scan_startup_items,
            commands::scan_asset_associations,
            commands::list_services,
            commands::list_scheduled_tasks,
            commands::list_processes,
            commands::inspect_process_affinity,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build SentinelLite");

    app.run(|app_handle, event| {
        if matches!(event, tauri::RunEvent::Exit) {
            app_handle.state::<storage_state::StorageState>().stop();
        }
    });
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::command_names::APP_COMMANDS;

    #[test]
    fn capability_allows_exactly_the_application_commands() {
        let capability: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/default.json")).unwrap();
        let actual: BTreeSet<String> = capability["permissions"]
            .as_array()
            .expect("permissions must be an array")
            .iter()
            .map(|permission| {
                permission
                    .as_str()
                    .expect("permission must be a string")
                    .to_string()
            })
            .collect();
        let expected: BTreeSet<String> = APP_COMMANDS
            .iter()
            .map(|command| format!("allow-{}", command.replace('_', "-")))
            .collect();

        assert_eq!(actual, expected);
        assert!(
            !actual.iter().any(|permission| permission == "core:default"),
            "the UI does not use any core plugin command; application ACL is the boundary"
        );
    }
}
