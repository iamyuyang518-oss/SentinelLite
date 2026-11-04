use std::time::Duration;

use sentinel_core::model::{
    PathState, ProtectionState, RiskLevel, RiskScope, SensorState, SignatureState,
    SoftwareClassification, StartupPathResolution,
};
use sentinel_core::overview::protection_overview;
use sentinel_core::rules::{
    AffinityFacts, SoftwareFacts, StartupFacts, assess_affinity, assess_software, assess_startup,
};
use sentinel_core::util::{
    ParsedCommandPath, calculate_cpu_percent, filetime_ticks_to_rfc3339, is_affinity_restricted,
    is_local_absolute_path, is_temp_path, mask_to_cpu_indices, normalize_install_date,
    parse_startup_command,
};

#[test]
fn overview_never_claims_protection_when_no_sensor_is_active() {
    let overview = protection_overview();

    assert_eq!(overview.overall_state, ProtectionState::Unknown);
    assert!(
        overview
            .sensors
            .iter()
            .all(|sensor| sensor.state != SensorState::Active),
        "phase 1 has no active continuous sensor"
    );
    assert!(
        overview
            .sensors
            .iter()
            .any(|sensor| sensor.state == SensorState::NotImplemented)
    );
    assert!(
        overview
            .read_only_checks
            .iter()
            .any(|check| check.id == "softwareInventory")
    );
    assert!(overview.boundaries.iter().any(|line| line.contains("实时")));
}

#[test]
fn interfaces_serialize_with_stable_camel_case_names() {
    let startup = assess_startup(StartupFacts {
        target_path: Some(r"C:\Users\Example\AppData\Local\Temp\helper.exe"),
        target_state: PathState::Present,
        path_resolution: StartupPathResolution::QuotedExecutable,
    });
    let value = serde_json::to_value(startup).expect("risk assessment must serialize");

    assert_eq!(value["level"], "potentialUnwanted");
    assert!(value["ruleIds"].is_array());
    assert!(value["blindSpots"].is_array());
}

#[test]
fn software_without_uninstall_entry_is_a_configuration_anomaly_not_a_virus() {
    let risk = assess_software(SoftwareFacts {
        uninstall_command_present: false,
        uninstall_command_known: true,
        install_location_state: PathState::Present,
        classification: SoftwareClassification::UserSoftware,
    });

    assert_eq!(risk.level, RiskLevel::ConfigurationAnomaly);
    assert!(risk.rule_ids.iter().any(|rule| rule == "INV-001"));
    assert!(!risk.summary.contains("病毒"));
    assert!(!risk.evidence.is_empty());
}

#[test]
fn inaccessible_install_location_adds_evidence_without_claiming_malice() {
    let risk = assess_software(SoftwareFacts {
        uninstall_command_present: true,
        uninstall_command_known: true,
        install_location_state: PathState::Missing,
        classification: SoftwareClassification::UserSoftware,
    });

    assert_eq!(risk.level, RiskLevel::ConfigurationAnomaly);
    assert!(risk.rule_ids.iter().any(|rule| rule == "INV-002"));
    assert_eq!(risk.scope, RiskScope::SoftwareMetadata);
    assert!(!risk.evidence.is_empty());
}

#[test]
fn ordinary_software_metadata_is_reported_as_no_clear_issue_with_caveats() {
    let risk = assess_software(SoftwareFacts {
        uninstall_command_present: true,
        uninstall_command_known: true,
        install_location_state: PathState::Present,
        classification: SoftwareClassification::UserSoftware,
    });

    assert_eq!(risk.level, RiskLevel::NoClearIssue);
    assert_eq!(risk.scope, RiskScope::SoftwareMetadata);
    assert_eq!(risk.confidence, sentinel_core::model::Confidence::Low);
    assert!(!risk.blind_spots.is_empty());
}

#[test]
fn unresolved_startup_target_is_insufficient_evidence() {
    let risk = assess_startup(StartupFacts {
        target_path: None,
        target_state: PathState::Unknown,
        path_resolution: StartupPathResolution::Unresolved,
    });

    assert_eq!(risk.level, RiskLevel::InsufficientEvidence);
    assert!(risk.rule_ids.iter().any(|rule| rule == "STARTUP-001"));
}

#[test]
fn missing_startup_target_is_a_configuration_anomaly() {
    let risk = assess_startup(StartupFacts {
        target_path: Some(r"C:\Program Files\Example\missing.exe"),
        target_state: PathState::Missing,
        path_resolution: StartupPathResolution::QuotedExecutable,
    });

    assert_eq!(risk.level, RiskLevel::ConfigurationAnomaly);
    assert!(risk.rule_ids.iter().any(|rule| rule == "STARTUP-002"));
}

#[test]
fn startup_target_in_temp_is_potentially_unwanted_but_unconfirmed() {
    let risk = assess_startup(StartupFacts {
        target_path: Some(r"C:\Users\Example\AppData\Local\Temp\helper.exe"),
        target_state: PathState::Present,
        path_resolution: StartupPathResolution::QuotedExecutable,
    });

    assert_eq!(risk.level, RiskLevel::PotentialUnwanted);
    assert!(risk.rule_ids.iter().any(|rule| rule == "STARTUP-003"));
    assert!(!risk.blind_spots.is_empty());
}

#[test]
fn ordinary_startup_target_is_not_upgraded_to_a_threat() {
    let risk = assess_startup(StartupFacts {
        target_path: Some(r"C:\Program Files\Example\app.exe"),
        target_state: PathState::Present,
        path_resolution: StartupPathResolution::QuotedExecutable,
    });

    assert_eq!(risk.level, RiskLevel::NoClearIssue);
    assert_eq!(risk.scope, RiskScope::StartupMetadata);
    assert!(risk.rule_ids.iter().any(|rule| rule == "STARTUP-000"));
}

#[test]
fn affinity_difference_is_an_observed_configuration_not_malice() {
    let risk = assess_affinity(AffinityFacts {
        restricted: Some(true),
        process_mask: Some(0b1111),
        system_mask: Some(0b1111_1111_1111),
    });

    assert_eq!(risk.level, RiskLevel::ConfigurationAnomaly);
    assert!(risk.rule_ids.iter().any(|rule| rule == "AFF-001"));
    assert!(!risk.summary.contains("病毒"));
    assert!(!risk.blind_spots.is_empty());
}

#[test]
fn unreadable_affinity_is_insufficient_evidence() {
    let risk = assess_affinity(AffinityFacts {
        restricted: None,
        process_mask: None,
        system_mask: None,
    });

    assert_eq!(risk.level, RiskLevel::InsufficientEvidence);
    assert!(risk.rule_ids.iter().any(|rule| rule == "AFF-002"));
}

#[test]
fn matching_affinity_scope_does_not_create_a_finding() {
    let risk = assess_affinity(AffinityFacts {
        restricted: Some(false),
        process_mask: Some(0b1111),
        system_mask: Some(0b1111),
    });

    assert_eq!(risk.level, RiskLevel::NoClearIssue);
    assert_eq!(risk.scope, RiskScope::AffinityConfiguration);
    assert!(risk.rule_ids.iter().any(|rule| rule == "AFF-000"));
}

#[test]
fn masks_convert_to_human_readable_cpu_sets() {
    assert_eq!(mask_to_cpu_indices(0b1011), vec![0, 1, 3]);
    assert_eq!(
        mask_to_cpu_indices(1u64 << 63),
        vec![63],
        "the highest bit must not overflow"
    );
    assert!(mask_to_cpu_indices(0).is_empty());
}

#[test]
fn affinity_restriction_requires_a_strict_subset() {
    assert!(is_affinity_restricted(0b11, 0b1111));
    assert!(!is_affinity_restricted(0b1111, 0b1111));
    assert!(!is_affinity_restricted(0, 0b1111));
}

#[test]
fn cpu_percent_uses_total_machine_capacity_and_is_bounded() {
    let percent = calculate_cpu_percent(Duration::from_millis(500), Duration::from_secs(1), 20)
        .expect("valid sample");
    assert!((percent - 2.5).abs() < 0.001);

    let saturated = calculate_cpu_percent(Duration::from_secs(2), Duration::from_secs(1), 1)
        .expect("valid sample");
    assert_eq!(saturated, 100.0);

    assert_eq!(
        calculate_cpu_percent(Duration::ZERO, Duration::ZERO, 0),
        None
    );
}

#[test]
fn executable_parser_handles_quoted_and_unquoted_commands() {
    assert_eq!(
        parse_startup_command(r#"  "C:\Program Files\Example\app.exe" --silent  "#),
        ParsedCommandPath::Quoted(r"C:\Program Files\Example\app.exe".to_string())
    );
    assert_eq!(
        parse_startup_command(r"C:\Windows\System32\cmd.exe /c start"),
        ParsedCommandPath::Unquoted(r"C:\Windows\System32\cmd.exe".to_string())
    );
    assert_eq!(
        parse_startup_command(r"C:\Program Files\Example\app.exe -run"),
        ParsedCommandPath::AmbiguousUnquoted {
            candidate: Some(r"C:\Program Files\Example\app.exe".to_string())
        }
    );
    assert_eq!(parse_startup_command("   "), ParsedCommandPath::Unresolved);
}

#[test]
fn install_dates_are_normalized_when_valid_and_preserved_when_unclear() {
    assert_eq!(
        normalize_install_date(Some("20240131")),
        Some("2024-01-31".to_string())
    );
    assert_eq!(
        normalize_install_date(Some("2024-01")),
        Some("2024-01".to_string())
    );
    assert_eq!(normalize_install_date(Some("  ")), None);
    assert_eq!(normalize_install_date(None), None);
}

#[test]
fn temp_paths_are_detected_case_insensitively() {
    assert!(is_temp_path(
        r"c:\users\example\appdata\local\temp\payload.exe"
    ));
    assert!(is_temp_path(r"C:\Windows\Temp\setup.exe"));
    assert!(!is_temp_path(r"C:\Program Files\Example\app.exe"));
}

#[test]
fn only_local_absolute_paths_are_eligible_for_filesystem_probes() {
    assert!(is_local_absolute_path(r"C:\Program Files\Example\app.exe"));
    assert!(is_local_absolute_path(r"D:\Tools\app.exe"));
    assert!(!is_local_absolute_path(r"\\server\share\app.exe"));
    assert!(!is_local_absolute_path(r"\\?\C:\very-long\app.exe"));
    assert!(!is_local_absolute_path(r"\\.\pipe\example"));
    assert!(!is_local_absolute_path(r"app.exe"));
    assert!(!is_local_absolute_path(r"C:relative\app.exe"));
    assert!(!is_local_absolute_path(""));
}

#[test]
fn windows_filetime_conversion_handles_the_unix_epoch_and_zero() {
    assert_eq!(
        filetime_ticks_to_rfc3339(116_444_736_000_000_000),
        Some("1970-01-01T00:00:00Z".to_string())
    );
    assert_eq!(filetime_ticks_to_rfc3339(0), None);
}

#[test]
fn signature_states_are_serializable_without_collapsing_invalid_into_unsigned() {
    let invalid = serde_json::to_value(SignatureState::Invalid).expect("serialize");
    let unsigned = serde_json::to_value(SignatureState::Unsigned).expect("serialize");

    assert_eq!(invalid, "invalid");
    assert_eq!(unsigned, "unsigned");
    assert_ne!(invalid, unsigned);
}
